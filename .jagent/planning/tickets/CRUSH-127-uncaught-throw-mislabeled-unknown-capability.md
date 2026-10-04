# CRUSH-127 — Uncaught `throw` is reported as `unknown capability`; compile errors are labelled `[runtime]`

| Field | Value |
|-------|-------|
| **ID** | CRUSH-127 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | M1 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | XS |
| **GitHub** | [#67](https://github.com/nixpt/crush-ast/issues/67) (filed 2026-10-04 by pranix) |

## Problem

Both VMs return `VmError::UnknownCap("uncaught error: …")` for an uncaught throw (`portable_vm.rs` ~l.961, `scheduler.rs` ~l.1180), so the message reads `[runtime] unknown capability: uncaught error: …`. Separately (found in triage), `crush-run run x.crush` prints parse/type errors as `[runtime] Parse errors: …`.

## Reproduction

From [#67](https://github.com/nixpt/crush-ast/issues/67) (verbatim):

#### Summary

A plain uncaught `throw "msg"` prints `[runtime] unknown capability: uncaught error: msg`. The "unknown capability" prefix is wrong and misleading — no capability is involved.

#### Repro

```crush
fn main() { throw "line 5, col 5: expected a value" }
```

Output: `[runtime] unknown capability: uncaught error: line 5, col 5: expected a value`.

#### Expected

Something like `[runtime] uncaught error: line 5, col 5: expected a value` — the message preserved, without the bogus capability label. (Also note: `fs.read` failures surface as *uncatchable* `unknown capability` errors, so try/catch cannot be used for file-exists fallback.)

## Success criteria

- [ ] uncaught throw prints `[runtime] uncaught error: <msg>`
- [ ] compile-time errors are not labelled `[runtime]`

## Technical approach

- Add `VmError::Uncaught(String)`; use it in both VMs.
- In `crush-run`, label frontend failures as compile errors.

## Files to modify

- `crates/crush-vm/src/vm.rs`
- `crates/crush-vm/src/portable_vm.rs`
- `crates/crush-vm/src/scheduler.rs`
- `crates/crush-lang-sdk/src/bin/crush-run.rs`
