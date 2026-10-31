# Rust SDK parity completion: CRUSH-xps-8 through CRUSH-xps-13

**Status:** Proposed for review.
**Requested scope:** Expand the six existing tickets; retain their IDs and work history.
**Snapshot:** 2026-10-15. Main `30288bc`; SDK/value stack through PR #233,
head `1be8c51`. This document is based on the SDK stack, where its contracts
and xps-13 ticket already exist. Runtime changes on main still require an
integration pass before the next implementation starts.

## Delivery order and ownership

| Ticket | State at this snapshot | Required delivery |
|---|---|---|
| [xps-8](../tickets/CRUSH-xps-8-rust-integer-widths.md) | Backlog | Preserve numeric facts; exact fixed-width conversions/arithmetic or source-located refusal |
| [xps-9](../tickets/CRUSH-xps-9-rust-debug-format.md) | Backlog | Typed Rust Debug formatting and correct format argument evaluation, or refusal |
| [xps-10](../tickets/CRUSH-xps-10-walker-intrinsics-and-for-loop-semantics.md) | In review in #213 | Close the shared helper/loop contract with the review matrix below |
| [xps-11](../tickets/CRUSH-xps-11-rust-closures-exact-or-loud.md) | In review in #213 | Close the documented scalar closure subset and its rejection boundary |
| [xps-12](../tickets/CRUSH-xps-12-rust-expression-block-lowering.md) | In review in #213 | Close complete function/closure bodies and safe rejection of other value blocks |
| [xps-13](../tickets/CRUSH-xps-13-aot-rust-mapped-capabilities.md) | Backlog | Shared named-capability kernels and typed method dispatch on VM/Rust AOT/C AOT |

Merge/review the existing #213 → #220 → #224 → #233 stack in dependency order.
Start new implementation with xps-8, then xps-9, then xps-13. The numeric
dispatch part of xps-13 depends on xps-8; the string-only part can be developed
independently after xps-5. xps-10–12 are review closure work, not six new fixes
to assign to another implementation team. Keep their completed acceptance
items and evidence; add missing proof or focused corrections in their existing
review lane.

After the Rust source gates are closed, xps-4 can enable the `.rs` CLI path.
xps-8 must also reject or correct integer method mappings whose actual result
type is incompatible with Rust; it cannot merely label a float result as an
integer. Full AOT capability coverage in xps-13 is a separate delivery gate: documented
compile-time AOT refusals do not block a correct VM CLI. AOT closures remain
owned by CRUSH-251. Coordinate CLI changes with CRUSH-zorro-27 / PR #201.

## Shared rules

1. A new fixture defaults to exact. The SDK VM must produce rustc's output or
   the Rust frontend must explicitly reject an unsupported construct. AOT can
   produce the same output or refuse the unsupported operation before loading
   generated code. A panic, crash, timeout, compiler/linker failure, missing
   host grant, or missing tool is not an unsupported-language success.
2. Remove each silent exception only after its replacements have passed on the
   SDK VM and the two AOT engines. Existing silent entries are temporary debt,
   not acceptable final delivery. Keep ticket-specific diagnostic matching.
3. Use the existing corpus and shared policy; do not create another parity
   runner. Preserve stdout bytes, input closure, quotas, process exit status,
   and subprocess deadlines. Give each implementation worktree its own Cargo
   target and limit build parallelism on this box. Record `rustc -Vv`, edition, target and flags in
   proof so compiler-dependent formatting changes are visible.
4. Preserve xps-6 binding identity and xps-7 ownership checks. `Kind::Copy` is
   an ownership category, not a numeric type or evidence of Rust Debug behavior.
5. Pure stdcaps remain opt-in through the host's standard-library registration.
   New formatting/numeric kernels do not grant I/O, native plugins, or Cargo.
   Generated compiler helpers must have explicit semantic/optimizer/codegen
   contracts and be consumed before execution.
6. Keep one semantic kernel per operation. Generated Rust can embed the same
   dependency-light Rust source used by the VM, as the existing JS core does.
   C must use shared generated logic or a private bridge to that kernel; a
   separately maintained approximation is not sufficient proof.

## xps-8: numeric facts and the bounded integer profile

### Source contract

The initial oracle profile is Rust 2021 with `rustc -O`; overflow checking is
off for the runtime wrapping fixtures. The implementation must record that
profile explicitly and use runtime parameters so const-overflow diagnostics
are not accidentally treated as executable wrapping behavior. Debug panic
behavior is outside this profile. Invalid Rust stays outside the runnable
corpus and belongs in separate frontend rejection cases.

Preserve a numeric fact alongside lexical binding identity: integer width and
signedness, float precision, Boolean, character, quota-bounded count/index, or
unknown. Gather facts from literal suffixes, annotations, casts, parameters,
returns, closures, and expression joins. Unsuffixed literals follow the
supported Rust inference constraints, with `i32` fallback where Rust applies
it; they are not silently treated as `i64`. Unknown/incompatible facts need
refusal when an operation depends on their width. Facts must survive optimizer
rewrites and assignment/return boundaries.

