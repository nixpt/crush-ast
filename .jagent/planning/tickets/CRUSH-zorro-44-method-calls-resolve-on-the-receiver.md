# CRUSH-zorro-44 — COSM K1.5 — method calls resolve on the receiver

| Field | Value |
|-------|-------|
| **ID** | CRUSH-zorro-44 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | COSM K1 |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-zorro-43, CRUSH-zorro-41 |
| **Estimated effort** | M |
| **Filed by** | vega — 2026-10-15, COSM phase K1 (owner decisions recorded in the design doc and dejavue) |

## Status note (kai, 2026-10-20)

Part of this ticket lands, deliberately narrowed, in **PR #285** (CRUSH-277 S3/S5, open at the time of writing):
JS method calls lower to member calls only where the receiver is **provably an instance**: `this`, bindings bound with
`new C()`, and computed receivers (`new P(4).get2()`, CAST `Expression::MethodCall`), plus `super.m()`. Parameter and
capture receivers stay **loud** on purpose (its decision D4), so built-in methods on primitives never silently change.

What's left here is the **general** rule: a receiver whose Kind isn't known statically (a parameter, a capture, a
field holding a function), with `CALL_MEMBER` + a runtime fallback to the built-in path. That needs a **design pass
first** (the dispatch rule for Records vs primitives vs capability namespaces, written into
`docs/design/object-model.md`), not just an implementation. Keep it Backlog until that design note exists, and check
it against what #285 ships so the two don't disagree.

## Problem

Today `obj.m(x)` compiles to a call to the **global** host capability `m` with `obj` as the first argument
(`compiler.rs`'s dotted CapabilityCall path), so methods are global names and a function stored in a field can't
be called as `o.f(x)`. With COSM, a method call must resolve on the receiver.

## Success criteria

- [ ] `o.m(args)` where `o` is a Record (statically known Kind) compiles to a **direct call** of the resolved method (via `GET_SLOT`/a static CALL where possible), or to `CALL_MEMBER` when the Kind isn't known statically.
- [ ] Receivers that aren't Records keep today's behaviour: built-in methods on primitives (`xs.push`, `s.upper()`, the Python/JS walkers' method tables) and **capability namespaces** (`net.fetch`, `fs.cat`, `math.pow`) are unchanged. The dispatch rule is documented in the design doc and in compiler comments.
- [ ] `o.f(x)` where `f` is a **field holding a function value** (a Map or Record field) calls it (`GET_MEMBER` + `CALL_VALUE`).
- [ ] No silent change for existing programs: the whole `crush-lang-sdk` suite, the walker set and the parity harnesses pass unchanged, or with explicitly updated lists in the same PR.
- [ ] Tests: Crush `impl` methods (static and dynamic receivers); a field holding a lambda called as a method; `self` mutation visible through the caller (`reference`) vs not (`value(copy)`); a trait default method; a name clash between a Record method and a built-in method name (the Record's own wins).

## Technical approach

- Decide statically when the receiver's Kind is known (a let with a struct literal/constructor, a typed param), and emit `CALL_MEMBER` with a runtime fallback to the built-in path for non-Records otherwise.

## Files to modify

- `crates/crush-frontend/src/compiler.rs` (CapabilityCall / method-call lowering)
- VM dispatch for `CALL_MEMBER` on non-Records (fallback)

## Non-goals

- Walker changes (K3).
- Operator overloading (K2).

## Dispatch

**Read first:** `docs/design/object-model.md` (PR #206: **COSM — the Crush Object Semantic Model**), especially *COSM: objects are lowered, like code*, *The strategy catalogue*, *Worked
examples* and *Decisions*. If #206 isn't merged yet:
`git show origin/agent/vega/crush-kinds-design:docs/design/object-model.md`.

**The bar** (as for the parity work): the scheduler VM and the portable VM are exact. FastVM, JIT and AOT either
implement the feature or refuse **loudly at load/compile time**. Nothing is ever silently wrong, and anything
unsupported names what isn't supported.

**Rules:**
- Work in a worktree under `crush-ast/.jagent/worktrees/<you>` (a sibling of the `buckets` symlink).
- `CARGO_TARGET_DIR=/workspace/scratch/builds/<you>`, `CARGO_BUILD_JOBS=6` (14 GB RAM shared).
- Commit trailer `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- A ready-for-review PR; never force-push or merge.
- If GitHub shows a conflict only in `.dejavue/*` (merge=union), merge `origin/main` locally.
- **Update the parity harness lists in the same PR** if your change flips any corpus entry
  (`crates/crush-lang-js/tests/node_corpus_test.rs`, `crates/crush-aot/tests/js_parity.rs`, `python_parity.rs`).
- Gates: the crates you touch, plus `-p crush-vm -p crush-lang-sdk -p crush-frontend -p crush-aot`, the walker set
  (the `Test (walkers)` CI job's package list), clippy on touched crates, and `git diff --check`.
