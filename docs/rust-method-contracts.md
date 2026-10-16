# Rust mapped method contracts — CRUSH-xps-13

Source integration base: `aa07413` (main through #270; includes #246/#247/#252). The shared
Debug kernel is retained; xps-13 method guards apply before formatter proofs.

The supported subset now uses one std-only kernel on VM, Rust AOT and native
Linux C AOT. Integer methods retain widths and the optimized rustc wrapping
profile; floating methods require proven f64 receivers and distinguish powi
from powf. Prefix/suffix methods use explicitly registered Rust Bool adapters.
This is not a general Rust type checker: written/unknown/closure numeric
producers, async functions, unproven numeric call arguments/arithmetic, f32 and unsupported integer result
representations remain source-located refusals.

## Dispatch inventory

Arity counts the receiver. `SDK` below means explicit host registration through
`HostCapsBuilder::stdlib(true)` (or the equivalent stdlib registration API).
Existing SDK coercions and result types are preserved. PortableVm dispatches
builtins before host handlers: `str.contains` and `str.replace` use the core
registry even when the SDK registers handlers with the same names. Missing
registration or host denial is a host error, never an allowlisted language limitation.

| Source method | Arity | Source input and result | Lowering / registration | Current VM / Rust AOT / C AOT contract | Owner |
|---|---:|---|---|---|---|
| `len` | 1 | Known sequence/iterator or str/String → usize | compiler builtin | Existing supported sequence/text path; integer representation bounded by xps-8 | xps-8, xps-7 |
| `push` | 2 | Known Vec + supported flat element → unit, discard only | `array.push`, core cap registry | Existing append path on all three; value-position calls refused | xps-7, xps-8 element types |
| `pop` | 1 | Vec → Option | refused before execution | All source-refused; no Option semantics | xps-7 |
| `contains` | 2 | proven str/String + string pattern → bool | `str.contains`, core cap registry | Shared Bool kernel on all three; char/other patterns refused | xps-13 |
| `starts_with` | 2 | str + supported Pattern → bool | `rust.str.starts_with`, SDK | Shared Bool kernel on all three; generic SDK `str.starts_with` keeps Int flags | xps-13 |
| `ends_with` | 2 | str + supported Pattern → bool | `rust.str.ends_with`, SDK | Shared Bool kernel on all three; generic SDK `str.ends_with` keeps Int flags | xps-13 |
| `to_lowercase` | 1 | proven str/String → String | `str.to_lower`, SDK | Shared Unicode kernel on all three; char iterator results unsupported | xps-13 |
| `to_uppercase` | 1 | proven str/String → String | `str.to_upper`, SDK | Shared Unicode kernel on all three, including expansion | xps-13 |
| `trim` | 1 | proven str/String → borrowed str | `str.trim`, SDK | Shared text kernel on all three; source borrowed-view restrictions retained | xps-13, xps-7 |
| `replace` | 3 | proven str/String receiver, string pattern/replacement → String | `str.replace`, core cap registry | Shared string-pattern kernel on all three; char/other patterns refused | xps-13 |
| `to_string` | 1 | known supported scalar/string → String | `conv.text(value, "rust")`, core cap registry | Existing shared text conversion path on all three; unknown/sequence receivers refused | xps-13, xps-7 |
| `abs` | 1 | signed integer → same width, or float → same float type | `rust.num`, SDK | Shared width-aware wrapping signed integer abs / f64 abs; generic math.abs stays Float | xps-13, xps-8 |
| `sqrt` | 1 | f32/f64 → same float type | `rust.num`, SDK | Shared proven f64 kernel on all three; f32 unsupported | xps-13 |
| `floor` | 1 | f32/f64 → same float type | `rust.num`, SDK | Shared proven f64 kernel on all three | xps-13 |
| `ceil` | 1 | f32/f64 → same float type | `rust.num`, SDK | Shared proven f64 kernel on all three, including signed zero | xps-13 |
| `round` | 1 | f32/f64 → same float type | `rust.num`, SDK | Shared proven f64 kernel, signed zero and ties away from zero | xps-13 |
| `pow` | 2 | integer receiver + u32 exponent → receiver type | `rust.num`, SDK | Shared width-aware wrapping power; bounded u64/usize need representable result proofs | xps-13, xps-8 |
| `powi` | 2 | float receiver + i32 exponent → receiver type | `rust.num`, SDK | Shared f64::powi with proven i32 exponent; no powf alias | xps-13 |
| `powf` | 2 | float receiver + same float exponent → receiver type | `rust.num`, SDK | Shared f64::powf with proven f64 exponent | xps-13 |
| `clone` | 1 | known scalar/string/flat sequence → same value kind | identity for copies, `seq.slice` for sequences | Existing xps-7 path on all three; nested/unknown/reference cases refused | xps-7 |
| `to_owned` | 1 | known str/string/flat sequence → owned value | ownership adapter / `seq.slice` | Existing xps-7 subset, all three | xps-7 |
| `to_vec` | 1 | known flat sequence/view → fresh Vec | `seq.slice`, core cap registry | Existing xps-7 subset, all three | xps-7 |
| `as_str` | 1 | known String → borrowed str view | identity with ownership restrictions | Existing xps-7 subset, all three | xps-7 |
| `as_slice` | 1 | known sequence → borrowed view | identity with ownership restrictions | Existing xps-7 subset, all three | xps-7 |
| `iter` | 1 | known sequence → borrowed iteration view | identity in supported iteration context | Existing xps-7 subset, all three; not a general iterator object | xps-7 |
| `into_iter` | 1 | supported fresh sequence → owned iteration view | ownership adapter | Existing xps-7 subset; binding transfers refused | xps-7 |
| `copied`, `cloned` | 1 | iterator adapters | refused | All source-refused; iterator chains not implemented | xps-7 |
| `into`, `borrow`, `unwrap` | 1 | trait/borrow/Option adapters | refused | All source-refused; no general trait or Option model | xps-7 |
| Any other method or unsupported arity | variable | no contract | refused | Source-located unsupported method error | owning follow-up ticket required |

## Canonical operations and validation

SDK `get_str` converts a Value through Display. It does **not** require a string.
The walker must prove string receiver/pattern/replacement kinds before entering
that API. Char literals carry source metadata and have Copy value semantics;
a char cannot masquerade as a str receiver or string replacement pattern.
Rust supports char patterns for some string methods, but this slice explicitly
refuses them until pattern-specific adapters exist.

Both core and SDK string operations use Rust's `str` methods: Unicode casing, Unicode trim,
and substring matching/replacement (including empty patterns). Predicates return
Int(0/1) in the SDK handlers; the core `str.contains` builtin returns Bool and
shadows its SDK handler. Core `str.replace` also shadows its SDK handler.
Math operations accept numbers or parseable strings through `get_float`
and return Float. Wrong arity is checked by host dispatch; adapters must preserve
canonical missing-argument/type errors rather than unchecked indexing.

All admitted methods call `crush-vm::rust_methods_core`. SDK adapters retain
canonical coercions/results, core contains retains Bool, and generated Rust
embeds the kernel source. C links a private Rust static support library; see
[the reviewed bridge contract](rust-aot-support.md). C accepts string operations
only with walker-proven string inputs or immediate string literal arguments.
Generic C string coercions remain compile-time refusals; no bytewise/locale
alias or composite-text approximation is introduced. NUL constants and
external/character input producers are refused before a bridge-using C artifact
is built. VM and Rust AOT support NUL strings exactly.

Numeric methods use lexical xps-8 facts, direct-call argument checks and
least-fixed-point return summaries. Annotation alone is insufficient. Numeric method programs refuse unproven
width-sensitive arithmetic and wide integer ordering outside exact f64 ranges;
constant proofs refine ranges while runtime calls still execute once. Signed
and <=32-bit unsigned methods wrap at their source width under the existing
`rustc -O` conformance profile. u64/usize outputs need bounded representability
proofs; unsupported wrap/target domains are refused. General Rust arithmetic
remains xps-8 work. The frontend leaves non-finite constant arithmetic as runtime
operations because JSON CAST/CASM cannot encode infinity/NaN literals; operator
semantics are unchanged.

## Completion evidence and remaining ownership

The original `capability_namespace_bindings`, `int_arith`, `vecmut` and
`numeric_cast_float_producer` exceptions are removed: all three engines match
rustc. New fixtures cover Unicode, empty patterns/predicates, large strings,
integer wrapping/type-sensitive division, signed zero, NaN/infinity, distinct
powi/powf, NUL representation refusal, and source proof failures. Native oracle
binaries are deleted after each check to bound the audit's disk usage.

The private bridge's invalid tags, UTF-8, lengths, arity and reserved fields,
repeated owned-buffer freeing, and cache identity are directly covered. C is
also exercised under GC stress. Host registration/allowlist failures remain
host errors. xps-9 retains its C dynamic Debug refusal until a sequence-aware
bridge adapter is implemented; xps-13's scalar/string bridge does not expand
the C Value ABI. The ticket records the exact validation commands and results.

Public AOT loader probes verify returned Int/Float/Bool/String kinds, including
numeric/Boolean-looking strings, whitespace and infinity/NaN. Rust exports use
the existing quoted-string transport; non-finite floats retain Float identity.
Potential NUL root strings require a length-aware host result ABI and are
refused before artifact generation. Internal NUL strings and unit-returning
Rust programs remain supported on VM/Rust AOT. Unexpected NUL export failures
are loud rather than silently returning empty text.