| Family | Initial delivery contract |
|---|---|
| `i8/i16/i32`, `u8/u16/u32`, `i64` values | Canonical mathematical values stored in existing signed `Int`; preserve source type separately |
| Integer `as` conversions within that domain | Truncation/sign interpretation/extension exactly match Rust; identity lowering requires a proof |
| Narrow `+/-/*`, unary and compound operations | Normalize at each typed operation, only after proving the intermediate cannot overflow the shared i64 operation |
| Ordinary i64 arithmetic | Exact where range facts prove its checked VM operation agrees with the selected Rust profile; otherwise refuse or use a reviewed shared wrapping kernel |
| `wrapping_*` | Exact explicit width-aware implementation, or frontend refusal; never erase the method |
| `usize/isize` | No implicit host-pointer-width assumption; bounded length/index values can be carried when independent of width, but width-sensitive operations require a declared matching source target or refusal |
| `u64/i128/u128`, unknown numeric aliases/generics | Refuse unless a separately reviewed representability proof covers the entire operation; do not encode large unsigned values as negative guest integers |
| float/character/pointer casts, f32 arithmetic | Refuse until exact conversion/precision semantics are implemented; these casts must no longer disappear |

For an integer target of width `w < 64`, use the low `w` bits, then subtract
`2^w` when a signed target's sign bit is set. Widen an already canonical source
with its correct signedness. This can be lowered to existing bit/comparison
operations when their backend behavior is proven. It does **not** justify
normalizing after an overflowing intermediate: `u32 * u32`, unrestricted
i64 addition/multiplication, `MIN / -1`, and large shifts need dedicated proof
or refusal. The sign on a minimum signed literal must be handled before an
out-of-range positive parse loses it.

Evaluate a converted operand once. A proposed compiler-owned
`__crush_rust_int_cast__(value, width, signed)` contract can validate literal
width/signedness and use a temporary/stack value during normalization; it must
not duplicate a source call inside a sign test and its result branch. Add the
contract to shared helper validation/optimization/codegen before emitting it.
No unconsumed numeric marker may reach runtime.

The intended pass order is source lowering with retained facts → lexical
binding resolution → numeric facts/cast lowering → typed format lowering →
xps-7 value checks → shared semantics/optimization/codegen. Ownership and numeric
facts remain distinct. New formatter return classes must be taught to the value
pass; a capability returning String must not default to `Kind::Copy`.

Delivery slices: (8a) preserve facts and close erased-cast/width holes with
diagnostics; (8b) exact supported fixed-width casts; (8c) exact narrow arithmetic
and supported wrapping methods, with conservative refusals for the rest.
All slices belong to xps-8; no new IDs are needed. Do not change the generic
Crush arithmetic policy to make the Rust fixtures pass. In particular, the
separate JS Int-to-Float promotion plan does not define Rust overflow.

## xps-9: typed Debug and macro evaluation

Retain Rust formatting intent and a type descriptor before values become
generic CAST strings/numbers. String, char and unit are distinct even though
their runtime representations can overlap. Initial supported Debug types:
Bool, supported integers, f64, str/String, char, unit, and bounded acyclic
sequences of supported elements. Reject unknown types, f32 where precision is
lost, maps/structs/enums/traits, alternate/width/precision formats, and graphs
whose bounded traversal cannot be guaranteed.

Proposed wire contract: a new pure stdcap `rust.debug(value, descriptor)`
returns a String. The descriptor is a compiler-produced literal JSON type tree
(for example `{"kind":"seq","item":{"kind":"char"}}`), validated by the
shared formatter. Invalid descriptors and value/type mismatches are errors;
there is no generic `as_text` fallback. Existing `conv.text(value, "rust")`
continues to implement the Display subset. Adding this capability is part of
xps-9, not an ambient runtime registration or a change to `conv.text` arity.

Use the same Rust formatter kernel in VM and Rust AOT. C uses a shared bridge
when it can faithfully represent inputs/outputs; otherwise reject at AOT
compile time and retain an xps-9 exception. String quoting, escapes, Unicode,
char quoting, unit `()`, sequence delimiters and f64 Debug text must be checked
against the compiler used by the oracle. Debug output is not promised stable
across Rust releases; compiler upgrades require an explicit freshness review.

Macro arguments are evaluated once, in source argument order, before their
values are formatted. Repeated or reordered placeholders must not duplicate
or reorder calls. If the current expression IR cannot materialize temporaries,
reject effectful repeated/reordered uses at the source location. Do not hide
statement blocks inside synthesized lambdas: that would change capture and
return semantics. Apply this check to shared Display/Debug macro parsing, and
preserve `format!`'s owned String identity even for a literal-only format.
Audit newline-free and stderr macros too: `print!`, `eprint!`, and `eprintln!`
need faithful output routing or source refusal. Their currently documented
output differences cannot be silently carried into normal `.rs` dispatch.
Use separate stdout/stderr oracle coverage where the current corpus requires
empty stderr; adding stderr fixtures to that corpus unchanged is invalid.

