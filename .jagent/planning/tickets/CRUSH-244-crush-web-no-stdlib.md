# CRUSH-244 — `crush-web` registers no capabilities: `math.*`, `str.*`, `system.*`, `sys.*` missing in the browser

| Field | Value |
|-------|-------|
| **ID** | CRUSH-244 |
| **Priority** | P2 |
| **Status** | Done |
| **Phase** | M8 |
| **Assignee** | claude |
| **Dependencies** | none |
| **Estimated effort** | S |
| **Filed by** | claude — 2026-10-09 CASM↔WASM test drive |

## Problem

`crush-run` registers the pure standard library (62 capabilities: `math.*`, `str.*`,
`system.*`, …), `sys.args`/`sys.exit` and `caison.parse` with no grant flag. `crush-web`
ran every program with no host capabilities at all (`crush_vm::run`, a bare `PortableVm`),
and depended on `crush-lang-sdk` with `default-features = false`, which also drops the
`stdlib` feature. So programs that run natively with no grants failed in the browser:

- `examples/crush/math_test.crush`: `runtime error: unknown capability: math.sqrt`
- `examples/crush/test_sbl.crush`: `runtime error: unknown capability: system.path_normalize`

Corpus check (all 43 `examples/crush` programs, wasm build in Node vs native `crush-run`,
stdin closed, no grants): 22 identical; these 2 were the only browser-only failures that
weren't missing a grant.

## Success criteria

- [x] Every `crush-web` entry point registers the same capabilities as `crush-run` with no
      grant flags (`browser_caps`), and nothing that reaches outside the VM.
- [x] `sys.args` answers a new `args` option; `sys.exit` reports `exit_code` and keeps
      output printed before it.
- [x] `math_test` and `test_sbl` match native output in the wasm build.
- [x] The standard library can be left out of a build (`stdlib` feature, default on).

## Resolution

`browser_caps()` builds `HostCapsBuilder::new().stdlib(true).args(..)`, used by `execute`/
`run_blob` (via `run_with_caps_streaming`, so output before `sys.exit` survives) and by
`execute_with`/`Session` (`PortableVm::set_host_caps`). `VmError::Exit` is a finished run
with `exit_code`, not an error. Tests: `crates/crush-web/tests/caps.rs`. Live: the wasm
corpus run now matches native on 24 programs; the 7 remaining browser-only failures all need
grants the browser has no source for (CRUSH-245). Cost: ~0.5 → ~0.95 MB gzipped (`regex`),
hence the feature.
