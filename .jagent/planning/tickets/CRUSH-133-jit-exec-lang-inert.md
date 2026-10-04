# CRUSH-133 — JIT `ExecLang` yields a bare tag that nothing services

| Field | Value |
|-------|-------|
| **ID** | CRUSH-133 |
| **Priority** | P3 |
| **Status** | Backlog |
| **Phase** | M2 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | S |
| **GitHub** | [#73](https://github.com/nixpt/crush-ast/issues/73) (filed 2026-10-04 by pranix) |

## Problem

`crates/crush-jit/src/compiler.rs` (~l.1178) lowers `ExecLang` to `emit_host_yield(…, HOST_REQ_EXEC_LANG)` with no lang/code/vars; nothing services the request and `String` results resume as null. Confirmed by reading; not run here.

## Reproduction

From [#73](https://github.com/nixpt/crush-ast/issues/73) (verbatim):

#### Summary

In the JIT pipeline, `ExecLang` lowers to a host-request yield that carries only a tag — no language, no code, no variables — and nothing services the request. On resume, `String` results are mapped to null. Polyglot via JIT is effectively dead code.

#### Expected

Either wire the JIT `ExecLang` path through to the polyglot executor (like the interpreter does), or remove/gate it so users get a clear "not supported" instead of silent nulls. (Note: rebuilding the JIT stack locally is currently blocked by the ort prebuilt-download 403, so this is filed for tracking.)

## Success criteria

- [ ] JIT either executes `@lang` blocks like the interpreter, or refuses them with a clear "not supported" error

## Technical approach

- Short term: reject at JIT compile time. Long term: carry the payload and service it via the shared EXEC_LANG path.

## Files to modify

- `crates/crush-jit/src/compiler.rs`
- `crates/crush-jit/src/runtime.rs`
