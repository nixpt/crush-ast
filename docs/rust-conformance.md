# Rust source conformance

The SDK's `compile_source("rust", source)` compiles a Rust **subset** to Crush
bytecode. It does not invoke Cargo, resolve arbitrary Rust crates, or provide
Rust's type checker and borrow checker. Rust crate APIs enter through explicit
host-approved capability/plugin wrappers. Hosts choose their own grants.

## Gate (CRUSH-xps-5)

`crates/crush-lang-sdk/tests/rustc_corpus/` contains runnable Rust programs and
their stdout. The SDK test recompiles every fixture with real `rustc
--edition=2021 -O`, checks a successful exit and empty stderr, and compares
stdout byte-for-byte with its `.out` file. A missing compiler fails the test.
This oracle uses release overflow behavior; debug overflow traps are not
claimed as supported.

The same files run through:

- the SDK's public source API and CVM1 runtime, with the host's standard library
  explicitly registered;
- CAST → CASM → Rust AOT;
- CAST → CASM → C AOT.

The policy in `tests/rust_support/mod.rs` is shared by both test targets:

| Classification | Required result |
|---|---|
| Exact (the default) | Identical stdout, including whitespace |
| Known unsupported | Rejection containing the recorded diagnostic and fixing ticket |
| Known silent divergence | The precise recorded wrong stdout and a fixing ticket |

A new failure, different wrong output, or wrong output from a known unsupported
fixture fails the gate. When an exception becomes exact, the test fails with
“now exact — remove it” so the inventory cannot silently go stale. Subprocess
timeouts and launch failures fail the test; they do not count as unsupported
language features. stdin is closed for all subprocesses.

**A green inventory audit does not mean all Rust programs are correct.** The
silent entries are blockers, not supported behavior. CRUSH-xps-4's `.rs` CLI
dispatch remains gated on xps-5 and closing xps-6–12.

## Current boundaries

The exact baseline covers scalar arithmetic, function calls and returns,
conditionals, loops and ranges, literal matches, vector indexing/mutation,
mapped standard methods, and supported scalar closure captures and block bodies.
See `crush-lang-rust`'s crate docs for the syntax subset. Syntax acceptance alone
does not establish Rust semantics.
The supported output macro is `println!`. Newline-free `print!` and stderr
`eprint!`/`eprintln!` are refused at the Rust source boundary until faithful
routing is available.

| Family | Inventory / owner |
|---|---|
| Local/parameter shadowing in blocks, branches, loops, matches, closures | Exact supported cases; distinct binding identities (CRUSH-xps-6) |
| Nested functions | Rejected with location until block-local item resolution exists (CRUSH-xps-6) |
| Flat vector/array clones, fresh arguments and reassignment | Exact on VM and both AOT backends (CRUSH-xps-7) |
| String clone, `to_owned`, and `+=` on local bindings | Exact supported cases (CRUSH-xps-7) |
| Implicit owned transfers, nested clones, borrowed views, `pop`/`unwrap`, indexed writes, owned captures | Rejected with location (CRUSH-xps-7) |
| Other mutable string operations and struct value semantics | Rejected; CRUSH-xps-7 and COSM K3 |
| Literal integer casts in i8/i16/i32/i64/u8/u16/u32 | Exact; other casts rejected with location (CRUSH-xps-8 slice 1) |
| Typed arithmetic and release overflow | Known silent divergence; CRUSH-xps-8 remains open |
| Proven typed `{:?}` / `{name:?}` | Shared kernel on SDK VM/Rust AOT; C refuses dynamic calls pending its bridge (CRUSH-xps-9) |
| Walker assignments / loops | Exact baseline; CRUSH-xps-10 |
| Supported closures | Exact on VM; AOT rejects closure opcodes (CRUSH-251) |
| Resolved `str.to_lower`, `str.contains`, `math.pow` capabilities | Exact on VM; both AOT backends reject them (CRUSH-xps-13) |
| Move/destructured closures | Rejected; CRUSH-xps-11 |
| Multi-statement blocks in a value position | Rejected with location; CRUSH-xps-12 |
| Rust imports / crate resolution | Rejected; host wrappers are the supported crate integration path |
| Format precision and range patterns | Rejected; no lowering implemented |

