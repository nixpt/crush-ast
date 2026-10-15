# CRUSH-zorro-26 — AOT Rust/C read missing values as `null` where the VM errors (silently wrong)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-zorro-26 |
| **Priority** | P0 |
| **Status** | Backlog |
| **Phase** | JS/TS parity spike |
| **Assignee** | unassigned |
| **Dependencies** | none (found by the CRUSH-zorro-4 harness, #198) |
| **Estimated effort** | M |
| **Filed by** | vega — 2026-10-15, from the JS/TS parity harness's AOT baseline |

## Problem

The parity harness (CRUSH-zorro-4, #198) found programs that the **VM rejects loudly** but that **AOT Rust and
AOT C run silently wrong**. From the outputs, the AOT runtimes read an undefined name, an uninitialised slot,
an out-of-range index or a missing field as `null` (and leave rest/spread unexpanded) where the VM raises an error:

| program | node | VM | AOT Rust + C |
|---|---|---|---|
| `console.log(NaN === NaN)` | `false` | loud ("uninitialised slot") | `true` |
| `let u; console.log(u, [1][5], ({}).x)` | `undefined undefined undefined` | loud | `null null null` |
| `console.log('a', 1, true, null, undefined)` | `a 1 true null undefined` | loud | `a 1 true null null` |
| `const o={a:{b:5}}; console.log(o?.a?.b, o?.x?.y)` | `5 undefined` | loud | `5 null` |
| `function s(...xs){return xs.length} console.log(s(1,2,3))` | `3` | loud | `0` |
| `const a=[1,2]; const b=[...a,3]; console.log(b.length, b[2])` | `3 3` | loud | `2 null` |

Also, **the two AOT backends disagree with each other**:
`console.log('1' == 1, 0 == '', 2 != '2')` gives node `true true false`, VM and AOT Rust `false false true`,
and AOT C `false false false`.

This breaks the exact-or-loud bar at the **backend** level, for every frontend, not only JS. Any language whose
lowering reaches these paths can get a silently wrong answer from AOT where the VM would have refused.

## Success criteria

- [ ] For each case above, AOT Rust and AOT C either match the VM's behaviour (the same error, as a non-zero
      exit with a message) or refuse at compile time. Never a silent `null`/`0`.
- [ ] Root-cause each class in the codegen (load of an undefined local/slot, index out of range, missing map
      field, rest/spread, NaN equality) and fix it at the runtime-helper level, so every frontend benefits.
- [ ] AOT Rust and AOT C agree with each other on every comparison operator. Add a cross-backend
      differential test over a small operator matrix (int/float/str/bool/null, `== != < <= > >=`).
- [ ] The harness's AOT `KNOWN_SILENT` entries for these programs move to match or loud.
- [ ] Check the Python parity corpus (`python_parity.rs`) and the other walker corpora for the same class.

## Notes

- Inferred from outputs, not yet traced in the codegen (per the harness author).
- Related: `enter_try` is unsupported on AOT, so try/catch/finally is loud there. That's fine under the
  bar, but worth a ticket of its own if AOT should support exceptions.
- Owner: whoever owns `crates/crush-aot/src/{codegen,codegen_c}.rs` runtime helpers. Not JS-walker work.

## Dispatch

Work in a worktree under `crush-ast/.jagent/worktrees/<you>` (a sibling of the `buckets` symlink). The
harness is `crates/crush-aot/tests/js_parity.rs` (after #198 lands); reproduce each row there and with a
minimal Crush program, since the bug is backend-level. Gates: `cargo test -p crush-aot`,
`CRUSH_GC_STRESS=1 cargo test -p crush-aot`, `python_parity`, clippy. Commit trailer
`Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Ready-for-review PR; don't merge.
