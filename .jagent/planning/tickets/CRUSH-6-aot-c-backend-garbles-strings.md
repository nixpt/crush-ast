# CRUSH-6 — AOT C-codegen backend runs but garbles string values

| Field | Value |
|-------|-------|
| **ID** | CRUSH-6 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | M1 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | M |

## Problem

`crush_aot::AotCompiler::compile_c` (`--backend gcc`/`clang`,
`crates/crush-aot/src/codegen_c.rs`) compiles and *runs* successfully
(unlike the Rust backend — see `CRUSH-5`), and correctly handles
pure-numeric programs (`square(add(3,4))` → `49`, confirmed correct).
Programs that print strings, however, produce garbage output: instead of
the actual string content, the printed value is something like
`1.73347e-308` — a value that looks exactly like a valid pointer/tagged
value reinterpreted through the wrong `printf` format specifier (`%g`/`%f`
instead of `%s`, or a union member read with the wrong tag). The program
does not crash and does reach completion (the final numeric return value,
e.g. a game's final score, comes back correct via `Module::call_main()`)
— only the *string* output during execution is corrupted.

## Reproduction

```bash
# examples/js-walked/turtle_runner.js, walked + compiled via the C backend:
js_walker examples/js-walked/turtle_runner.js > /tmp/tr.cast.json
# compile_cast -> casm::Program -> AotCompiler::new().compile_c(&program, "m", "gcc")
# -> Module::load + call_main()
```

Expected: ASCII grid frames (`___T___________________#`, `score: 0`, etc.)
on stdout, matching exactly what the interpreter path
(`crush-walk-run`/`crush_vm::run`) produces for the same program. Actual:
repeating blocks of `1.73347e-308` where string lines should be, with only
the numeric `score` lines and the final `call_main()` return value coming
through correctly.

## Impact

Any AOT-compiled program that prints or otherwise surfaces string values
via the C backend produces silently wrong output — not a crash, not an
error, just corrupted data that looks superficially plausible (a real
floating-point-looking number) rather than obviously broken. That's a
worse failure mode than `CRUSH-5`'s hard compile error: a caller could
easily ship this without noticing unless they specifically diff output
against the interpreter path, which is exactly how this was found (used
`crush-walk-run` on the same program first to know what correct output
should look like).

## Technical approach

Not investigated — found via output comparison, not source-diving.
Starting point: `crates/crush-aot/src/codegen_c.rs`'s string-value
representation and whatever the generated C's `printf`/output call site
looks like for a string-typed local — the `1.73347e-308` value is a strong
signal this is a format-specifier or union-tag mismatch (printing a
pointer's bit pattern as a `double`), not a logic error in how strings are
built (string *concatenation itself* appears to work fine internally,
since `build_air_row`/`build_ground_row`'s recursive string-building
produces the right *length* of output structurally — the corruption is at
the print/output boundary specifically).

## Files to modify

- `crates/crush-aot/src/codegen_c.rs`

## Non-goals

- Fixing the Rust-codegen backend (`CRUSH-5`, separate — that one doesn't
  even compile, so this bug doesn't apply there)