Rust `Copy`/move/clone semantics must follow COSM's contracts when Rust type
lowering arrives. The schema and Crush frontend alone do not enable Rust
structs, traits, enums, or `impl` blocks. Engine support and receiver dispatch
are tracked in CRUSH-zorro-42–44; Rust's producer belongs to COSM K3.

## Rust formatting boundary (xps-9)

The pure stdcap `rust.debug(value, descriptor) -> String` validates bounded JSON
type trees, checks runtime type/value agreement, and formats with Rust's Debug
implementations. It has no generic text fallback. For example,
`{"kind":"char"}` distinguishes a Rust char from a one-character string;
`{"kind":"seq","item":{"kind":"str"}}` describes a homogeneous sequence.
Only `kind` and `item` keys are accepted (either order), with canonical unescaped
ASCII kind names and JSON whitespace. Unknown/duplicate keys, unsupported
widths and malformed/trailing data are errors.

| Execution | Dynamic Debug |
|---|---|
| SDK / CVM1 | `HostCapsBuilder::stdlib(true)` explicitly registers the pure stdcap |
| Rust AOT | Embeds the VM's std-only kernel verbatim; only value access differs |
| C AOT | Refuses compilation before artifact generation until a shared bridge exists |

Source descriptors are inferred after lexical binding resolution and before
ownership checking. Supported proofs include immutable bindings, compatible
direct-call arguments and acyclic return summaries, char versus str/String,
unit, booleans, representable integers, proven f64 producers and homogeneous
sequences. Character values retain their scalar Copy identity. Unknown aliases,
f32, unproven numeric producers, written bindings, mutable collection flows,
closures, iterators and unresolved control flow are refused before execution.
Loops/matches and wide integer comparisons need richer flow/numeric proofs;
`.powi()` is deferred to exact numeric method dispatch. Builtin/helper named
source calls and effectful multiargument source calls are conservatively refused
until the integrated xps-8 call contract is consumed by this profile.

The kernel borrows sequence contents, checks cycles before descending and limits
runtime traversal to depth 16 / 1024 visited values. Descriptors are at most
4096 bytes; output is streamed into a 1 MiB bounded buffer. Direct stdcap callers
receive explicit descriptor/type/quota errors; permission or registration
failures remain distinct. Compile-time literal Debug folding retains its depth
16 / 256 node and 1 MiB bounds. Debug text is tied to the recorded rustc profile,
not promised as a stable cross-version serialization format.

Display retains the existing `conv.text(value, "rust")` ABI. Repeated/reordered
inert arguments remain supported; potentially effectful macro arguments need
one use each in source order and no inline captures. Other uses are refused
pending explicit materialization. Empty and literal-only `format!` results and
runtime formatter results retain their owned String identity. The Debug-string
silent exception is removed; the C-only dynamic policies explicitly pin its
bridge boundary. Integer overflow remains the sole silent exception.

## Rust integer conversion boundary

The first xps-8 slice supports casts whose operand is an integer literal, a
direct negative integer literal, parentheses around those, or another supported
cast. Narrowing keeps the low target-width bits and interprets the target sign;
widening preserves the source value. Integer cast targets constrain unsuffixed
literal operands; `255 as i8` is rejected as an overflowing literal. Examples:
`300i64 as u8` is `44`, `255u8 as i8 as i64` is
`-1`, and `3_000_000_000i64 as i32` is `-1294967296`.

Casts of variables, calls and computed operands are refused before compilation
until numeric binding/range analysis and single-evaluation lowering exist.
Float/char/pointer casts, wide unsigned/128-bit targets, and aliases are also
refused. The original cast audit is now a frontend-refusal policy on all three
engines. Signed minimum literals parse correctly; explicit integer suffixes
have range checks, and unsupported integer/f32 suffixes are rejected.

