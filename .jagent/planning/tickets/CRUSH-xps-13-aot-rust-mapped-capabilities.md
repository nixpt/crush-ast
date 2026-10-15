# CRUSH-xps-13 — Share Rust mapped capability semantics across VM and AOT

| Field | Value |
|-------|-------|
| **ID** | CRUSH-xps-13 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | M2 / Rust AOT parity |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-xps-3, CRUSH-xps-5, CRUSH-xps-7; CRUSH-xps-8 for numeric method dispatch |
| **Coordination** | CRUSH-xps-9 may use the same private C support bridge |
| **Estimated effort** | L; inventory, string kernels/bridge, typed numeric dispatch |
| **Spec** | [Rust parity completion](../designs/rust-parity-xps-8-13.md#xps-13-named-capabilities-and-typed-method-dispatch) |

## Problem and evidence

Resolved named capabilities keep Rust local bindings named `str`, `math` or
`array` from changing dispatch. Both AOT backends currently refuse `str.to_lower`,
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

- [ ] Every mapped method has a reviewed semantic/backend support row.
- [ ] Named capabilities use a shared kernel and preserve the canonical
      argument, result and error semantics for the supported domain.
- [ ] Unicode/string representation and numeric result-type probes pass;
      no method is accepted solely because an ASCII/stdout-only example agrees.
- [ ] Unsupported source domains fail at the Rust boundary, and unsupported
      AOT domains fail before generated code loads. Infrastructure failures,
      crashes and permission failures are not allowlisted language refusals.
- [ ] The three original AOT exceptions are removed only after all engines
      match rustc; new domain restrictions remain explicit and ticket-linked.
- [ ] Private ABI error/allocator/unwind behavior and cache invalidation are
      covered; C cases pass normal GC and `CRUSH_GC_STRESS=1`.
- [ ] Live SDK/Rust AOT/C AOT output and updated conformance/release docs are
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
