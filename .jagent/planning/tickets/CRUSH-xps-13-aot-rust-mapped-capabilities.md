# CRUSH-xps-13 — Share Rust mapped capability semantics across VM and AOT

| Field | Value |
|-------|-------|
| **ID** | CRUSH-xps-13 |
| **Priority** | P1 |
| **Status** | In review — all slices folded into combined PR |
| **Phase** | M2 / Rust AOT parity |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-xps-3, CRUSH-xps-5, CRUSH-xps-7; CRUSH-xps-8 for numeric method dispatch |
| **Coordination** | CRUSH-xps-9 may use the same private C support bridge |
| **Estimated effort** | L; inventory, string kernels/bridge, typed numeric dispatch |
| **Spec** | [Rust parity completion](../designs/rust-parity-xps-8-13.md#xps-13-named-capabilities-and-typed-method-dispatch) |

## Original problem and evidence

Resolved named capabilities keep Rust local bindings named `str`, `math` or
`array` from changing dispatch. At filing, both AOT backends refused `str.to_lower`,
`str.contains` and `math.pow` in the shared corpus. Existing intrinsic/opcode
spellings do not establish the same argument/result/error contract.

Pinned AOT exceptions: `capability_namespace_bindings`, `int_arith`, `vecmut`.
The inventory is shared with the SDK; removing an exception requires exact
outputs on VM, Rust AOT and C AOT.

There is also a type-sensitive risk to verify: the SDK's generic `math.pow`
returns Float, whereas Rust integer `.pow` returns the integer receiver's type.
Printing `8` can hide the difference. Use a downstream operation such as
`2i64.pow(3) / 3` (Rust integer result `2`) when checking this mapping. This
candidate is identified from source inspection; it is not a claimed executed
reproduction in this documentation change.

## Capability and method contract

| Family | Required audit |
|---|---|
| lower/upper, contains, starts/ends, trim, replace | UTF-8/Unicode semantics, output type, empty strings, representation limits, invalid arity/types |
| abs, sqrt, floor, ceil, round | Rust receiver/result type and edge cases, not merely integer-looking stdout |
| integer `.pow` | Width-aware integer result and overflow profile from xps-8; generic float power is insufficient |
| float `.powi` and `.powf` | Distinct source operations; implement compatibly or reject, never alias just because a simple fixture matches |
| Other `lower_method` mappings | Explicit VM/Rust AOT/C AOT support/refusal row and owning ticket |

Existing SDK names retain their contracts. If a source method is incompatible,
select an existing truly compatible operation, add a separately named shared
kernel with explicit registration, or reject that source method. Do not silently
redefine `math.pow`, alias it to `math.power`, or change generic Crush arithmetic.

## Implementation plan

1. Inventory every mapped method and its source type requirements, capability
   name, arity, result type, errors, host registration and backend support.
   Audit current main's COSM/SDK changes before touching dispatch.
2. Factor dependency-light pure kernels out of the canonical VM/SDK operations.
   VM stdlib adapters and generated Rust call the same implementation/source.
3. Proposed C approach: link a private Rust support kernel through the AOT build
   step. Use validated scalar tags and UTF-8 pointer+length inputs, status plus
   owned-byte outputs, matching allocator free functions and no unwind across
   the C boundary. Copy text into rooted C-managed storage before releasing the
   Rust buffer. This ABI is private AOT plumbing, not `crush-ffi` plugin ABI.
4. Gate C representations explicitly. Embedded NULs, dynamic text of unproven
   representability, fixed-buffer overflow and unsupported Unicode handling
   must not silently truncate or switch to bytewise/locale behavior. Preserve
   compile-time refusals until exact support is established; general C Value
   ABI expansion needs a separate review.
5. Include kernel source/compiler/target/options in AOT artifact cache keys.
   Map typed numeric methods using xps-8 facts and supported operations only.
6. Remove each baseline AOT exception after all three engines match rustc;
   retain new unsupported domains with narrow, ticket-specific diagnostics.

A string-only slice can precede numeric dispatch. A bridge/kernel design must
be reviewed before expanding generated module linkage; no new peer dependency
or dependency back-edge is authorized by this specification.

## Proposed fixture matrix

| Case | Expected proof |
|---|---|
| Three pinned AOT exception fixtures | Exact output on all engines before removing their exceptions |
| Unicode lower/upper, expanding/contextual case mappings, search | Shared kernel output equals Rust/SDK, without locale-dependent shortcuts |
| Empty/absent/repeated search, prefix/suffix, trim and replace | Correct text/Boolean type and boundary semantics |
| Embedded NUL, large text, invalid UTF-8 at private ABI | Exact representable behavior or explicit pre-execution refusal; invalid ABI inputs are errors |
| Bindings named `str`, `math`, `array` | Source locals do not capture resolved capabilities |
| Integer power followed by division/modulo/comparison | Result type and width remain correct, including supported overflow behavior |
| Float powi/powf, negative powers, zero, numeric boundaries | Correct typed operation or source-located refusal; include nontrivial precision cases |
| Bad arity/types and missing stdlib registration | Match canonical validation; host denial is not a language exception |
| Repeated result allocation with `CRUSH_GC_STRESS=1` | Correct C roots, output ownership and freeing; normal collection also passes |

## Acceptance

- [x] Every mapped method has a reviewed semantic/backend support row.
- [x] Named capabilities use a shared kernel and preserve the canonical
      argument, result and error semantics for the supported domain.
- [x] Unicode/string representation and numeric result-type probes pass;
      no method is accepted solely because an ASCII/stdout-only example agrees.
- [x] Unsupported source domains fail at the Rust boundary, and unsupported
      AOT domains fail before generated code loads. Infrastructure failures,
      crashes and permission failures are not allowlisted language refusals.
- [x] The three original AOT exceptions are removed only after all engines
      match rustc; new domain restrictions remain explicit and ticket-linked.
- [x] Private ABI error/allocator/unwind behavior and cache invalidation are
      covered; C cases pass normal GC and `CRUSH_GC_STRESS=1`.
- [x] Live SDK/Rust AOT/C AOT output and updated conformance/release docs are
      recorded. The numeric slice uses xps-8 facts instead of erasing types.

## Dispatch and validation

Use an isolated xps-13 worktree after refreshing the SDK stack. Start at
`crush-lang-rust::lower_method`, SDK stdlib registration, VM kernels,
`crush-aot::{codegen,codegen_c}`, and the shared Rust policy. Validate actual
canonical capability behavior before choosing aliases. Reuse the existing
corpus plus direct kernel/ABI invalid-input cases; compare actual native
artifacts and the public SDK with rustc. Keep string and numeric slices small
and separately reviewable under this ticket.

## Non-goals

General Rust crates/traits, native plugin builds, new ambient grants, full Rust
ownership, AOT closure support, a public ABI redesign, or automatic CLI enabling.

## Slice 1 implementation

User-directed xps lane, branch `agent/nixp/CRUSH-xps-13-method-contracts`.
Full method inventory: [Rust method contracts](../../../docs/rust-method-contracts.md).
Numeric methods are conservatively source-refused (including valid f64 methods)
until typed proof/kernel support exists. Prefix/suffix predicates are refused pending
a Rust Bool adapter. String receivers and replacement patterns must be proven
strings; char literals retain distinct Copy identity. Existing SDK capabilities
and host grants retain their contracts; no new AOT linkage is introduced.

The power-related AOT-only corpus exceptions now fail at the source boundary
on all engines; `vecmut` retains its AOT-only lowercase refusal. The `numeric_cast_float_producer` exception now belongs to this earlier
method boundary. This ticket remains open for shared kernels, C bridge/cache
contracts, numeric dispatch and differential completion evidence.

### Slice 1 validation (2026-10-16)

- Offline Cargo build of an isolated host using the public SDK, Rust walker,
  frontend and both AOT compilers succeeded (existing dependency warnings).
- Live rustc edition-2021 optimized output matched SDK Unicode case expansion,
  Greek contextual lowercase, Unicode trim, empty-pattern replace and contains.
- Char copies, sequence length and integer casts matched rustc through SDK and
  executed Rust/C AOT shared libraries. String transforms retained explicit
  named-capability AOT refusals before artifacts.
- Thirteen valid Rust examples (eight numeric methods, prefix/suffix predicates,
  char patterns and char receiver) refused at line 2 with xps-13 diagnostics
  and empty stdout through SDK and both AOT compilation paths.
- Existing direct Crush math.pow, str.starts_with and str.to_lower output matched
  the pre-slice SDK host exactly, including Float `8.0` and integer flag `1`.
- Four old-host reproductions confirmed silent mismatches: integer power/division,
  integer abs/division, prefix and suffix Bool display. New source guards reject
  each. Evidence: `/tmp/crush-xps13-live.log` and
  `/tmp/crush-xps13-before-after.log`.
- Existing corpus policies/diagnostic assertions were updated. No test suite was
  run and no new tests were added in this slice.

### Refresh after #252 merged

Rebased onto main `3ae0b4a`. Rebuilt the public SDK/AOT host successfully.
Live shared Debug plus char-copy output was exact through SDK and executed Rust
AOT; C retained its explicit bridge refusal. All thirteen xps-13 source guards
remained intact on all three paths; Unicode strings and contains remained exact.
Evidence: `/build/tmp/crush-xps13-rebase-build.log` and
`/build/tmp/crush-xps13-rebase-live.log`. No test suite run after the rebase.

## Combined implementation (slices 2–4)

- `crush-vm::rust_methods_core` is std-only and embedded verbatim (documentation
  comments normalized) in Rust AOT and private C support. Canonical SDK string
  and mapped math handlers delegate to it; core contains/replace do too.
- Opt-in `rust.num` uses `(method, descriptor, receiver, exponent-or-null)` and
  xps-8 lexical facts, canonical numeric arguments and return summaries. Integer
  pow/abs wrap at source width under the existing optimized rustc oracle profile.
  u64/usize require representability proofs; f64 powi/powf remain distinct.
  `rust.str.starts_with` / `rust.str.ends_with` return Bool; SDK generic flags and
  math.pow Float semantics are unchanged.
- Native Linux C AOT links a generated private Rust static library. Its reviewed
  scalar/pointer+length ABI validates inputs, contains unwinding, returns owned
  output/error bytes and uses paired frees. C copies output into managed roots
  before the next collection. Support symbols are hidden from module exports.
- C requires proven string inputs (or immediate string literals), rejects NUL
  constants and external/character text producers before artifact generation,
  and never substitutes locale/bytewise string operations. No C Value layout or
  public plugin ABI changed; xps-9's C sequence Debug adapter remains separate.
- Artifact/support cache keys cover source, compiler identity, target and
  optimization/linkage options. Cross-device artifact publication stages a
  complete sibling file before atomic rename.
- Non-finite constant arithmetic stays at runtime because JSON CAST/CASM cannot
  encode infinity/NaN constants. This preserves runtime arithmetic rules.
- Original xps-13 exceptions plus numeric_cast_float_producer are removed after
  native parity. New fixtures cover Unicode/patterns, large strings, integer
  widths, float edges, NUL and explicit proof refusals. The oracle sweep releases
  native binaries after checking each fixture to bound builder disk usage.

Design and support matrix: [method contracts](../../../docs/rust-method-contracts.md),
[private bridge review](../../../docs/rust-aot-support.md).

### Completion validation

All commands use the owned `/build/tmp/crush-xps9-target`, two build jobs,
incremental off, dev/test debug info off and `RUSTC_WRAPPER=`. `TMPDIR=/tmp`
keeps transient native audit artifacts off the constrained build volume.

- `cargo test --offline -p crush-vm -p crush-lang-rust -p crush-frontend --lib`:
  283 VM + 11 walker + 110 frontend tests passed.
- SDK default-feature library and Rust binding/Debug/formatting/method/numeric/
  source/corpus and polyglot-executor tests: 364 passed, including fresh optimized rustc oracles.
- AOT library, backend agreement, deterministic generation, direct Debug,
  method results and shared Rust/C corpus audits: 28 + 12 + 1 + 2 + 3 + 6 passed.
  The corpus now contains 112 fixtures. Native compiler crashes or infrastructure
  failures remain errors, never ticket-specific allowlisted outcomes.
- C GC-stress corpus rerun: passed. Production-library clippy passed with
  existing warnings; targeted rustfmt checks passed.

Evidence logs: `/build/tmp/crush-xps13-core-rebased.log`,
`/build/tmp/crush-xps13-sdk-rebased.log`, `/build/tmp/crush-xps13-aot-rebased.log`.
The initial limited SDK feature selection omitted optional polyglot support
needed by unrelated library tests; the default-feature suite passed. The first
oracle sweep exhausted transient build space; its retry passed and per-fixture
native binary cleanup now bounds that usage. No unrelated source was changed
for those environment/feature issues.

### Integration refresh

Folded all slices into one commit and rebased onto main `aa07413` (#270).
Preserved #268 deterministic codegen, cached compiler identities, environment
cache/optimization controls, cache-hit mtime refresh and parallel/sharded tests.
Extended that driver with support-source/Rust-toolchain/CC-target identities and
paired private support archives; their cache hits also refresh mtimes.

Final additions protect numeric method programs from unproven arithmetic and
wide ordering precision loss; async functions require Future/poll semantics.
Public Module loader probes verify actual scalar/string result kinds (including
numeric-looking text and non-finite floats), alongside NUL host result refusal.
The private ABI has direct invalid-input, allocation/free and forced-panic
containment tests. No public ABI signature or C Value layout was expanded.

Current rebased support matrix (112 native oracle fixtures): VM 63 exact / 48
loud / 1 existing xps-8 silent; Rust AOT 58 exact / 53 loud / 1 silent; C AOT
52 exact / 59 loud / 1 silent. No new silent policy was added. The older 82-fixture
xps-9 snapshot preceded the additional numeric fixtures merged with #246.
