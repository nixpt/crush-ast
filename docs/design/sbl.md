# SBL — the `system.*` functions written in Crush, and where they sit among the capability layers

**Status:** design note, 2026-10-07 (CRUSH-173). Ported in CRUSH-122 (see its `dejavue` decision);
code in `crates/crush-lang-sdk/src/sbl.rs` and `crates/crush-lang-sdk/sbl/sbl_core.crush`.

## What it is

The **System Bytecode Layer** is a small library of standard functions implemented *in Crush* rather
than in host Rust, exposed to programs as host capabilities named `system.<fn>`:

| Capability | Returns |
|---|---|
| `system.path_normalize(path)` | path with `.`/empty segments dropped and `..` applied; always absolute (`"a/b/../c"` → `"/a/c"`; `..` at the root is dropped) |
| `system.format_node_info(id, version)` | `"Node: <id> (v<version>)"` |
| `system.format_info(id, version)` | same as `format_node_info` (kept for the name exosphere's nanovm used) |

It replaces nanovm's `sbl_core.casm`, which was a stub (`path_normalize` returned its input) and was
never wired into crush-ast.

## How a call runs

1. `sbl_core.crush` is embedded in the binary (`include_str!`) and compiled **once**, lazily, on the
   first `system.*` call; the result (or the compile error) is cached in a `OnceLock`.
2. Each call clones the compiled program, sets its entry to the requested function, and runs it in a
   **fresh `PortableVm`** with `Quotas::default()` (1M steps, 4096 stack, 256 call depth, 30 s wall
   clock) and a `HostCaps` holding **only the pure stdlib** (`stdlib::register_pure`). The arguments
   are pushed as the entry function's parameters; the top of the result stack is the return value.
3. Arity is checked against the Crush signature before running; unknown names and wrong arity are
   errors (`system.<f>: …`), as is any VM error inside the call.

Consequences of that design, all intentional:

- **No authority.** An SBL function can call `str.*`, `math.*`, `path.*` … and nothing else — no
  `fs`, no `time.now`, no `system.*` (so no recursion into itself), no polyglot. A bug in the SBL
  cannot reach anything the pure stdlib cannot.
- **Its own budget.** The inner quotas are fixed, not inherited from the caller, and the inner VM's
  printed output is discarded.
- **Gated with the stdlib.** `system.*` is registered by `stdlib::register` and the module only
  compiles with the `stdlib` cargo feature; a registry built without `HostCapsBuilder::stdlib(true)`
  has no `system.*`. If the embedded source ever fails to compile, the SBL registers nothing and logs
  `SBL disabled` to stderr rather than failing the host.

## Why Crush and not Rust

The point of an SBL is that system logic is written once, in the language's own bytecode, and runs
under the same VM rules as user code. Rewriting `path_normalize` in Rust would have duplicated
`path.normalize` and removed the reason to have an SBL at all. It also exercises the toolchain: every
`system.*` call is a real compile + run of Crush code inside the host. The cost — a VM start per call —
is fine for functions of this size; anything hot belongs in the Rust stdlib.

## Capability layers

What a Crush program can call, from the inside out:

| Layer | Examples | Where | How it is enabled |
|---|---|---|---|
| **VM built-ins** | `io.print`, `io.read`, `str.len/concat/contains/split/join/replace`, `conv.chr/ord` | `crates/crush-vm/src/caps.rs` (registry), `scheduler.rs` `dispatch_cap` | always; `--cap` / manifest declaration only |
| **stdcaps** (pure) | `str.*`, `math.*` (incl. seeded RNG), `conv.*`, `json.*`, `regex.*`, `path.*`, `collections.*`, `bytes/buffer/binary`, `result.*`, `text.sort/uniq`, `time.format/parse`, `env.os/arch` | `crates/crush-lang-sdk/src/stdlib.rs`, `src/stdlib/` | `HostCapsBuilder::stdlib(true)` (feature `stdlib`) |
| **SBL** | `system.*` | `src/sbl.rs`, `sbl/sbl_core.crush` | with the stdlib |
| **corecaps** (I/O, granted) | `fs.*`, `text.head/tail/wc/cut/grep`, `time.now/sleep`, `env.get`, `net.*`, `process.*`, `crypto.*`, `polyglot.<lang>` | `src/host_caps.rs`, `src/text_tools.rs`, `src/net.rs`, crush-vm `host.rs` | a named grant each (`--fs`, `--time`, `--env`, `--net`, `--polyglot`, …) |

The rule (also MIGRATION-INVENTORY §2.1): *pure → stdcap, in-VM, no grant; touches the world → a
corecap behind a named grant; execution semantics → crush-vm.* exosphere's older plan stacked a Crush
"corecap" coreutils layer *on top of* the SBL; crush-ast doesn't — coreutils-style file operations are
grant-gated host caps (CRUSH-151), because a Crush-implemented `ls` would still need a host `fs` grant
underneath and adds nothing but a second implementation.

Dispatch order matters when names overlap: `dispatch_cap` checks that the name is declared and allowed
by the quotas, then **looks it up among the VM built-ins first and returns there** — a host
capability with a built-in's name is never reached. `str.split` and `str.join` exist in both the VM
and the stdlib, so programs (including the SBL) always get the VM's version. Treat built-in names as
reserved; a stdlib function that should behave differently needs a different name.

## WIT / component model (not implemented — note only)

An ancestor design floated describing capabilities as WebAssembly Component Model interfaces (WIT).
Nothing in crush-ast implements or depends on WIT today: no `.wit` files, no `wit-bindgen`, and the
capability metadata is too thin to generate one — `HostCapSpec` is `{ name, argc: Option<usize>,
returns: bool }`, with no argument or return types. On `main`, nothing exports it in machine-readable
form (`crush-run caps` prints a hand-written text list); CRUSH-155 (#104, in review) adds `effects`
metadata and `crush-run caps --json`. The prerequisites, if this is ever picked up:

1. typed argument/return signatures on `HostCapSpec`;
2. one machine-readable export of the registry that the WIT, `crush-run caps` and the docs are all
   generated from, so they cannot drift (CRUSH-155's `--json` is the natural starting point);
3. a consumer — a wasm host embedding crush-web, or Crush calling a component — to decide which
   direction the interface faces.

Until then, keep capability names stable (`family.verb`) and their argument conventions uniform; that
is what a WIT mapping would need from the current design.
