# CRUSH-363 — Call a subscript: `h[k](41)` is a parse error ("Cannot call non-function")

| Field | Value |
|-------|-------|
| **ID** | CRUSH-363 |
| **Priority** | P2 |
| **Status** | Done |
| **Phase** | M6 (frontends) |
| **Assignee** | opencode |
| **Dependencies** | CRUSH-251 (function values) |
| **Estimated effort** | S |
| **Filed by** | opencode — 2026-11-10, from GitHub issue #363 |

## Problem

A subscript expression can't be called directly. `let f = h[k]; print(f(41))`
works and prints 42, so the value is callable; only the parser's call rule
rejects a call on a subscript (and other non-identifier callees):

```crush
fn main() {
    let h = { add: |a| => a + 1 }
    let k = "add"
    print(h[k](41))
}
```

```text
[compile] Parse errors: Unexpected token at line 4, column 19: Cannot call non-function
```

The parse rule only accepts a `Var` callee (`Expression::Call`) or a
`Var.field` callee (`Expression::CapabilityCall`); any other postfix
expression — `h[k]`, `f(x)`, a struct literal — hits "Cannot call
non-function" (`crates/crush-frontend/src/parser/mod.rs`, the `op == "()"`
branch).

## Success criteria

- [x] `expr[index](args)` parses and calls the value, printing 42 for the issue's program
- [x] `f(x)(y)` and `S { n: 3 }.size()` / `f().m(x)` (computed receivers) parse and run too
- [x] Semantics identical to the two-step form (`let f = h[k]; f(41)`): the callee's value is looked up and called at run time (`call_value`); a genuinely non-callable value still fails clearly at run time
- [x] The tree-sitter grammar (`crates/tree-sitter-crush`) agrees; parser regenerated the way prior PRs did
- [x] Parser unit test in crush-frontend + end-to-end `.crush` repro run green

## Resolution

**Root cause:** the parser's `()` postfix rule (`crates/crush-frontend/src/parser/mod.rs`,
`parse_expression_with_precedence`) only accepted a `Var` callee (→ `Expression::Call`)
or a `Var.field` callee (→ dotted `Expression::CapabilityCall`) and emitted
"Cannot call non-function" for every other postfix expression — a subscript
(`h[k]`), a call result (`f(x)`), a struct literal.

**Fix:** a parse-rule relaxation plus a CAST node to carry the computed callee.

- `crush-cast`: new `Expression::CallValue { callee, args, meta }` variant.
- parser: the two fast paths stay (so `f(x)`, `obj.m(x)` compile exactly as
  before); a `GetField` over a computed target now emits `MethodCall`
  (`call_member`, CRUSH-277 S5) and every other postfix expression emits
  `CallValue`.
- compiler: `CallValue` compiles to the exact instruction shape the
  declared-variable path already emitted for `let f = …; f(args)` — args
  last-first, the callee value on top, `call_value argc` — so semantics are
  identical to the two-step form and a non-function still fails at run time
  ("<type> is not callable"); FastVM/JIT keep refusing `call_value` as they
  did for the two-step form (CRUSH-251's tracked gap, unchanged).
- `semantics`/`render`/`optimizer`/`cast_enrich`/`crush-index`: recursion and
  type arms for the new variant (args checked, result `Any`, rendered
  `callee(args)`, precedence 90).
- `tree-sitter-crush`: `capability_call`'s name now accepts an
  `index_expression` (`h[k](41)`) and a nested `capability_call`
  (`f(x)(y)`, `obj.f(x)(y)`); parser.c/grammar.json/node-types.json
  regenerated with tree-sitter 0.25.10 (the version that generated the
  committed parser.c); corpus `test/corpus/call_values.txt` + fixture
  `test_call_values.crush`.

Out of scope, unchanged: `h.add(41)` still parses as a capability call (the
companion issue); FastVM/JIT still refuse function values (CRUSH-251).

**Verification** (crush-run built from this branch, all with `< /dev/null`):

- the issue's program prints `42` (was: parse error "Cannot call non-function")
- `make_adder(10)(32)` → 42; `h[k](6)` → 36; `fs[1](40)` → 42;
  `(make_adder(7))(1)` → 8; `Point { x: 3, y: 4 }.sum()` → 7;
  `Point::new(3, 4).sum()` → 7; the two-step form still → 42
- `let x = 5; x(3)` and `42(3)`: `[runtime] int is not callable: only
  functions can be called`, exit 1
- `cargo test -p crush-frontend` green (incl. the two new parser tests);
  `cargo test -p tree-sitter-crush` green; `tree-sitter test` 21/21
- `cargo clippy -p crush-frontend --all-targets`: no new warnings from this
  change; touched hunks are rustfmt-clean

## Technical approach

- CAST: a call whose callee is an expression — `Expression::CallValue { callee, args }` (the shape CRUSH-zorro-24 sketched; this ticket lands its parse-rule core).
- Parser: in the `()` postfix branch, keep the `Var` → `Call` and `Var.field` → `CapabilityCall` fast paths; route a `GetField` over a computed target to `MethodCall` (`call_member`, CRUSH-277 S5) and every other postfix expression to `CallValue`.
- Compiler: args last-first (`compile_arg`), then the callee expression, then `call_value argc` — exactly the instruction shape the declared-variable path emits, so FastVM/JIT keep refusing function values as they did for the two-step form.
- tree-sitter: let a `capability_call`'s name be an `index_expression` (and a nested call for `f(x)(y)`); regenerate parser.c/grammar.json/node-types.json with the tree-sitter CLI.

## Files to modify

- `crates/crush-cast/src/lib.rs` — new `Expression::CallValue` variant
- `crates/crush-frontend/src/parser/mod.rs` — relax the call rule
- `crates/crush-frontend/src/compiler.rs` — compile `CallValue` to `call_value`
- `crates/crush-frontend/src/{semantics,render,optimizer,cast_enrich}.rs` + `crates/crush-index/src/index.rs` — recursion/type arms for the new variant
- `crates/tree-sitter-crush/grammar.js` + regenerated `src/` — grammar agrees

## Non-goals

- `h.add(41)` being treated as a capability call (companion issue — out of scope)
- Function values on FastVM/JIT (CRUSH-251 tracks that; the two-step form refuses there too)