## xps-10-12: close the existing review contracts

| Ticket | Review proof that must accompany closure |
|---|---|
| xps-10 | Assignment's unit result; helper arity/lvalue checks; locals restored after loops; self-referential and loop-carried assignments; unknown calls stay errors; source names cannot accidentally invoke compiler helper behavior |
| xps-11 | Ordered typed/wildcard parameters, shadowing, scalar captures and shared mutable scalar captures, complete closure bodies, source-located rejection of unsupported ownership/capture forms, AOT closure refusal retained under CRUSH-251 |
| xps-12 | Full function/closure statement sequence and tail value, lazy branch selection, empty/single-tail generic value blocks, source-located refusal of multi-statement/semicolon-ending value blocks and expression-position returns |

These are exact-or-loud closure criteria. xps-12 does not require broadening
the generic value-block subset to close its original ticket. If new review
cases reveal a hole, correct that ticket's implementation or leave its
acceptance unchecked; implementation present in #213 is not the same as merged
and released. Do not claim COSM record runtime support makes Rust struct
syntax valid. Main's COSM K1.4 and Boa default-parser changes need shared-path
regression checks when this stack is integrated.

## xps-13: named capabilities and typed method dispatch

Audit every mapping in `lower_method`, not only the three current AOT
exceptions. Distinguish each existing capability's SDK contract from Rust's
source method contract. In particular, `math.pow` currently returns a generic
float; mapping `i64.pow` to it is not justified by stdout that happens to print
`8`. Require a use of the result such as `2i64.pow(3) / 3`, whose Rust result is
the integer `2`. Integer power and narrow-width overflow belong to xps-8's
numeric facts; xps-13 wires only a compatible operation. `powi` and `powf` must
also retain distinct source semantics or be refused.

String lower/upper conversion, search, prefix/suffix, trim and replace must
respect UTF-8 and the SDK kernel's behavior. C's bytewise/locale functions are
not substitutes for Unicode case conversion. Numeric abs/round/floor/ceil/
sqrt/power need matching result types and edge behavior as well as printed
text. Unsupported argument domains must be explicit before executing AOT.

Proposed C integration: compile a dependency-light Rust support kernel with
the same controlled AOT build step and link a private C bridge into the
generated module. Pass scalar tags or pointer+byte-length inputs, return a
status plus owned UTF-8 output, and free Rust allocations through their Rust
allocator entry point after copying into rooted C-managed storage. Prevent
unwinds across the ABI and validate argument arity/types. This is private AOT
plumbing; it does not reuse or change the public `crush-ffi` plugin ABI.
Include support-source revision, compiler/target/options in the AOT cache key.

The existing C string representation also needs a representability gate:
do not truncate embedded NULs or outputs exceeding its buffers. In the first
slice, reject inputs whose faithful representation cannot be established;
retain targeted AOT exceptions for those cases. General C string/Value ABI
changes need a separate review before expanding that domain. The shared
bridge must not make a rejected domain look supported. Apply the same shared
kernel/explicit-refusal rule if xps-9 uses this bridge earlier.

## Evidence and release gates

Each ticket contains its proposed fixture matrix and dispatch steps. Runnable
fixtures must be valid Rust; unsupported valid Rust cases get narrow pinned
diagnostics. Add assertions on return types/values or downstream operations
where formatted stdout would hide a type error. Separate permission denial,
Rust panics, and toolchain failures from unsupported language diagnostics.

Required implementation proof (to be run by the implementer, not this design PR):

```sh
cargo test --locked -p crush-lang-rust -p crush-lang-sdk --lib --tests
cargo test --locked -p crush-aot --test rust_parity
cargo test --locked -p crush-frontend --test walker_intrinsics --test compiler_tests
```

For xps-13 add focused stdcap/bridge tests and run the C fixtures under
`CRUSH_GC_STRESS=1`, as well as normal collection. Live-run a source program
through the public SDK and compare it with an actual rustc executable; for
newly supported AOT cases run both native artifacts. Update the ticket,
conformance guide, and behavioral release notes together. No manual version
bump, CLI enablement, Cargo-managed plugins or COSM Rust producer is included.

## Primary reference points

- [Rust operators, overflow and casts](https://doc.rust-lang.org/reference/expressions/operator-expr.html).
- [Rust numeric types](https://doc.rust-lang.org/reference/types/numeric.html).
- [Rust Debug and its stability boundary](https://doc.rust-lang.org/std/fmt/trait.Debug.html).
- [Rust operand evaluation order](https://doc.rust-lang.org/reference/expressions.html#evaluation-order-of-operands).
- [Formatting macro argument contract](https://doc.rust-lang.org/std/macro.format_args.html).
- [Rust block evaluation and values](https://doc.rust-lang.org/reference/expressions/block-expr.html).

These define Rust behavior. The bounded subset, refusal rules, delivery slices,
and private AOT bridge above are this proposal's engineering decisions.
