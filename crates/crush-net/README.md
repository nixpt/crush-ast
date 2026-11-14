# crush-net

Raw TCP (and client-side TLS) sockets as [Crush](../../README.md) host
capabilities. Portable and std-only: a small polling reactor, no tokio, no
libp2p. Ported from exosphere's `exo-mesh` (see `src/lib.rs`).

Depends only on `crush-vm` (for `HostCap` / `HostCaps` / `Value`). `crush-lang-sdk`
depends on this crate behind its `tcp` feature, not the other way round.

## Capabilities

The caps are named `tcp.*` and deliberately **not** `net.*`: `net.*` is the
HTTP-client family that `crush-run --net` grants (and that a `net` family
declaration in `@capabilities` / `capsule.toml` covers). Raw sockets need their
own grant.

| capability | args | returns | grant (`crush-run`) | `register_*` |
|---|---|---|---|---|
| `tcp.connect` | `"tcp://IP:PORT"` | connection id (Int) | `--tcp` | `register_client` |
| `tcp.tls_wrap` | connection id, SNI host | TLS connection id (`tls` feature) | `--tcp` | `register_client` |
| `tcp.listen` | `"tcp://IP:PORT"` | listener id | `--tcp-listen` | `register_listen` |
| `tcp.accept` | listener id, opts? `{timeout_ms}` | connection id (blocks; past `timeout_ms` a catchable error) | `--tcp-listen` | `register_listen` |
| `tcp.send` | connection id, string or bytes | bytes written | `--tcp` or `--tcp-listen` | `register_io` |
| `tcp.recv` | connection id, opts? `{timeout_ms}` | string (blocks; UTF-8; `""` only at end of stream) | `--tcp` or `--tcp-listen` | `register_io` |
| `tcp.close` | connection id | - | `--tcp` or `--tcp-listen` | `register_io` |
| `tcp.ping` | anything | `"pong"` | `--tcp` or `--tcp-listen` | `register_io` |

`tcp.recv` returns whatever one read delivered (at most 16 KiB), minus an
incomplete UTF-8 character at the end, which is held back and returned at the
start of the next call (#507). A message can therefore arrive over several
calls: **loop until you have it** (a delimiter, a `Content-Length`, or `""`).
A timed-out `tcp.recv` loses nothing; the data is read by the next call.

Addresses are literal IPs (no DNS lookup). `--tcp-listen` is the stronger grant:
it makes the program reachable from other hosts, and it does **not** include
outbound `tcp.connect`. `register_io` carries no authority of its own (an id
only exists if a client or listen cap produced it), so hosts register it with
either of the other two.

Grants are all-or-nothing: there is no host:port scoping (a `--tcp` program can
dial any IP; a `--tcp-listen` program can bind any address, including
`0.0.0.0`). Scope them with the OS (network namespace, firewall) if you need to.

```crush
fn main() {
    let c = tcp.connect("tcp://127.0.0.1:7000")
    tcp.send(c, "hello")
    io.print(tcp.recv(c))
    tcp.close(c)
}
```

```text
$ crush-run run --tcp echo_client.crush
```

## Embedding

```rust
use crush_vm::HostCaps;
let mut caps = HostCaps::new();
let state = crush_net::new_state();            // one table shared by the groups
crush_net::register_io(&mut caps, state.clone());
crush_net::register_client(&mut caps, state);  // client only; no listen/accept
```

`crush_net::register(&mut caps)` registers every group with no gating.

## Features

- `tls` (default): `tcp.tls_wrap` over rustls (ring) with the webpki roots.
