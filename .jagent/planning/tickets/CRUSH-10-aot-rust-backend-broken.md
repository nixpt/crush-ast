# CRUSH-10 — AOT Rust-codegen backend can't compile any program (missing enum variant)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-10 |
| **Priority** | P0 |
| **Status** | **Done** — verified s388 (2026-07-16) |
| **Phase** | M1 |
| **Assignee** | fixed via `agent/kai/CRUSHAST-RELEASE-1`-era `RuntimeValue::Str`→`RuntimeValue::String` fix, `crush-ast` `5f30520`/`c27601e` |
| **Dependencies** | none |
| **Estimated effort** | S |

## Resolution (verified s388)

Confirmed fixed independent of this ticket, by a different session's AOT
verification pass (the `RuntimeValue::Str` vs the enum's real `String`
variant fix, `crush-ast` `5f30520`/`c27601e`). Re-verified end-to-end here:
`crush-aotc compile --emit so` on a pure-numeric program compiled AND
executed correctly via `ctypes.CDLL` (`print(5+3)` → `8`, correct). The
crate's own `cargo test -p crush-aot --test integration` (this ticket's
"22/22 failing" evidence) is now **all green** as part of the same-day full
workspace test run. `grep -rn RuntimeValue::Str` across `crush-aot` returns
zero hits — only the correct `RuntimeValue::String` remains.

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
**any** program today. Confirmed independently of `CRUSH-7`/`CRUSH-8`/
`CRUSH-9` — this is a pure Rust-codegen bug, unrelated to array mutation,
stale examples, or the JS-walker type-inference issues. `crush-aotc
compile --backend gcc` (the alternate C-codegen path) does not have this
specific bug — see `CRUSH-11` for what *does* break there.

**This is not a rare edge case — it's the project's own shipped examples.**
`crates/crush-aot/examples/{simple,arithmetic,sum1k}.crush` (three
existing, presumably-once-working canonical AOT example files, all pure
numeric, no strings) **all fail identically** via `crush-aotc run
<file>.crush` (default `rustc` backend) — same `RuntimeValue::Str` error,
100% of the time:

```
$ crush-aotc run crates/crush-aot/examples/simple.crush
error[E0599]: no variant, associated function, or constant named `Str` found for enum `RuntimeValue`
$ crush-aotc run crates/crush-aot/examples/arithmetic.crush
error[E0599]: ...
$ crush-aotc run crates/crush-aot/examples/sum1k.crush
error[E0599]: ...
```

The **same three files, run with `--backend gcc` instead, all produce
correct output**: `simple.crush` → `42`, `arithmetic.crush` → `44`,
`sum1k.crush` → `499500`. So the C-codegen backend is the one that's
actually alive and correct for numeric programs (see `CRUSH-11` for its
separate string-output bug) — `docs/design/aotc-math-optimizations.md`
("Pathway 1... Active") is entirely about the C-codegen path too, which
lines up: the Rust backend looks like dead/unmaintained code, not a
recent regression in an actively-used path. **Suggest making `gcc`/`clang`
the default backend** (flip `RunArgs`'s `--backend` default in
`crates/crush-aot/src/bin/aotc.rs`) as an immediate, near-zero-risk
stopgap while `CRUSH-10`/`CRUSH-11` get properly fixed — right now the
*documented default* is the one that never works.

**Decisive confirmation: the crate's own committed test suite is 100%
red.** `cargo test -p crush-aot --test integration` (the Rust-backend
integration tests — `test_aot_int_return`, `test_aot_arithmetic_add`,
`test_aot_bool_true`, the most basic sanity checks possible) — **22 of 22
tests fail, 0 pass**, all the identical `RuntimeValue::Str` compile error.
This isn't a scenario I constructed; it's the project's own dedicated test
file for exactly this code, and it's been failing outright. By contrast
`cargo test -p crush-aot --test integration_c` (the C-backend suite) is
**16 of 19 passing** — the only 3 failures
(`test_cross_all_three_vs_fastvm`, `test_cross_c_clang_vs_rust`,
`test_cross_c_gcc_vs_rust`) are cross-comparison tests that fail only
because they *also* invoke the broken Rust backend for comparison, not
because the C backend itself is wrong. Confirmed this isn't a stale-
worktree artifact on the reporter's end either: worktree base is
`origin/main`'s exact current tip, `git merge-base --is-ancestor` confirms
all of `df97771`/`462ae99`/`62964aa`/`63b5b40` (the recent AOT-touching
commits) are included.

## Reproduction

Simplest: run any of the project's own existing examples.

```bash
cargo run -p crush-aot --bin crush-aotc -- run crates/crush-aot/examples/simple.crush
# error[E0599] ... RuntimeValue::Str ...

cargo run -p crush-aot --bin crush-aotc -- run crates/crush-aot/examples/simple.crush --backend gcc
# 42  (correct — confirms this is Rust-backend-specific)
```

Also confirmed via a fresh JS-walked program (zero string usage, to rule
out anything walker-specific):

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

- Fixing the C-codegen backend's string-output bug (`CRUSH-11`, separate)
