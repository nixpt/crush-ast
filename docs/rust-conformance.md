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
| Proven integer casts, lengths and bounded unsigned values | Exact within the numeric proof profile; unproven cases refused (CRUSH-xps-8) |
| Typed arithmetic and release overflow | Known silent divergence; CRUSH-xps-8 remains open |
| Typed literal `{:?}` quoting | Exact; dynamic Debug refused pending the shared formatter (CRUSH-xps-9) |
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

## Rust formatting boundary (xps-9 slice 1)

Typed literal Debug is evaluated from Rust syntax, retaining char versus string,
unit, numeric width and f64 precision until formatting. Literal arrays are
bounded to depth 16 / 256 traversal nodes; each result is limited to 1 MiB.
Dynamic Debug values, including inline-name captures, are refused pending the
shared typed runtime formatter. `audit_debug_string` is exact on all engines.
The existing dynamic `float_display` oracle remains pinned to an explicit
xps-9 refusal; its expected native output is retained for the next slice.

Display keeps `conv.text`'s existing two-argument ABI. Repeated/reordered inert
literal or variable arguments remain supported. Potentially effectful arguments
require one use each in source order and no inline captures; otherwise lowering
refuses them before execution. Named argument materialization is deferred.
Empty and literal-only `format!` values retain owned String identity.

No new runtime formatter capability or backend-specific formatter is added in
this slice. The shared kernel, descriptors, registration and bridge remain open.

## Rust integer conversion boundary

Integer casts support literals, proven immutable bindings, direct function results,
and computed operands with canonical integer range proofs. Narrowing keeps the
low target-width bits and interprets the target sign; widening preserves the
source value. The shared CASM expansion evaluates each operand once. Integer
cast targets constrain unsuffixed literals: `255 as i8` is an overflowing
literal, while `255u8 as i8 as i64` produces `-1`.

A numeric pass runs after lexical binding resolution and before ownership
checking. It records parameter, binding and result facts, validates direct-call
arguments, and invalidates written bindings, including captured writes. Rust
free-function calls evaluate arguments from left to right and resolve to actual
source functions even when their names overlap compiler helpers or builtins.

Small `u64` values are supported only in `0..=i64::MAX`. New integer-to-`usize`
conversions need a proven bound in `0..=65535`, portable across Rust pointer
widths; this ceiling does not select a host pointer width. Existing canonical
`usize` provenance from a proven sequence or non-NUL string `len()` can retain
its guest-representable range. Length conversions to fixed-width integers and
nonnegative bounded range indices are covered by the cross-engine corpus.

Float/char/pointer casts, 128-bit integers, unknown aliases, unproven producers,
recursive or indirect calls, and casts inside closures remain refused. Mutable
collection lengths and NUL-containing text lengths are refused in this numeric
profile. This is a closed-program proof profile, not a Rust type checker.
General typed arithmetic/wrapping remains unfinished: the overflow audit stays
a known silent divergence. CRUSH-xps-8 and normal `.rs` CLI dispatch remain gated.

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
Integer overflow remains a known silent divergence under xps-8; dynamic Debug
is refused under xps-9.

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
