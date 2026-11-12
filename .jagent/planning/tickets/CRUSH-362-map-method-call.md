# CRUSH-362 — Method-call syntax on a local map compiles to a host capability call instead of calling the stored function

| Field | Value |
|-------|-------|
| **ID** | CRUSH-362 |
| **Priority** | P1 |
| **Status** | Done |
| **Phase** | M2 |
| **Assignee** | opencode |
| **Dependencies** | none |
| **Estimated effort** | M |

Filed by opencode — 2026-11-12, from GitHub issue #362.

## Problem

Calling a function stored in a **local map** with method syntax compiles to a
host-capability call named after the field, instead of calling the stored
function. Checked on the v0.9.0 release binaries (2026-10-31).

```crush
fn main() {
    let h = { add: |a| => a + 1 }
    print(h.add(41))
}
```

```text
$ crush-run run call_map_field.crush < /dev/null
[capabilities] call_map_field.crush needs capabilities this run does not grant; nothing was run:
  not available in this host: add
```

## Success criteria

- [ ] `h.add(41)` (function stored in a local map) calls the stored function and prints 42
- [ ] Genuine capability calls are unchanged (`fs.read` still requires `--fs` etc.)
- [ ] Method-form on structs (existing behavior) is unchanged
- [ ] `obj.f(x)` where the field does NOT hold a function behaves sensibly (clear runtime error, not a silent capability refusal); exact semantics documented below
- [ ] Parser/compiler-level test + end-to-end repro

## Technical approach

- The parser's postfix rule lowers `Var.field(...)` into a dotted
  `Expression::CapabilityCall` — but a map field holding a function is not a
  capability.
- Resolve on the receiver's runtime type instead: reuse the existing
  `call_member`/`call_value` dispatch so `h.add(41)` calls the map value under
  key `"add"` at runtime; only emit a `CapabilityCall` when the callee is a
  capability (`fs.read`, `io.print`, ...).
- The tree-sitter grammar needs no changes (the syntax already parses).

## Files to modify

- `crates/crush-frontend/src/parser/mod.rs` — postfix-call lowering for dotted callees
- `crates/crush-vm/...` — runtime member-call dispatch (if needed)
- tests in `crush-frontend` / `crush-vm`

## Non-goals

- No grammar changes.
- No new opcodes if an existing member-call path suffices.
- Not touching the postfix-call/member-call rework in unmerged PR #416
  (`agent/opencode/CRUSH-363`) — keep this fix minimal and compatible with it.

## Resolution

**Root cause.** The parser lowers `Var.field(args)` into a dotted
`Expression::CapabilityCall { name: "obj.field" }`. The compiler split that
name on the dot and, when the receiver was a declared variable and the field
was not a struct method, emitted `cap_call "<field>"` — a *host capability*
request named after the field. A map field holding a lambda is not a
capability, so the run was refused in preflight ("not available in this host:
add") and never executed.

**Design.** Runtime-side resolution, reusing the existing member-call path —
no new opcode, no grammar change:

- *Compiler* (`crush-frontend/src/compiler.rs`): an unmarked dotted call on a
  **declared** receiver now lowers to `call_member` (args last-first, receiver
  on top) — the same instruction `p.norm2()` uses (CRUSH-277 S5) — *unless*
  the field names a dot-less host capability invoked in method form
  (`push`, `arr_set`, `arr_get`, `arr_slice`, `append`, `make_range`,
  `parallel_map`; CRUSH-235 arg order preserved). That set is the new
  `RECEIVER_CAPABILITIES` constant; a cross-crate test in crush-lang-sdk
  (`method_call_receiver_caps.rs`) fails if it drifts from the VM's capability
  registry (the frontend can't depend on crush-vm). Namespaced capability
  calls (`fs.read`) never had a declared receiver, so they are untouched.
- *Runtime* (`crush-vm/src/cosm_rt.rs`, `call_member` — the one shared
  implementation every VM calls into): a `Value::Map` receiver calls the
  function stored under the key, with the lambda's captured values as the
  leading parameters (the `CALL_VALUE` convention). Scheduler VM and
  PortableVm both go through it; FastVM/JIT already refuse member ops at load;
  AOT refuses at compile time (unchanged).

**Decided semantics for `obj.f(x)` (criterion 4):**

- map, key holds a function → it is called (`h.add(41)` → `42`); captures work.
- map, key missing → runtime error `map has no member \`f\`: a member call on a
  map calls the value stored under the key`.
- map, key holds a non-function → runtime error `int is not callable…` (the
  same check `CALL_VALUE` applies); wrong arity → `FuncArity`.
- struct instance without the method → unchanged `` `Kind` has no method `f` ``.
- any other receiver (int, string, …) → type error naming the supported
  receivers: "member calls resolve on struct instances (COSM Records) and
  maps".
- **Caveat:** a map field named exactly like a bare host capability
  (`push`, `arr_set`, …) called in method form on a declared local keeps the
  capability semantics — rename the field or call it via the value
  (`f = h.push; f(…)`) to reach the stored function. Struct methods still win
  over capabilities (unchanged: `receiver_methods` is checked first).

**Compatibility.** No parser changes, so unmerged PR #416
(`agent/opencode/CRUSH-363`) is unaffected; its `MethodCall` lowering flows
through the same compiler path.

**Verification.** `cargo build -p crush-lang-sdk`; repro prints `42`;
`fs.read` still refused without `--fs` ("--fs: fs.read") and works with it;
`xs.arr_slice(1,3)` / `zs.arr_set(1,7)` still reach the host capabilities;
`p.norm2()` / `p.clone()` unchanged. `cargo test -p crush-frontend
-p crush-vm -p crush-lang-sdk`: 1460 passed, 0 failed. Clippy: no new
warnings from the changed code. Tests added: `gh_issue_362_map_method_call.rs`
(end-to-end, 7 cases), `records.rs` member-call-on-map cases (both VMs),
`cosm_lowering.rs` compile-level case, drift guard
`method_call_receiver_caps.rs`.

**Non-goals honored:** no grammar changes, no new opcodes, no CHANGELOG edit.
