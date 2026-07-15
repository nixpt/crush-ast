# CRUSH-5 — AOT Rust-codegen backend can't compile any program (missing enum variant)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-5 |
| **Priority** | P0 |
| **Status** | Backlog |
| **Phase** | M1 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | S |

## Problem

`crush_aot::AotCompiler::compile_casm` (the Rust-codegen AOT backend —
`crates/crush-aot/src/codegen.rs`'s `gen_rust_source`) generates Rust
source that references `RuntimeValue::Str(...)` (both as a constructor and
in a `match` arm), but the `RuntimeValue` enum it *also* generates does not
have a `Str` variant. The generated crate fails to compile with `rustc`
every single time, **unconditionally** — confirmed with a pure-numeric
program (`function add(a,b){return a+b;} function square(x){return x*x;}
console.log(square(add(3,4)));`, zero string usage anywhere) failing
identically to a string-heavy program. The string-handling helper
functions (concat, `as_text`) are apparently always emitted regardless of
whether the source program uses strings, and always reference the missing
variant.

## Impact

`crush-aotc compile`/`crush-aotc run` (default backend, no `--backend`
flag) and `AotCompiler::compile_casm` cannot produce a working `.so` for
**any** program today. Confirmed independently of `CRUSH-2`/`CRUSH-3`/
`CRUSH-4` — this is a pure Rust-codegen bug, unrelated to array mutation,
stale examples, or the JS-walker type-inference issues. `crush-aotc
compile --backend gcc` (the alternate C-codegen path) does not have this
specific bug — see `CRUSH-6` for what *does* break there.

## Reproduction

```bash
cat > /tmp/numeric.js <<'JS'
function add(a, b) { return a + b; }
function square(x) { return x * x; }
console.log(square(add(3, 4)));
JS
js_walker /tmp/numeric.js > /tmp/numeric.cast.json
# Then: compile_cast -> casm::Program -> AotCompiler::new().compile_casm(&program, "m")
```

```
error[E0599]: no variant, associated function, or constant named `Str` found
  for enum `RuntimeValue` in the current scope
   --> .../lib.rs:105:34
105 |         stack.push(RuntimeValue::Str(format!("{}{}", as_text(&a), as_text(&b))));
```

(two occurrences: the string-concat helper's constructor call, and a
`match` arm converting `RuntimeValue::Str(s) => s.clone()`)

## Technical approach

- `crates/crush-aot/src/codegen.rs`: find the `RuntimeValue` enum
  definition (`gen_rust_source`, near wherever `LocalType`/similar is
  defined per the sibling `codegen_c.rs`'s `enum LocalType { Value, F64,
  I64 }` pattern) and either (a) add the missing `Str(String)` variant, or
  (b) if strings are meant to be represented some other way in this
  backend (e.g. boxed/interned), fix the concat-helper and match-arm
  codegen to match whatever the enum actually has.
- Add a regression test that actually invokes `rustc` on generated output
  for both a numeric-only and a string-using program — this bug is exactly
  the kind of thing "compiles the Rust source" verification would have
  caught immediately, and evidently nothing in CI currently does that
  (`crush-aotc`/`AotCompiler::compile_casm` appear untested end-to-end).

## Files to modify

- `crates/crush-aot/src/codegen.rs`

## Non-goals

- Fixing the C-codegen backend's string-output bug (`CRUSH-6`, separate)
