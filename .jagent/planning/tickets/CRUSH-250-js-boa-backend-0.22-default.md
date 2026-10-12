# CRUSH-250 — crush-lang-js: Boa parser backend → 0.22 and default for `.js`

| Field | Value |
|-------|-------|
| **ID** | CRUSH-250 |
| **Priority** | P1 — first step of the browser's "drop boa_engine" path |
| **Status** | Backlog |
| **Phase** | M6 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | M |

## Problem

surfer-browser wants page JS on the crush path (bozo frontend → CASM → CVM1/FastVM/JIT/AOT) so it can
drop `boa_engine` (≈153k LOC, 67 crates of which 43 nothing else in surfer uses) and keep only Boa's
**parser** (`boa_parser`+`boa_ast`+`boa_interner`, ≈54k LOC, 65 crates). crush-lang-js already has a
Boa backend, but it is pinned to **0.21** (`crates/crush-lang-js/Cargo.toml:29-31`) while surfer uses
**0.22**, it is off by default (`default = ["swc-backend"]`), and its lowering is thinner than swc's
(`lower_boa.rs` 930 LOC vs `lower_swc.rs` 1521; classes → empty `StructDef` at `lower_boa.rs:364-370`,
async/generator expressions → `null` at `:758-762`). With swc as the JS path, surfer also carries swc's
147-crate closure through `crush-lang-sdk`'s default `polyglot-javascript` feature.

## Success criteria

- [ ] `boa_parser`/`boa_ast`/`boa_interner` bumped to **0.22** (one Boa parser version across the workspace + surfer).
- [ ] A feature layout where `.js/.mjs/.cjs` can parse with Boa **without swc in the dependency graph**
      (e.g. `js-boa` feature; swc kept for `.ts/.tsx` behind `js-swc`/`ts`). `cargo tree -e normal`
      of a `--no-default-features --features js-boa` build shows no `swc_*`.
- [ ] `lower_boa.rs` reaches parity with `lower_swc.rs` on the existing JS test corpus (73 tests) — every
      test runs under both backends (`#[rstest]`-style matrix) with identical CAST output or a listed, justified diff.
- [ ] Unsupported constructs return a typed `Unsupported { construct, span }` error, never a silent `null`/drop.

## Technical approach

- Bump the three crates; fix API drift (0.21→0.22 `Interner`, `Source`, `StatementList` changes).
- Split features: `js-boa` (parser only), `js-swc` (current), `ts` (swc); `crush-lang-sdk`'s
  `polyglot-javascript` → `js-boa` by default, add `polyglot-typescript` → swc.
- Run the lowering test matrix over both backends; fix `lower_boa` gaps (classes, multi-declarators,
  destructuring, template literals) to match swc; anything not lowerable → `Unsupported`.
- Note: real JS semantics (prototypes, closures, `this`) are **not** this ticket — they belong to the
  bozo frontend (bozo `BOZO-M7-1`) + VM primitives (CRUSH-251). This ticket only makes the Boa
  parser the shared, lightweight front door.

## Files to modify

- `crates/crush-lang-js/Cargo.toml` — versions, features
- `crates/crush-lang-js/src/{lib.rs,backend/boa.rs,lower_boa.rs,analyzer_boa.rs}`
- `crates/crush-lang-sdk/Cargo.toml` — `polyglot-javascript` / `polyglot-typescript` features
- tests under `crates/crush-lang-js/tests/`

## Cross-refs

surfer-browser `SURF-ENGINE-2` (plan), `SURF-BUILD-1` (drop swc from surfer); bozo `BOZO-M7-1`.
