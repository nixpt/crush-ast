# CRUSH-392 — Binary CASM (.casmb) round-trips functions via named encoding

| Field | Value |
|-------|-------|
| **ID** | CRUSH-392 |
| **Priority** | P2 |
| **Status** | Done |
| **Phase** | M1 |
| **Assignee** | opencode |
| **Dependencies** | CRUSH-365 (one extension per format; its `.casmb` refusal guard) |
| **Estimated effort** | S |

> Note: GitHub issue **#372**; the ID CRUSH-372 is taken, so this work is
> numbered CRUSH-392. Cite both in commits/docs.

## Problem

`casm::Program::serialize(Format::Binary)` used `rmp_serde::encode::write`
(structs encode **positionally**, as arrays), while `Instruction` carries
`#[serde(flatten)] args`, which needs a self-describing **map** encoding. The
positional decoder therefore refused any `.casmb` holding a function body:
`invalid type: sequence, expected a map`. Only function-less programs
round-tripped, so CRUSH-365/#365 added a guard: `crushc --emit casm -o
x.casmb` refuses rather than write an unloadable file.

## Success criteria

- [x] A program WITH functions round-trips: serialize → deserialize → same
      semantics; end-to-end `crushc --emit casm -o hi.casmb` → `crush-run run
      hi.casmb` prints 42.
- [x] Old positional (structs-as-arrays) `.casmb` files still deserialize.
- [x] The #365 refusal guard is lifted; its CLI test asserts the new reality.
- [x] `docs/FILE-FORMATS.md` reflects the named (structs-as-maps) encoding.
- [x] `cargo test -p casm -p crush-lang-sdk` green; no new clippy warnings.

## Technical approach

- Switch `Format::Binary` serialization to `rmp_serde::encode::write_named`
  (structs as maps). Decoding is unchanged: `rmp_serde`'s decoder accepts both
  maps (named) and arrays (positional) for structs, so old four-field files
  still load and nothing loadable is lost (every function-bearing `.casmb` in
  the wild was already unreadable).
- Update the intentionally-changed byte-identity test in
  `crates/casm/tests/cosm_kind_table.rs` with a comment on the encoding
  decision; make its binary tests round-trip functions.
- Lift the `crushc` guard and rewrite `binary_casm_is_refused_rather_than_
  written_unloadable` as an end-to-end round-trip test.
- Update `docs/FILE-FORMATS.md` and the in-source comments that describe the
  positional encoding.

## Files to modify

- `crates/casm/src/lib.rs` — `write_named`; update encoding comments.
- `crates/casm/tests/cosm_kind_table.rs` — round-trip functions; commented
  byte-identity change.
- `crates/crush-lang-sdk/src/bin/crushc.rs` — remove the refusal guard.
- `crates/crush-lang-sdk/tests/formats_cli.rs` — `.casmb` end-to-end test.
- `docs/FILE-FORMATS.md` — truthful `.casmb` encoding description.

## Non-goals

- Dropping `#[serde(flatten)]` on `Instruction` (bigger change across every
  consumer; the issue itself favors the encoding fix).
- JSON encoding changes (`casm_json_without_the_field…` stays byte-identical).

## Resolution

Adopted `write_named` (2026-11-12, branch `agent/opencode/CRUSH-392`). The
decoder was already two-way (maps and arrays both decode as structs), so
positional files — the only `.casmb` ever loadable — keep loading, and the
flattened `Instruction` args now round-trip.

Changed:

- `crates/casm/src/lib.rs` — `Program::serialize(Format::Binary)` uses
  `rmp_serde::encode::write_named`; encoding comments updated.
- `crates/casm/tests/cosm_kind_table.rs` — binary tests round-trip functions
  (new `a_program_with_functions_round_trips_through_binary_casm`); the
  byte-identity assertion in `old_binary_casm_still_loads` intentionally
  replaced (old positional file still loads; new output is a map, not the
  same array) with a comment on the encoding decision.
- `crates/crush-lang-sdk/src/bin/crushc.rs` — #365 refusal guard lifted.
- `crates/crush-lang-sdk/tests/formats_cli.rs` —
  `binary_casm_is_refused_rather_than_written_unloadable` →
  `binary_casm_round_trips_and_runs` (compile → load → run, prints 42).
- `docs/FILE-FORMATS.md` — `.casmb` row now describes the named encoding and
  the two-way decoder.

Verified: `crushc hi.crush --emit casm -o hi.casmb` (434 bytes) →
`crush-run run hi.casmb` prints `42`; a two-function program likewise
(745 bytes → `42`). `cargo test -p casm` (43 tests) and `-p
crush-lang-sdk` (85 suites) green; `cargo clippy -p casm -p crush-lang-sdk
--all-targets` shows no warnings from the changed code.

Non-goals held: `#[serde(flatten)]` stays on `Instruction`; JSON encoding
untouched (`.casm` byte-identity test unchanged and still passing).
