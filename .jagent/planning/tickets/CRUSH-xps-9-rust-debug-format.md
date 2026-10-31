# CRUSH-xps-9 — Typed Rust Debug formatting and exact macro evaluation

| Field | Value |
|-------|-------|
| **ID** | CRUSH-xps-9 |
| **Priority** | P1; closes the last formatting silent entry |
| **Status** | In Progress; shared runtime kernel and conservative descriptor proofs |
| **Phase** | M2 / Rust parity |
| **Assignee** | nixp (user-directed) |
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

## First implementation slice

Branch: `agent/nixp/CRUSH-xps-9`, initially based on `fcb2224` after #244 merged;
#246 was still open when this work started. This independent first slice
establishes the macro/refusal boundary before dynamic numeric descriptors land.

- Fold typed literal Debug from the Rust AST: str, char, bool, representable
  integers and finite f64, unit and bounded literal arrays. Rust char/string
  identity survives until formatting; escaping uses the Rust formatter of the
  recorded toolchain, not generic Crush text conversion.
- Bound compile-time traversal to depth 16 / 256 nodes and each Debug result to
  1 MiB. Dynamic values, including variables/calls and inline Debug captures,
  fail at their source location pending the typed runtime formatter.
- Keep Display on the existing `conv.text(value, "rust")` ABI. If arguments can
  have effects, require one use each in source order and no inline captures;
  repeated/reordered cases are refused rather than duplicating/reordering calls.
  Named argument materialization and unused arguments are explicitly refused.
- Preserve owned String identity for empty and literal-only `format!`.
- Refuse `print!`, `eprint!`, and `eprintln!` in both statement and expression
  positions until newline/channel routing can be faithful.
- Promote `audit_debug_string` to exact on all three engines. Dynamic float
  Debug in `float_display` becomes a targeted xps-9 refusal, preserving its
  existing oracle output for the future runtime formatter.

This does not add `rust.debug` or close xps-9. Remaining slices add the shared
runtime kernel, literal descriptors from binding/type facts, explicit stdlib
registration, dynamic aggregate limits and the C bridge/refusal boundary.

### Slice validation

Rebased onto integrated main `23d7550` (explicit @lang outputs, #245).
Rust oracle: `rustc 1.98.1 (48a229cea 2026-09-01)`, x86_64, Rust 2021 with `-O`.

- 486 focused tests: 8 walker, 115 shared frontend, 343 SDK, 20 AOT.
- 76-fixture oracle inventory: VM 39 exact / 36 loud / 1 silent; each AOT
  31 exact / 44 loud / 1 silent. All eight added expected-output artifacts are
  generated from native rustc; the freshness test recompiles every fixture.
- C parity also passes `CRUSH_GC_STRESS=1`. The remaining silent case is integer
  overflow (xps-8); the Debug-string exception is removed rather than relaxed.
- Standalone SDK host matches fresh native rustc for literal Debug, Display-only
  floats, ordered effectful arguments and the original Debug-string audit.
  Dynamic Debug/repeated/reordered arguments and unsupported output macros
  refuse before output. A `.crush` smoke still prints and returns Int(42).
- Scoped Rust walker Clippy passes the CI `never_loop` gate with existing
  warnings. Targeted formatting and `git diff --check` pass.
- Full workspace tests were not run. A dedicated writable target isolates this
  worktree from other checkouts and uses two build jobs.

## Shared formatter slice

Branch: `agent/nixp/CRUSH-xps-9-shared-debug`, following #247. Vega owns merging
#246/#247; they were still open while this implementation was developed.

- Implement a std-only typed Debug kernel in crush-vm and embed that exact source
  in Rust AOT. Both adapt borrowed values through the same View interface.
- Add explicit pure stdlib registration for `rust.debug(value, descriptor)`;
  no ambient builtin registration or `conv.text` ABI change.
- Validate bounded JSON descriptors, supported integer widths, char scalar
  identity and runtime value agreement. Traverse without recursively cloning
  graphs; detect cycles/borrow conflicts and enforce depth 16, 1024 visited
  values, 4096 descriptor bytes and 1 MiB streamed output.
- Resolve dynamic descriptor proofs after lexical binding identity. Validate
  compatible direct-call arguments, compute acyclic return summaries, invalidate
  writes/captures, preserve char Copy and formatter String ownership. Consume
  canonical numeric metadata when available without treating annotations alone
  as canonical-value evidence.
- Refuse unproven predicates/indices, loops/matches, closure Debug, iterators,
  `.powi()`, and unsafe wide comparisons. Builtin/helper named and effectful
  multiargument source calls remain conservative until the integrated xps-8
  call contract is consumed. Richer inference/materialization remains open.
- C explicitly refuses dynamic `rust.debug` calls before producing artifacts;
  a separately written C formatter is not introduced. Five C-only policies pin
  the bridge boundary while VM/Rust AOT run these programs exactly.
- Add six fresh rustc corpus fixtures; promote dynamic string and float Debug
  from all-engine refusals to exact VM/Rust AOT behavior.

Validation (Rust 1.98.1, x86_64; oracle edition 2021 with `-O`):

- 774 focused tests pass: 281 VM, 8 Rust walker, 348 SDK, 115 frontend and
  22 AOT (including direct single-call kernel/embedded-source tests).
- 82 freshly checked rustc fixtures: VM 44 exact / 37 loud / 1 silent;
  Rust AOT 36 exact / 45 loud / 1 silent; C AOT 31 exact / 50 loud / 1 silent.
  Five C-only bridge policies are explicit; integer overflow remains silent
  under xps-8. No Debug silent exception is reintroduced.
- C parity also passes `CRUSH_GC_STRESS=1`. Kernel tests cover invalid JSON,
  type/width mismatches, duplicate/unknown keys, cycles, aliased acyclic graphs,
  mutable-borrow conflicts, and descriptor/traversal/output limits.
- Live public SDK and Rust AOT match native rustc for dynamic text (8 bytes),
  mixed float formatting (70), scalar/char/unit/shadowing (85), sequences (121),
  and runtime NaN/inf/negative zero (17). C refuses all five before artifacts;
  three source-proof failures refuse before output. `.crush` still returns
  Int(42). A temporary standalone Cargo host keeps linked crate versions coherent.
- Scoped Clippy passes `never_loop`; test targets retain an allowance for
  existing unrelated `approx_constant` fixtures in VM/SDK tests. Existing
  warnings remain. Targeted formatting and `git diff --check` pass.
- Full workspace tests were not run; validation covers the changed VM/SDK/Rust
  pipeline, shared frontend and both AOT paths. Dedicated target, two jobs.

This slice does not close xps-9: the C bridge, richer numeric/control/call proofs
and argument materialization remain follow-up work.

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
