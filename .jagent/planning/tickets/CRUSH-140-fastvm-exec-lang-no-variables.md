# CRUSH-140 — FastVM's `ExecLang` request carries no variables

| Field | Value |
|-------|-------|
| **ID** | CRUSH-140 |
| **Priority** | P3 |
| **Status** | Backlog |
| **Phase** | M2 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | S |

## Problem

For `let base = 5; @python { result = base * 2 }`, FastVM yields `HostRequest::ExecLang { lang: "python", code, variables: {} }` — the block's input (`base`) isn't in `variables`, so a host servicing the request can't run the block correctly. The CVM1 path marshals it (`r=10`). Found while testing CRUSH-133 (the JIT now falls back to FastVM for `ExecLang`, so it inherits this).

## Reproduction

`crates/crush-aot/tests/jit_exec_lang.rs` — `python_request(fastvm.run(..))` returns an empty variable list.

## Success criteria

- [ ] FastVM's `ExecLang` request includes every input variable the polyglot pass recorded on the block (`LangBlock.variables`), with its current value
- [ ] `jit_exec_lang.rs` asserts `base=Int(5)` instead of just JIT/FastVM agreement

## Technical approach

- Check `ExecLangSite` lowering (`fastvm/instructions.rs` ~l.998) and the yield in `fastvm/execution.rs` ~l.1111: whether the variable names reach the site and their values are read from the frame at yield time.
- `crates/crush-vm/src/fastvm/` is lane-guarded — coordinate before landing.

## Files to modify

- `crates/crush-vm/src/fastvm/instructions.rs`, `execution.rs`
