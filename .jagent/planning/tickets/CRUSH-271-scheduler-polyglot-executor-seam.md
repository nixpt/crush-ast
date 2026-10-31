# CRUSH-271 — The scheduler VM takes an embedder-supplied polyglot executor

| Field | Value |
|-------|-------|
| **ID** | CRUSH-271 |
| **Priority** | P1 |
| **Status** | In review |
| **Phase** | M6 |
| **Assignee** | claude |
| **Dependencies** | #91 (PortableVm `LangExecutor`), CRUSH-20, CRUSH-226, CRUSH-270 |
| **Estimated effort** | M |
| **Filed by** | claude — 2026-10-16, for an embedder (a browser) that runs polyglot blocks in its own capsule host |

## Problem

Polyglot *runners* belong to the embedder (a CLI, a browser, an OS), not to the language.
Crush should keep the polyglot syntax, the variable marshaling and the `polyglot.<lang>`
grant check, and let the embedder decide how a granted block runs.

`PortableVm` already has that seam (`LangExecutor`, `PortableVm::set_lang_executor`, #91,
used by crush-web). The scheduler VM — the native default behind `run_with_caps`,
`run_scheduled*` and `crush_lang_sdk::Runtime` — has none: `EXEC_LANG` always spawns the
interpreter itself (`Command::new(binary)`, or the buckets path with `sandboxed-polyglot`).
An embedder running Crush through crush-vm or crush-lang-sdk cannot route blocks anywhere else.

## Decision

- One executor type for both VMs: `crush_vm::PolyglotExecutor` (`Send + Sync`, `&self`),
  carried on `HostCaps` (`set_polyglot_executor` / `polyglot_executor` /
  `take_polyglot_executor`), so every entry point that takes host caps carries it.
- It receives a `PolyglotRequest`: lang, code, encoded input vars, declared outputs,
  pinned deps, the gate name, the wall-clock deadline and the `.crush` line; it returns
  the block's stdout. Errors are a `PolyglotError` (`Failed` / `Guest` / `Timeout` / `Vm`)
  that maps onto the same VM errors the subprocess path produces.
- The built-in subprocess/buckets runner becomes `SubprocessExecutor`, the default when
  none is set — so a later change can move it out of crush-vm.
- The grant check stays in the VM and runs first, on both engines.
- `parallel_map` items run with no host caps (no grants), so their blocks are refused
  before any executor; spawned green tasks share the caller's host caps and so its executor.
- `PortableVm::set_lang_executor` / `LangExecutor` stay as they are (crush-web's API); when
  set, it takes precedence over the host-caps executor.

## Success criteria

- [x] With an executor set, the scheduler VM runs `EXEC_LANG` through it instead of the
      subprocess path; its stdout becomes the block's output and declared outputs.
- [x] Same executor, same result on `PortableVm`, and the same as a legacy `LangExecutor`.
- [x] An ungranted, undeclared, `allowed_caps`-denied or unknown-language block never
      reaches the executor.
- [x] An executor error surfaces as a VM error with the executor's message.
- [x] Spawned tasks use the executor; `parallel_map` items are refused, as before.
- [x] `crush_lang_sdk::Runtime::with_polyglot_executor` and
      `HostCapsBuilder::polyglot_executor`.
- [x] `crush-run --polyglot` unchanged; existing polyglot and parity suites pass.

## Not in scope / follow-ups

- CRUSH-236 (sandbox by default) — unchanged; `SubprocessExecutor` is where it lands.
- Moving `SubprocessExecutor` out of crush-vm (into crush-lang-sdk or a runner crate).
- FastVM/JIT `HostRequest::ExecLang` already yields to the host; not touched.
- Embedder-defined languages beyond the built-in allowlist (python/javascript/bash): the
  allowlist check still runs before the executor.
