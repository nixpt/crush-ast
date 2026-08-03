# CRUSHAST-BUCKETSPIKE-1 — results

Throwaway spike, not production work. Branch `agent/cece/CRUSHAST-BUCKETSPIKE-1`,
off `agent/cece/CRUSHAST-POLYGLOT-1`. Empirical validation only, for a design
decision Kai (foreman) is making elsewhere on this project.

## Question

Can a `@python { ... }` polyglot block run inside a `buckets`-provisioned,
bwrap-sandboxed `python3` instead of the host's `python3`, with the existing
sentinel-line JSON marshaling protocol (`crush_vm::scheduler`'s
`CRUSH_RESULT_SENTINEL` + `crush-lang-sdk`'s `rewrite_python_marshaling`)
surviving unchanged?

## Answer: yes

- `bwrap` **was** actually exercised, not the unsandboxed fallback — verified
  both by an explicit `which bwrap`-equivalent check in the spike binary
  (`bwrap on PATH: true`) and by the *absence* of `buckets::sandbox`'s stderr
  fallback warning ("bwrap not found on PATH — running WITHOUT sandbox
  isolation"), which would have printed if it had fallen through.
- `bwrap` is present on this box at `/usr/bin/bwrap`.
- The sentinel-line + ordinary-output split parsed identically to the
  unsandboxed case: the block's own `print(...)` came through as ordinary
  stdout, and the sentinel-prefixed line decoded to the correct
  `crush_vm::vm::Value` via the real `Deserialize` impl. Two independent runs,
  both `PASS`.
- Real cold vs warm provisioning latency numbers below.

## What the spike does NOT prove

- Not wired into the real `EXEC_LANG` opcode handler (explicitly out of
  scope) — this is a standalone binary that hand-replicates both the Python
  source shape `rewrite_python_marshaling` emits and the stdout-scanning
  logic the real opcode handler uses, then runs it via
  `buckets::sandbox::sandboxed_command` instead of a bare
  `std::process::Command::new("python3")`.
- Bare `python@3.11`, stdlib only — no PyPI/numpy (confirmed out of scope;
  buckets has no PyPI-level provisioning).
- `python@3.11` resolved to buckets' newest matching version, `v3.14.6` (caret
  constraint `^3.11` = `>=3.11.0, <4.0.0`, and pkgx's dist server's newest
  3.x happens to be 3.14.6) — not literally CPython 3.11.x. Not a bug, just
  worth flagging: if the real integration needs a specific minor pinned, the
  spec string needs `python@=3.11.x` or similar, not bare `python@3.11`.

## Setup

- Repo: `/home/nixp/WORKSPACE/projects/crush-ast` (this worktree)
- `buckets` dep: `buckets = { path = "/home/nixp/WORKSPACE/projects/buckets" }`
  — **absolute path, spike-only**. A real integration needs a relative
  sibling-project path (this workspace's usual peer-project convention) or a
  registry dependency, not an absolute path.
- New crate: `crates/crush-bucketspike` (binary `bucketspike`), **deliberately
  NOT added to crush-ast's root workspace `members` list**. Its own
  `Cargo.toml` declares `[workspace]` (empty) to make it its own workspace
  root. Reason: `crates/crush-pkg` has its own pre-existing, already-tracked
  broken `buckets` path dependency in this nested-worktree layout
  (`../../../buckets` resolves outside the worktree) — `cargo build` on the
  full crush-ast workspace fails trying to load `crush-pkg`'s manifest before
  it ever reaches this crate. This spike does not touch `crush-pkg` at all;
  making `crush-bucketspike` its own workspace root was the way to build and
  run it without tripping over that unrelated, already-known issue.
- Depends on the real `crush-vm` crate (path dep to `../crush-vm`) for
  `crush_vm::scheduler::CRUSH_RESULT_SENTINEL` and `crush_vm::vm::Value`, so
  the spike's scanning/decoding logic can't silently drift from the real
  `EXEC_LANG` opcode handler.

### Build gotcha found along the way (spike-local, not fixed upstream)

`crush-ast`'s own `.cargo/config.toml` sets `CFLAGS`/`LDFLAGS` to `-flto`
("Layer 2" of a 3-layer LTO strategy for AOT-generated C, per its own
comments). Cargo's `.cargo/config.toml` discovery walks up the directory
tree from CWD regardless of which Cargo workspace you're actually building
(declaring `[workspace]` in `crush-bucketspike`'s own manifest does not stop
that walk), so it's found and applied to `crush-bucketspike`'s build too —
and it broke `buckets`' `ureq`/`ring` TLS dependency's linking, producing
pages of `undefined symbol: ring_core_0_17_14__...` from `rust-lld` (the C
side gets compiled `-flto`, the objects that link against it don't match).
Building with `CFLAGS= LDFLAGS=` (cleared) and `RUSTC_WRAPPER=` (bypassing
the global `sccache` wrapper, unrelated but also picked up from
`~/.cargo/config.toml`) fixed it. Not investigated further or fixed
upstream — out of scope for this spike, and this crate isn't part of the
real workspace's LTO story anyway (it's not shipping).

## Real run output (two independent invocations)

Command each time:
```
cd crates/crush-bucketspike
CARGO_TARGET_DIR=<scratch> RUSTC_WRAPPER= CFLAGS= LDFLAGS= cargo build
<scratch>/debug/bucketspike
```

### Run 1

```
=== CRUSHAST-BUCKETSPIKE-1 ===
bwrap on PATH: true
fresh BUCKETS_CACHE_DIR: /tmp/bucketspike-cache-578290

--- COLD run (fresh cache, first resolve+install+sandbox+exec) ---
✓ resolved python.org@^3.11 → v3.14.6
↓ installing 1 package(s)...
↓ fetching https://dist.pkgx.dev/python.org/linux/x86-64/v3.14.6.tar.xz
✓ cached python.org v3.14.6
cold: success=true resolve_duration=4.439881308s total_wall=4.482832764s
cold stdout (raw, with embedded NUL sentinel bytes shown as \0):
"hello from sandboxed python\n\0CRUSH_RESULT\06\n"
cold visible output: "hello from sandboxed python"
cold decoded sentinel Value: Int(6)

--- WARM run (same cache dir, second resolve = cache hit) ---
✓ resolved python.org@^3.11 → v3.14.6
warm: success=true resolve_duration=6.15461ms total_wall=50.558982ms
warm stdout (raw):
"hello from sandboxed python\n\0CRUSH_RESULT\06\n"
warm visible output: "hello from sandboxed python"
warm decoded sentinel Value: Int(6)

=== SUMMARY ===
bwrap exercised: true
cold resolve_duration: 4.439881308s  (total incl. sandbox exec: 4.482832764s)
warm resolve_duration: 6.15461ms  (total incl. sandbox exec: 50.558982ms)
marshaling proof: PASS
```

### Run 2 (fresh cache dir again, independent process)

```
=== CRUSHAST-BUCKETSPIKE-1 ===
bwrap on PATH: true
fresh BUCKETS_CACHE_DIR: /tmp/bucketspike-cache-578325

--- COLD run (fresh cache, first resolve+install+sandbox+exec) ---
✓ resolved python.org@^3.11 → v3.14.6
↓ installing 1 package(s)...
↓ fetching https://dist.pkgx.dev/python.org/linux/x86-64/v3.14.6.tar.xz
✓ cached python.org v3.14.6
cold: success=true resolve_duration=6.078227613s total_wall=6.132565843s
cold stdout (raw, with embedded NUL sentinel bytes shown as \0):
"hello from sandboxed python\n\0CRUSH_RESULT\06\n"
cold visible output: "hello from sandboxed python"
cold decoded sentinel Value: Int(6)

--- WARM run (same cache dir, second resolve = cache hit) ---
✓ resolved python.org@^3.11 → v3.14.6
warm: success=true resolve_duration=7.383175ms total_wall=43.468928ms
warm stdout (raw):
"hello from sandboxed python\n\0CRUSH_RESULT\06\n"
warm visible output: "hello from sandboxed python"
warm decoded sentinel Value: Int(6)

=== SUMMARY ===
bwrap exercised: true
cold resolve_duration: 6.078227613s  (total incl. sandbox exec: 6.132565843s)
warm resolve_duration: 7.383175ms  (total incl. sandbox exec: 43.468928ms)
marshaling proof: PASS
```

(Note: the `✓`/`↓` glyphs above are `buckets`' own `eprintln!` output —
its source uses UTF-8 arrows/checkmarks that rendered as replacement bytes
in the raw terminal capture; reproduced here as the intended glyphs for
readability. The `resolve_duration`/`total_wall`/stdout/decoded-value lines
are copied verbatim from the actual terminal output.)

## Latency summary

| | cold (fresh cache) | warm (cache hit) |
|---|---|---|
| Run 1 `resolve_duration` | 4.44 s | 6.2 ms |
| Run 1 total wall (incl. bwrap spawn + python exec) | 4.48 s | 50.6 ms |
| Run 2 `resolve_duration` | 6.08 s | 7.4 ms |
| Run 2 total wall | 6.13 s | 43.5 ms |

Cold latency is almost entirely network fetch + bottle extraction (a few
seconds, dominated by downloading `python.org`'s ~tens-of-MB bottle from
`dist.pkgx.dev`) — the `resolve_duration` and `total_wall` are nearly
identical on cold runs because sandbox spawn + python exec is negligible
next to the download. Warm latency (cache hit, no network) is dominated by
bwrap spawn + python startup: single-digit ms for `resolve_multi` itself,
tens of ms once the actual sandboxed `python3 -c` process is spawned, run,
and reaped.

## Marshaling proof (both runs)

Raw captured stdout from the sandboxed subprocess, byte for byte:
```
hello from sandboxed python\n\0CRUSH_RESULT\06\n
```

Split via the exact same logic as `crush_vm::scheduler`'s `EXEC_LANG`
opcode handler (line-split, pull the `CRUSH_RESULT_SENTINEL`-prefixed line
out, everything else is "visible" output):

- visible output: `"hello from sandboxed python"` — the block's own
  `print(...)`, correctly separated from the marshaled result.
- sentinel payload: `"6"` — JSON-decoded via `crush_vm::vm::Value`'s real
  `Deserialize` impl into `Value::Int(6)`, matching `base=5` (passed in as a
  JSON-encoded env var, exactly as the real `EXEC_LANG` handler does via
  `cmd.env(name, val.as_text())`) plus the block's own `result = base + 1`.

Both come through correctly, sandboxed, identically to how the unsandboxed
case is documented to behave in
`crates/crush-lang-sdk/src/compile.rs::test_python_polyglot_block_own_prints_still_visible`.

## Files

- `crates/crush-bucketspike/Cargo.toml`, `crates/crush-bucketspike/src/main.rs`
  — the spike binary.
- This file.

## Explicitly not done (per task scope)

- `crates/crush-vm/src/scheduler.rs`'s real `EXEC_LANG` handler: untouched.
- `crates/crush-pkg`: untouched, its pre-existing broken `buckets` dep
  unaffected either way.
- No PyPI/numpy provisioning attempted.
- `agent/kai/CRUSHAST-RELEASE-1` and any `Cargo.toml` `[package]` version
  lines: untouched.
