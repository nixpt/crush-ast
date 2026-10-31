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

**Design decided (2026-10-21):** [`docs/design/object-model.md` §9 "Method dispatch: how `o.m(args)` resolves"](../../../docs/design/object-model.md)
is the rule this ticket implements. Summary: a dotted call is classified once, at lowering, by form and receiver
provenance — (1) a capability namespace whose prefix is not a value binding stays a full-named `cap_call`; (2) a
provably exact Kind may lower to a direct `call` (an optimisation, never a different semantics); (3) a known but
inexact Kind and (4) a Kind known only at run time lower to `CALL_MEMBER`; (5) a provably primitive receiver keeps
the walkers' existing built-in lowering. At run time the one shared `cosm_rt::call_member` dispatches in the order:
instance field holding a function value → the receiver Kind's members (a Record's own method always beats a
same-named built-in) → a closed, per-type built-in table for non-Record primitives → a loud error. A Map has only
the field-function case. `CALL_MEMBER` on FastVM/JIT/AOT is refused at load/compile time, never partially
implemented. The CRUSH-280 rule stays: dynamic calls are passed raw and are refused at lowering if any same-named
member in the program has a default/rest (non-plain) signature, because the hidden `__has_*` ABI is positional.
No path from a member call to a host capability may remain: the frontend's unmarked `cap_call "<method>"`
fall-through for a declared-variable receiver becomes a member call, so a capability name can only originate from
a namespace/bare form naming it in full (§9.6 of the design).

Implementation plan (small slices, in order; each lands with its test and the gates in Dispatch below):

1. **Runtime fallback, Map + non-callable tiers (VM crate only).** Extend the shared `cosm_rt::call_member`
   (`crates/crush-vm/src/cosm_rt.rs`) with the `Value::Map` field-function branch (§9.3) and keep the non-Record
   error naming method and receiver type. Test: `crates/crush-vm/src/tests/records.rs`-style CVM1 cases run on
   both VMs — Map field lambda called as `o.f(x)` returns its result; absent member and non-function field each
   error with the §9.4 messages; a Record field function still shadows a same-named Kind method.
2. **Runtime fallback, built-in tier (VM crate only).** Add the one shared per-type built-in table beside
   `cosm_rt` (initial rows only where already exact on both VMs, per §9.7 Open (owner) 1) and call it from
   `cosm_rt::call_member` for non-Record/non-Map/non-Kind receivers; no capability lookup, no permission.
   Test: the §9.4 `bump(v)` example parameterised over an array, a Record with a `push` method, an int, and a
   Map with a non-function `push` field, on both VMs; a `"hi".push(x)` type/name mismatch errors, and
   `capabilities_used` on the program reports no capability named after the method.
3. **Frontend: close the capability fall-through.** In `crates/crush-frontend/src/compiler.rs`'s dotted
   `CapabilityCall` path, a declared-variable receiver whose method is not a receiver method lowers to
   `call_member` instead of `cap_call "<method>"` + a recorded permission (§9.6); namespace forms (prefix not a
   declared variable) are untouched. Test: frontend tests asserting `p.frobnicate()` on a struct compiles to
   `call_member` with an empty added-permission set, `net.fetch(…)` still compiles to `cap_call "net.fetch"`,
   and a local value shadowing a namespace name calls its field function.
4. **JS walker: unknown-receiver member calls.** In `crates/crush-lang-js`, an identifier/computed receiver the
   walker cannot prove is an instance and cannot claim with a built-in row (conservatively, per §9.1 rule 4)
   lowers to CAST `MethodCall` (marked for the §9.5 scan) instead of the walk-time refusal, and built-in rows lose
   first claim on receivers proven to be instances (§9.2). Test: boa e2e — a parameter/capture/call-result
   receiver holding a class instance calls its method, the same receiver holding an array uses the built-in,
   a Record method named `add`/`push` beats the built-in row, and parity lists change only where the PR says so.
5. **Argument-shape scan as a shared rule (§9.5).** Generalise the JS walker's end-of-module checks
   (`check_member_calls`) so every member call emitted under slices 3–4 is either adapted for an exact target,
   adapted with all reachable overrides proved shape-identical, or raw with no non-plain same-named member in
   the program; Crush's `receiver_methods` path gets the equivalent scan over its COSM module. Test: the §9.5
   worked example — `new A().m(1)` adapts and runs, `f(o) { o.m(1) }` over a defaulted `m` refuses at lowering,
   a differently-shaped override refuses, and a field-function call with the wrong arity errors at run time.
6. **Backend refusal sweep.** Pin, per backend, that a program containing `call_member` is refused at
   load/compile time naming the op (FastVM/JIT lowering, `crush-aot` Rust and C, `crush-aotc`, PTX), while a
   rule-2/rule-5 program with no `call_member` still compiles there. Test: one fixture program per tier in each
   backend's existing unsupported-ops/refusal test file; no parity list entry silently flips.
7. **End-to-end + docs close-out.** A `crush-lang-sdk` suite covering the ticket's Success criteria (Crush `impl`
   methods on static and dynamic receivers, a lambda in a field called as a method, `self` mutation visible
   through a `reference` receiver and not through `value(copy)`, a trait default method, the §9.2 name clash),
   compiler comments naming §9 at the lowered sites, and this ticket's Status updated. Test: the suite itself on
   both exact VMs, plus the walker and parity harnesses unchanged or with explicitly updated lists.

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