Local/function/closure annotations and literal suffixes survive as **source
hints** in CAST metadata beside lexical/ownership metadata. They do not yet
establish inferred types or typed arithmetic, and do not resolve annotation-only
f32/wide/pointer types. The overflow audit remains a known silent divergence;
xps-8 and normal `.rs` CLI dispatch remain gated.

## Rust binding identity

After lowering macros and statement blocks, the Rust frontend resolves locals
to distinct CAST storage names. Initializers see the previous binding; each
branch, loop body, match arm, and closure restores its enclosing environment on
exit. Source callable variables are resolved without renaming compiler helpers
or global capability namespaces.

Declarations and variable references retain `meta.rust_binding` with
`source_name`, `function`, `id`, `scope_id`, and `kind`. The function's
`meta.rust_bindings` catalogs all declarations, and closures keep
`meta.rust_scope_id`/`meta.rust_params`. Identity is `(function, id)`; these IDs
describe lexical declarations, not VM slots or dynamic executions. Closure
capture can therefore distinguish two source bindings named `x` without
changing the VM storage or cell-lifetime rules. Rust ownership checks run after
binding resolution, so the value pass can distinguish captures and shadowed
bindings. This bounded check does not replace Rust type/borrow checking.

## Rust value boundary (CRUSH-xps-7)

The frontend preserves a small type hint domain in CAST metadata and checks
value uses after resolving lexical bindings. This is deliberately conservative:
valid Rust moves can be rejected. Compile valid Rust with `rustc` first; Crush
does not establish that a program satisfies Rust's type or borrow rules.

- `.clone()`, `.to_owned()`, and `.to_vec()` create fresh flat sequences through
  the existing `seq.slice` capability on all engines. Elements must be known
  scalar values, immutable text, or owned strings. Nested, reference, and unknown
  element clones are rejected. Empty sequences need an explicit element type
  before cloning.
- Runtime strings are immutable values. String clones and `to_owned()` preserve
  their value; `+=` replaces a local string binding. `s.clone() + "!"` is supported.
  Other mutable String APIs, such as `push_str`, remain unsupported.
- Fresh literals, explicit supported clones, and fresh function results can
  initialize/reassign owned bindings and enter known compatible function
  parameters. Bare Vec/String/array binding transfers to another binding,
  argument, return, or owning iterator are rejected pending move/copy analysis.
  This includes implicit copies of Rust `Copy` arrays. Indexed writes are also
  rejected. Struct syntax still needs the COSM Rust producer.
- Direct sequence iteration and immediate view-to-owned copies are supported.
  Storing, passing, assigning, or returning borrowed views needs borrow analysis
  and is rejected. Explicit references and reference parameters other than
  `&str` are rejected. Owned/unknown closure captures are rejected; supported
  scalar closures retain their existing shared-cell behavior.
- `Vec.push` is supported as a statement on a known Vec with a fresh flat value.
  `pop`/`unwrap` need `Option` semantics and are rejected. Unproven `into`,
  `borrow`, `copied`, and `cloned` adapters are also rejected, rather than erased.

The corpus contains separate exact and loud cases for clones, assignment,
arguments, owned returns, captures, nested vectors, arrays, and borrowed views.
The original vector-clone aliasing and Debug-string exceptions are removed.
Integer overflow remains a known silent divergence under xps-8. Dynamic Debug
uses xps-9's proven descriptor profile; C still needs its shared bridge.

## Completion specifications

The [xps-8–13 proposal](../.jagent/planning/designs/rust-parity-xps-8-13.md)
defines the next numeric/Debug/mapped-capability slices and the review closure
matrices for xps-10–12. It specifies future supported/refused domains; it does
not expand the current implementation's accepted subset. Numeric method result
types and effectful format arguments require explicit proof as well as stdout.

## Run the gate

```sh
cargo test -p crush-lang-sdk --test rustc_corpus
cargo test -p crush-aot --test rust_parity
```

The second command requires `rustc` and `gcc`. Both test targets run in existing
CI jobs (`Test (sdk)` and `Test (aot)`). Adding a fixture makes it exact by
default; adding an exception requires a specific fixing ticket and evidence.
