# CRUSH-114 — `len()` diverges across backends: errors on strings in the VM, handles them in AOT

| Field | Value |
|-------|-------|
| **ID** | CRUSH-114 |
| **Priority** | P3 |
| **Status** | Done |
| **Phase** | M1 |
| **Assignee** | unassigned |
| **Dependencies** | none (see CRUSH-13 for the analogous arithmetic divergence) |
| **Estimated effort** | S |

## Problem

The `len` builtin (registered in `semantics.rs` as `(vec![Any], Int)`)
compiles to the `len` opcode, which the scheduler/portable VM lower to
`ARR_LEN` — and `ARR_LEN` calls `need_array(v)`, returning
`type error: expected array, got str` for a string. But `str.len(s)` (a
capability) works on strings, and the AOT C backend's `len` codegen explicitly
handles `RuntimeValue::String(s) => s.len()`. So the three "length" paths
disagree.

## Impact

A user who reaches for `len()` — the obvious, type-checker-blessed name — on a
string gets a runtime crash, while `str.len()` works. Surprising and
unhelpful, and the `awesome-crush` interpreters had to learn the hard way to
use `str.len` instead of `len` for strings.

## Reproduction

```bash
printf 'fn main() { io.print(len("abc")); return 0; }\n' > /tmp/l.crush
crushc /tmp/l.crush -o /tmp/l.cvm1 && crush-run run /tmp/l.cvm1 --cap io.print
# [runtime] type error: expected array, got str
```

## Success criteria

- [ ] `len("abc")` returns 3 (matching `str.len("abc")` and the AOT backend), or `len()` on a string fails at **compile** time with a clear message.
- [ ] The VM's `len`/`ARR_LEN` and the AOT `len` codegen agree on strings.

## Technical approach

- Make `ARR_LEN` (scheduler.rs ~line 866) accept `Value::Str` and return `s.chars().count()` — mirroring the `ARR_GET` string path already in scheduler.rs (~line 844).
- Alternatively, if strings should stay out of `len()`, have the type checker reject `len(<String>)` at compile time.

## Files to modify

- `crates/crush-vm/src/scheduler.rs` — `ARR_LEN` / `need_array` usage.
- `crates/crush-vm/src/portable_vm.rs` — if it carries its own `len` path.
- `crates/crush-frontend/src/semantics.rs` — compile-time guard, if that's the chosen direction.

## Non-goals

- Unifying the other array-op divergences (CRUSH-13 tracks arithmetic; this is `len` only).

## Resolution (Done 2026-09-04, branch `agent/nixp/CRUSH-114`)

Repro re-verified on `main` before fixing (`len("abc")` → `[runtime] type
error: expected array, got str`), then fixed. Surveyed all seven `len`
paths: only scheduler + PortableVM `ARR_LEN` errored; `str.len` caps,
FastVM `Len`, JIT `OP_LEN`, AOT-Rust and AOT-C codegen all returned **byte**
length already — so canonical semantics is byte length (matching the
documented `str.len` "byte length of a string"), not the ticket's guessed
`chars().count()` (which would have diverged from `str.len` on non-ASCII).

New shared `crush_vm::str_len::str_len(&str) -> i64` helper (`io_read.rs`
pattern) now serves scheduler `ARR_LEN`, PortableVM `ARR_LEN`, both
`str.len` caps, FastVM `Len`, and JIT `OP_LEN`; AOT-Rust (`s.len()`) and
AOT-C (`strlen`) were already byte-correct and left as-is. `ARR_GET`'s
char-based indexing untouched (indexing ≠ length).

Verified: `len("abc")`/`str.len("abc")`/`len([1,2,3])` all print `3`
end-to-end via crushc+crush-run; `cargo test -p crush-vm` 144 pass (new
`arr_len_accepts_strings` + `test_portable_arr_len_string` + `str_len`
units, incl. pinned `len("héllo")==6` bytes); `cargo test -p crush-jit
--lib` 97 pass; new `aot_len_on_string_agrees` differential test green
(FastVM/AOT-Rust/AOT-C/interpreter/portable agree). The 2 `cargo clippy`
errors in `crush-vm` (`approx_constant` on 3.14 in tests/arith.rs,
tests/matrix.rs) are pre-existing on `main`, untouched.
