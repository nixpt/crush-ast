# CRUSH-xps-9 — Typed Rust Debug formatting and exact macro evaluation

| Field | Value |
|-------|-------|
| **ID** | CRUSH-xps-9 |
| **Priority** | P1; closes the last formatting silent entry |
| **Status** | Backlog |
| **Phase** | M2 / Rust parity |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-xps-3, CRUSH-xps-5, CRUSH-xps-6, CRUSH-xps-7; CRUSH-xps-8 for numeric descriptors |
| **Coordination** | CRUSH-xps-8 numeric facts; CRUSH-xps-12 value-block boundary; CRUSH-xps-13 shared C bridge |
| **Estimated effort** | L; formatter and macro-evaluation slices |
| **Spec** | [Rust parity completion](../designs/rust-parity-xps-8-13.md#xps-9-typed-debug-and-macro-evaluation) |

## Problem and current evidence

The walker accepts `{:?}` but currently passes its operand to generic string
assembly. `audit_debug_string` prints `hello` instead of the quoted Rust Debug
string. Runtime representation alone cannot distinguish Rust char from String
or unit from generic null. A formatter that quotes every runtime string is
therefore insufficient.

The shared macro parser also reconstructs argument expressions at placeholder
sites. Repeated/reordered references to effectful arguments need an audit:
Rust evaluates arguments once in source order, not once per output occurrence.
Those cases are proposed coverage; this planning update does not claim a live
reproduction or a fix for them.

## Contract

- Keep Display and Debug separate. Existing `conv.text(value, "rust")` retains
  the supported Display contract and its two-argument ABI.
- Preserve Rust type and formatting intent through lowering. Initial Debug
  domain: Bool, supported integers, f64, str/String, char, unit and bounded
  acyclic sequences of supported elements.
- Proposed pure stdcap: `rust.debug(value, descriptor) -> String`, with a
  compiler-produced literal JSON type tree. Validate descriptors, type/value
  agreement, output quota and traversal bounds. Unknown descriptors/types
  fail; there is no generic text fallback.
- Share the Rust formatter kernel between VM and Rust AOT. C uses a shared
  kernel bridge only for representable values; otherwise refuse compilation
  and retain a targeted xps-9 AOT policy. Do not add a separate C approximation.
- Preserve `format!`'s owned String result even when its format is literal-only;
  teach the value pass the new formatter's String result class.
- Evaluate source macro arguments once, in source order. If the existing IR
  cannot materialize effectful repeated/reordered arguments, reject those uses
  at lowering. Do not synthesize a closure to emulate a generic value block.
- Audit print macro routing: newline-free `print!` and stderr `eprint!`/
  `eprintln!` need faithful output or source refusal. Documenting the difference
  alone does not satisfy the future normal `.rs` runner's boundary.
- Other widths/precision/alternate formats, f32 with erased precision, unknown
  custom Debug types, maps, structs/enums, cyclic or unbounded values remain
  explicitly unsupported until independently proven.

Debug output is tied to the recorded rustc/toolchain profile. Rust's standard
Debug implementations are not a stable cross-version serialization format.

## Implementation slices

1. Retain literal/annotation/format intent and classify the supported Debug
   types. Reuse binding identity and numeric facts; reject unknown pairs.
2. Add the shared formatter kernel and explicit stdlib registration. Integrate
   Rust AOT via the same source; give C an explicit support/refusal boundary.
3. Audit all positional, inline-name and mixed Display/Debug placeholder use
   in the common macro parser. Materialize arguments once or reject unproven
   effectful repetition/reordering. Preserve user scope and capture behavior.
4. Expand the shared corpus and remove the string Debug silent exception only
   after all engines satisfy their exact-or-loud gates.

## Proposed fixture matrix

| Case | Required behavior |
|---|---|
| `audit_debug_string` | Quoted `"hello"` with newline, or source-located refusal |
| Empty str/String; quote, slash, newline, tab, NUL and Unicode | Rust quoting/escapes exactly; explicit C representation refusal where necessary |
| char versus one-character String | Single versus double quotes, including Unicode scalar values |
| bool, supported integer, f64, negative zero, unit | Rust Debug representation or explicit type-specific refusal |
| Flat/nested sequences and mixed format placeholders | Correct recursive quoting and delimiters for supported type descriptors |
| Repeated `{0:?}` with a call; reordered `{1:?} {0:?}` calls | Exactly-once source order or frontend refusal |
| Unsupported alternate/width/precision formats, unknown types | Useful source-located diagnostic before execution |
| `print!`, `eprint!`, `eprintln!` | Faithful stdout/stderr/newline behavior or explicit source refusal; use separate stderr oracle coverage |
| Large/deep data and denied stdlib registration | Quota/permission behavior remains distinct from unsupported-language policies |

A later implementer must derive expected text from actual rustc output, record
its version and re-run the freshness check. No expected-output artifact is
minted from assumptions in this design PR. The current shared corpus requires
empty stderr, so stderr-routing cases need a separate paired-stream oracle or
explicit frontend rejection tests rather than incompatible corpus entries.

## Acceptance

- [ ] Every accepted Debug value matches rustc; unknown type/format pairs are
      rejected before VM execution with line:column and the fixing ticket.
- [ ] Char, String, unit and numeric precision remain distinguishable through
      source lowering. Formatter results retain their owned String class.
- [ ] Repeated/reordered placeholder cases cannot duplicate/reorder source
      effects; mixed Display/Debug parsing uses the same evaluation contract.
- [ ] Accepted print macros preserve their output channel/newline contract;
      unsupported channel forms are refused before execution.
- [ ] Formatter kernel/bridge behavior is shared, including validation and
      output/depth limits; unsupported AOT domains refuse before loading code.
- [ ] The silent Debug-string policy is removed only after VM/Rust AOT/C AOT
      checks pass; whitespace and escapes remain byte-exact.
- [ ] Live SDK and supported AOT proof, rustc profile, docs and behavioral
      release notes accompany the implementation.

## Dispatch and validation

Refresh the integrated SDK stack into an isolated xps-9 worktree. Inspect
`lower_expr::format_args_to_expr`, type/value metadata, `crush-vm::num_text`,
SDK stdlib registration, and both AOT formatters. Reproduce the pinned string
case; trace argument evaluation before designing temporary storage. First
implement the supported kernel/refusal boundary, then expand macro evaluation.
Use the shared SDK/AOT corpus and narrow diagnostic policies. Record a live
source-to-SDK/rustc comparison with quoted/escaped text and multiple arguments.

## Non-goals

Full formatting traits, user-defined Debug implementations, general value-block
lowering, new output-transport APIs, or enabling the `.rs` CLI early. Existing
unsupported output-channel forms still require source refusal in this ticket.
