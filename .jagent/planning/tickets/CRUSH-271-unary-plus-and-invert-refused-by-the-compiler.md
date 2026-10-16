# CRUSH-271 — Python/JS walker unary `+` and `~` are refused by the compiler (`Unsupported op`)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-271 |
| **Priority** | P3 |
| **Status** | Backlog |
| **Phase** | M6 |
| **Assignee** | unassigned |
| **Dependencies** | none (the JS half shares a decision with CRUSH-zorro-12/13) |
| **Estimated effort** | S |
| **Filed by** | buffy — 2026-10-16, found while measuring PR #263's walker table |

## Problem

The compiler's `Expression::UnaryOp` arm handles exactly three operators and bails on
everything else:

```rust
// crates/crush-frontend/src/compiler.rs:1713-1725
"-" => OpCode::Neg,
"not" | "!" => { ... }
_ => bail!("Unsupported op: {}", operator),      // :1721
```

Both walkers that support these operators emit the **CAST node**, not the intrinsic:

| Walker | site | emits |
|---|---|---|
| Python | `crush-lang-python/src/lower_expr.rs:120` (`UAdd → "+"`), `:122` (`Invert → "~"`) | `UnaryOp{"+"}`, `UnaryOp{"~"}` |
| JS (swc) | `crush-lang-js/src/lower_swc.rs:1249` (`Plus → "+"`), `:1251` (`Tilde → "~"`) | same |
| JS (boa) | `crush-lang-js/src/lower/expr.rs:52`, `:54` | same |
| C | `crush-lang-c/src/lib.rs:758-760` | **intrinsics** `__crush_pos__` / `__crush_bit_not__` |

So the *same* operation has two representations, and only one of them works. The intrinsic route is
fully wired: `__crush_pos__` is identity (`compiler.rs:2024-2027`), `__crush_bit_not__` emits
`bit_not` (`:2016-2019`), both are in the shared registry (`walker_intrinsics.rs:68,70`), and
`bit_not` is implemented in CVM1, FastVM, both AOT backends and AOT-C (CRUSH-232).

## Reproduction — measured on `3ae0b4a`, identical on **both** frontend entry points

| Program | `Compiler::new().compile` | `crush_frontend::compile_cast` |
|---|---|---|
| py `y = +5`, `+x`, `+(1 + 2)` | REFUSE `Unsupported op: +` | REFUSE `Unsupported op: +` |
| py `print(~5)` | REFUSE `Unsupported op: ~` | REFUSE `Unsupported op: ~` |
| js `+x`, `+"5"` | REFUSE `Unsupported op: +` | REFUSE `Unsupported op: +` |
| js `~5` | REFUSE `Unsupported op: ~` | REFUSE `Unsupported op: ~` |
| py `-x`, `not x`; js `!x` (controls) | OK | OK |
| CAST `__crush_pos__(5)` / `__crush_bit_not__(5)` | OK → `5` / `-6` | OK → `5` / `-6` |

Probe: walker → compile → VM, per door, with the C walker's intrinsic route constructed directly.

Not a silent bug: it is a loud compile error, and **both doors agree** — this is *not* CRUSH-111's
checker/compiler drift (there, one door refuses what the other accepts). It is a plain
missing-lowering gap, the mirror image of the C walker's working route.

## Scope note: Crush source is unaffected

The Crush parser only accepts `-` and `not`/`!` in prefix position
(`crates/crush-frontend/src/parser/mod.rs:2069-2084`); `fn main() { let x = +5; … }` fails to parse
(`unexpected token in expression: +`). Only walker input (Python/JS) can produce these nodes.

## The semantics call this needs (why it isn't just a one-line identity arm)

- **Python `+x`** is identity on numbers and a `TypeError` otherwise → the C walker's
  `__crush_pos__` (identity) is exactly right.
- **Python `~x`** is `-x-1` on ints → `bit_not` is exactly right.
- **JS `+x` is ToNumber coercion**, not identity: node prints `5` for `+"5"` and `NaN` for `+"abc"`.
  Lowering JS `+` to identity would leave the *string* on the stack and print `"5"` — **silently
  wrong**. JS `~x` is ToInt32 (`~1.5` → `-2`), likewise not plain `bit_not` on a non-Int.
- So a compiler arm `"+" => identity` would fix Python and quietly break JavaScript. The two
  languages must not share one lowering here.

## Proposed fix

1. **Python (the actual gap):** lower `+`/`~` to the intrinsics the C walker already uses
   (`__crush_pos__` / `__crush_bit_not__`) instead of `UnaryOp{"+"|"~"}` — proven on both doors and
   every engine, no compiler change. (Equivalent alternative: keep the CAST node for Python and add
   compiler arms for `"+"`/`"~"`, but then the JS walkers must not emit those nodes.)
2. **JS:** either implement the coercion (`+"str"` → number, `~x` → ToInt32) with node-parity tests,
   or keep today's loud refusal and record the decision in CRUSH-zorro-12/13's number model. Do
   **not** paper over it with identity.

## Success criteria

- [ ] `y = +5`, `+x`, `+(1 + 2)` and `print(~5)` compile and run through **both** entry points, and
      through at least one AOT backend from the walker corpus.
- [ ] JS `+x` / `~x`: coercion implemented with `+"5"` → `5`, `+"abc"` → `NaN`, `~1.5` → `-2`
      matching node — or an explicit, recorded decision to keep refusing.
- [ ] No path lowers JS `+` to identity (a regression test asserts `+"5"` is not `"5"`).
- [ ] `cargo test -p crush-frontend -p crush-lang-python -p crush-lang-js -p crush-aot` green.

## Files in scope

- `crates/crush-lang-python/src/lower_expr.rs:110-127` (the Python fix)
- `crates/crush-lang-js/src/lower_swc.rs:1248-1256`, `crates/crush-lang-js/src/lower/expr.rs:51-60`
  (JS half, only with the coercion decision)
- `crates/crush-frontend/src/compiler.rs:1713-1725` (only if the intrinsic route is *not* taken)

## Non-goals

- Unary `+` / `~` in Crush source (parser change; see CRUSH-128 for the prefix-operator precedent).
- Binary `^` (XOR) — `compiler_tests.rs:706` asserts it stays `Unsupported op`; unrelated.
- Optimizer constant-folding of `+`/`~` before the VM.

## Related

- **CRUSH-111** — adjacent but a different mechanism: there the *checker* refuses what the compiler
  accepts (`make_range`). Here both refuse. Recorded in CRUSH-111 as an out-of-scope finding.
- **CRUSH-zorro-12 / CRUSH-zorro-13** — JS values/operators and the number model; own the coercion
  semantics the JS half depends on.
- **CRUSH-232** — `bit_not` lowering is complete across engines, so the intrinsic route is safe.
- **PR #263** review (vega, 2026-10-16) — where this was measured.

## Dispatch

Not yet dispatched. When it is:

- Branch: `agent/<horse>/CRUSH-271`
- Repo: `crush-ast`
- Turns: `30`
- Runner: `claude`
- Scope: this ticket file is the canonical spec; verify the repro on current `main` first (RULES §1),
  then take the Python intrinsic route. Do not touch `crates/crush-vm/src/fastvm/` or
  `crates/crush-vm/src/python.rs`.
- Verification: satisfy the success criteria with test evidence and quote the real post-commit `HEAD`.
- Halt: stop and report if the JS coercion decision blocks the Python half, if the intrinsic route
  turns out not to reach an AOT backend, or if `walkers.rs`-level tests disagree across engines.
