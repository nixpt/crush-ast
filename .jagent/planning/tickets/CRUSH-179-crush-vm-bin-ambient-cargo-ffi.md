# CRUSH-179 — `crush-vm run` grants `os.cargo` and `__crush_ffi__` ambiently (process exec + dlopen)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-179 |
| **Priority** | P0 |
| **Status** | Backlog |
| **Phase** | M7 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | S |
| **Filed by** | claude — 2026-10-09 test-drive sweep, reproduced on `main` `554f077` (debug build) |

## Problem

The `crush-vm` binary registers two powerful host caps for every program, with
no host-side flag: `CargoCap` (runs `cargo` with program-chosen args and cwd)
and `FfiGatewayCap` (`dlopen`s any path the program names). Both sit behind the
`native-plugins` feature, which is on by default. The only gate is the
program's own manifest — which the program author writes. This contradicts the
"every VM-external op is an explicit, named, opt-in capability — never ambient"
rule. `crush-run` correctly refuses both (`unknown capability: os.cargo`).

`os.cargo` is arbitrary code execution: cargo's `--config build.rustc-wrapper=...`
runs any script.

## Reproduction

```crush
let r = os.cargo(["--version"], "/tmp")
io.print(r)
```

```
crushc r.crush -o r.cvm1
crush-vm run r.cvm1
# {stderr: , stdout: cargo 1.97.0 ..., success: true}
```

Escalation verified: `os.cargo(["--config","build.rustc-wrapper='<script>'","check","--offline"], dir)`
executed `<script>`.

`__crush_ffi__`: `crushc` rejects the name at type-check, but hand-written CASM or a
patched `.cvm1` manifest listing `["__crush_ffi__","io.print"]` loads
`libcrush_plugin_example.so` (and `libc.so.6` — library constructors run before
the missing `crush_plugin_init` symbol is reported).

## Where

- `crates/crush-vm/src/main.rs:88-93` — unconditional registration.
- `crates/crush-vm/src/cargo_cap.rs`, `crates/crush-vm/src/plugin.rs` (`FfiGatewayCap`;
  `load_plugin()` at ~`plugin.rs:204` has no callers).

## Success criteria

- [ ] `crush-vm run` registers neither cap unless the host passes an explicit flag
      (e.g. `--cap os.cargo`, `--plugin <path>` with an allowlist of plugin paths).
- [ ] FFI loading is restricted to host-named libraries; a program cannot pick the path.
- [ ] Regression test: the repro above fails with a capability error without the flag.
- [ ] Readiness matrix entry for FFI updated to reflect the gate.

## Related

- `crates/crush-plugin-example/src/lib.rs` leaks a `CString` per call (`mem::forget`,
  never freed by the VM) — fix alongside.
