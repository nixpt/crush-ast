# CRUSH-27 — `@module`/`@invariants`/`@errors`/`@reads`/`@writes`/`@covers` as formal CAST node types

| Field | Value |
|-------|-------|
| **ID** | CRUSH-27 |
| **Priority** | P1 — foundational for M5; blocks CRUSH-28, CRUSH-29, CRUSH-30, CRUSH-31 directly |
| **Status** | Done |
| **Phase** | M5 |
| **Assignee** | unassigned |
| **Dependencies** | `.jagent/planning/ROADMAP.md` M5 section; `docs/design/ai-native-roadmap.md` Steps 1-3 |
| **Estimated effort** | L |

## Origin

Filed s394 (2026-07-23) from `.jagent/planning/ROADMAP.md` M5 section.
The seven `@`-annotation forms are the language-level surface of the
"AI-native compiler layer" — the type-system surface that lets AI writers
declare and agents query contracts, invariants, error surface, and coverage
without relying on free-text comments. Without formal CAST node types and a
parser/compiler that emits them, `crush-index` (CRUSH-28), `codebase.*` host
caps (CRUSH-29), and the `@exhaustive-match-sites` lint (CRUSH-30) have no
anchor. **This ticket is the load-bearing foundation of the M5 ladder** — the
reason it's first per user instruction.

## Problem

`ai-native-roadmap.md` proposes (verbatim):

> Make contracts, relationships, and invariants **explicit language constructs**
> — not documentation that gets stale, but compiler-checked nodes in the CAST
> that agents query for free.

None of the seven annotations — `@module`, `@invariants`, `@errors`,
`@reads`, `@writes`, `@covers`, `@exhaustive-match-sites` — exist as CAST
node types today:

- `crush-cast` has no `Annotation` ladder enum (no `ModuleAnnotation`,
  `InvariantAnnotation`, `ErrorAnnotation`, etc.)
- `crush-frontend`'s parser doesn't recognize the `@` token in first-class
  annotation positions
- The compiler doesn't emit annotation nodes into CASM output (annotations
  are dropped between CAST and CASM, even if the parser were extended today)

Without this foundation, `crush-index` (CRUSH-28) has no authoritative
annotation data to ingest, and `codebase.*` caps (CRUSH-29) query against
an empty schema. The "AI-native" thesis is structurally fiction today.

## Success criteria

- [ ] `crush-cast` declares `ModuleAnnotation`, `InvariantAnnotation`,
      `ErrorAnnotation`, `ReadAnnotation`, `WriteAnnotation`,
      `CoverageAnnotation`, and `ExhaustiveMatchSitesAnnotation` as formal
      enum variants in `crush_cast::Annotation` (the canonical ladder node).
- [ ] `crush-cast` derives `Serialize`/`Deserialize` for each, with stable
      `#[serde(rename)]` predicates so existing JSON-encoded CASTs remain
      forward-compatible (a missing annotation serializes as `null` /
      `Option::None`, not a deserialization error).
- [ ] `crush-frontend` parser recognizes the seven `@`-forms in both
      first-class positions (above `fn`, `struct`, top of file) and inside
      `AnnotationList` blocks; emits `ParseError::UnknownAnnotation { name,
      location }` for malformed forms — not a panic, not a silent drop.
- [ ] `crush-frontend` compiler emits annotation nodes into CASM output as
      part of the AST→CASM pass; annotations are not lost between CAST and
      CASM (smoke test: parse a 5-line example with `@module { purpose: "x"
      }`, compile, `--emit json` output contains the `ModuleAnnotation`
      node at the expected position).
- [ ] 30+ unit tests across `crush-cast` and `crush-frontend` (roundtrip
      per variant, parse-recognize per `@`-form, parse-reject for each
      malformed form, compiler-pass-through integration).

## Technical approach

1. **`crush-cast` types.** Add an `Annotation` enum (or extend the existing
   `crush_cast::Node` ladder) with the seven variants. Field shapes follow
   the `@`-form examples in `ai-native-roadmap.md`'s "Proposed Annotations"
   section verbatim — no design decisions needed; the roadmap already has
   the canonical shapes. Keys for `@module` → `purpose`,
   `exports`, `invariants`, `related`; for `@invariants` →
   `applies_to`, `reason`, `consequence`; etc.
2. **Parser.** Extend `crush-frontend/src/parser/mod.rs` with a `@`
   token recognized in `first_class_annotation` (above `fn`/`struct`/
   `module`) and inside `AnnotationList` blocks. The seven forms share a
   small dispatcher keyed on the identifier following `@`. Malformed
   forms fail with `ParseError::UnknownAnnotation`.
3. **Compiler emit.** `crush-frontend/src/compiler/mod.rs`'s AST→CASM
   pass intercepts annotation nodes and emits them alongside their
   anchor. The CASM representation needs an `AnnotationSection`
   opcode+metadata.
4. **`crushc` smoke.** Ensure `--emit json` (already supported) round-trips
   annotations through.
5. **Tests.** `crush-cast` roundtrip tests (one per variant, 7+),
   `crush-frontend` parse tests (one per `@`-form, plus rejection tests),
   compiler pass-through integration test.

## Files to modify

- `crates/crush-cast/src/lib.rs` — add `Annotation` enum ladder
- `crates/crush-frontend/src/parser/mod.rs` — `@`-token recognition + 7 dispatch cases
- `crates/crush-frontend/src/compiler/mod.rs` — emit `AnnotationSection` into CASM
- `crates/crush-cast/src/serde.rs` (or equivalent) — stable serde names + `Option` for backward-compat
- `crates/crush-frontend/tests/` — parse + emit regression tests

## Non-goals

- **No runtime semantics.** `@-annotations` are metadata for `crush-index`
  and `codebase.*` caps to read; they have no VM-side enforcement in this
  ticket. Runtime enforcement, if any, lives downstream in M7 hardening.
- **No query semantics.** This ticket gets annotations *into* CASM. The
  *query* layer (`codebase.invariants` returning real values) is CRUSH-29.
- **No dejavue integration.** dejavue ↔ crush-index change-feed joins are
  CRUSH-31.
- **No `@exhaustive-match-sites` lint logic.** Just the *parser/compiler
  emit* for the lint's input; the lint itself is CRUSH-30.
- **No CASM version bump.** Annotation nodes are forward-additive; older
  CASM-loaders must continue to work (annotations omitted = `null`).

## Cross-references

- `.jagent/planning/ROADMAP.md` — M5 overall, this ticket #1 of 8
- `docs/design/ai-native-roadmap.md` — Steps 1-3 plan; this ticket
  implements them verbatim
- Sister tickets in M5: CRUSH-28 (depends), CRUSH-29 (depends),
  CRUSH-30 (depends), CRUSH-31 (depends), CRUSH-32 (parallel —
  VM-side), CRUSH-33 (parallel — DOM), CRUSH-34 (parallel —
  spawn/await/yield)
