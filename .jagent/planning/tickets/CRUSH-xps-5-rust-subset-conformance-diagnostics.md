# CRUSH-xps-5 — Gate Rust execution on exact-or-loud parity

| Field | Value |
|-------|-------|
| **ID** | CRUSH-xps-5 |
| **Priority** | P1 |
| **Status** | In review |
| **Phase** | M2 / conformance gate |
| **Assignee** | nixp (user-directed Rust SDK work) |
| **Dependencies** | CRUSH-xps-3 |
| **Estimated effort** | M |

## Problem

The Rust frontend currently has silently mis-lowered constructs. Exposing `.rs`
through the default runner before these cases are exact or rejected would make
incorrect behavior look supported. This phase is the mandatory gate before
CRUSH-xps-4 enables Rust in `crush run`.

## Success criteria

- [x] VM fixtures compare exactly with `rustc`; the freshness test re-runs
      `rustc` so checked-in expected output cannot silently drift.
- [x] AOT fixtures compare exactly or fail loudly; add
      `crates/crush-aot/tests/rust_parity.rs`.
- [x] A known-loud allow-list verifies expected failures and fails with a
      "now exact — remove it" message when parity is fixed.
- [x] Known-silent cases are keyed to fixing tickets; an unlisted silent
      divergence fails the audit.
- [x] Initial known-silent entries link to CRUSH-xps-6 through CRUSH-xps-12;
      `.rs` CLI work stays blocked until those tickets close.
- [x] Unsupported syntax reports source location, construct, and current
      limitation; unsupported constructs never lower to a no-op.
- [x] The accepted subset and material semantic differences are documented;
      parser acceptance alone does not count as support.
- [x] Initial corpus includes block shadowing, `Vec`/`String`/struct value
      semantics, integer widths/casts/overflow, and `{:?}`; fixing these
      families is tracked separately in CRUSH-xps-6 through CRUSH-xps-12.

## Technical approach

- Copy the exact-or-loud harness shape used by the Python and JS parity work.
- Extend the existing rustc-backed corpus and add the AOT parity twin.
- Keep the known-silent inventory linked to feature-fix tickets; do not mark
  those entries exact until their VM and AOT parity proofs pass.
- Keep unsupported-feature tests fail-closed and remove each known-silent entry
  only when the VM and AOT behavior is exact.

## Non-goals

- Full Rust type checking or arbitrary Rust/Cargo compatibility.
- Enabling `.rs` in the ordinary runner before this ticket is complete.

## Implementation and evidence

- Shared policy and 29 rustc-backed fixtures run through the public SDK API and
  both AOT backends. Comparison preserves stdout bytes; real rustc freshness
  requires a successful compiler and fixture exit instead of skipping.
- Five pinned silent outputs link to xps-6–9. Eight shared rejection cases
  include import/type/value-block/closure/string limitations. AOT-only closure
  exclusions link to CRUSH-251; mapped-capability rejections discovered by the
  audit are tracked in CRUSH-xps-13.
- Unit tests prove unknown divergences and failures fail the gate, whitespace
  matters, fixed exceptions fail with “now exact”, and changed failure modes
  cannot satisfy the old exception. Orphan policy entries also fail.
- Lowering and parse errors report source coordinates; unsupported expressions
  name the construct without dumping AST debugging output to stderr.
- `.rs` runner dispatch remains blocked on closing xps-6–12. Green inventory
  tests are not a claim that the known-silent cases are supported.
- Validation: SDK/Rust regression suite 571 passed; final SDK corpus target
  11 passed (including real rustc freshness); AOT corpus target 6 passed;
  C corpus under `CRUSH_GC_STRESS=1` passed; scoped library/binary clippy passed
  with existing warnings.
- Live host proof: closure_blocks.rs through a separately compiled Rust SDK
  host matched actual rustc byte-for-byte; a .crush file printed
  `Crush from Rust` and returned `Some(Int(42))`; unsupported Rust value block
  failed at 2:13 before VM execution.

See [Rust conformance](../../../docs/rust-conformance.md) for the precise
supported boundary and reproduction commands. This branch depends on PR #213.
