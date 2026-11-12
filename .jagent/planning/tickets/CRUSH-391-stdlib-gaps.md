# CRUSH-391 — stdlib gaps: array pop, io.eprint (stderr), str.ord/str.chr

| Field | Value |
|-------|-------|
| **ID** | CRUSH-391 |
| **Priority** | P2 |
| **Status** | Done |
| **Phase** | M1 |
| **Assignee** | opencode |
| **Dependencies** | none |
| **Estimated effort** | M |

Filed by: opencode — 2026-11-10, from GitHub issue #370
(repo ticket 370 was taken — `CRUSH-370-remove-invariant-runtime` — so this is
CRUSH-391; the branch/commit may still say 370, referring to the GitHub issue.)

## Problem

From GitHub issue #370 (on 0.9.0): three stdlib gaps, each rejected up front by
`crush-run` ("not available in this host"):

```crush
fn main() {
    let a = [1, 2]
    let x = a.pop()
    io.eprint("e")
    print(str.ord("A"))
}
```

```text
[capabilities] b4.crush needs capabilities this run does not grant; nothing was run:
  not available in this host: io.eprint, pop, str.ord
```

- `array.push` exists but there is no `pop` (method form `a.pop()`; the
  function form `array.pop(a)` was already present at commit 3ae0b4a).
- There is no way to write to stderr — a CLI cannot separate diagnostics
  from output.
- No char <-> code conversion (`str.ord` / `str.chr`).

## Success criteria

- [x] `a.pop()` (method form) lowers to the same shared implementation as the
      existing array-pop function — one capability, one implementation.
- [x] `io.eprint(...)` writes its args to stderr, joined with NO separator
      (matching io.print's joining), respecting the `--max-output` quota.
      (Already on main since CRUSH-241; verified, not reimplemented.)
- [x] `str.ord("A")` → 65 (first char's Unicode scalar value; empty string →
      clear runtime error); `str.chr(65)` → "A" (out-of-range → clear error).
      (Aliases of the existing conv.ord/conv.chr.)
- [x] `crush-run caps` lists the new capability names.
- [x] Repro program from the issue runs: `e` on stderr, `65` on stdout, pop
      semantics right (`[1, 2].pop()` → 2).
- [x] Unit tests where the stdlib lives + end-to-end .crush repro + caps
      listing test; FastVM/JIT/AOT agree with the shared implementation.

## Technical approach

- Verify what main already has (v0.11.2) before writing code — run the repro
  and `crush-run caps`.
- Find `array.push`'s implementation end to end (stdlib registration, method
  whitelist, caps listing, per-engine wiring) and mirror it for the missing
  pieces.
- If a backend genuinely can't implement a capability, refuse loudly there
  rather than silently diverging, and note it in this ticket.

## Files to modify

- `crates/crush-vm/src/caps.rs` — register `pop`, `str.ord`, `str.chr`
- `crates/crush-vm/src/portable_vm.rs`, `crates/crush-vm/src/scheduler.rs` — dispatch arms (conv aliases + `pop`)
- `crates/crush-vm/src/fastvm/instructions.rs`, `crates/crush-vm/src/fastvm/execution.rs` — new `FastOp::ArrayPopValue` for `cap_call "pop"`
- `crates/crush-jit/src/{compiler,runtime,lib}.rs` — JIT helper `OP_ARRAY_POP_VALUE`
- `crates/crush-aot/src/codegen.rs`, `codegen_c.rs` — cap_call `pop` + conv aliases
- `crates/crush-aotc/src/codegen.rs` — conv aliases; loud refusal for `pop`
- `crates/crush-lang-sdk/src/bin/crush-run.rs` — `caps` listing
- `docs/design/sbl.md` — capability table
- tests: `crush-vm/src/tests/capabilities.rs`, `crush-lang-sdk/src/compile.rs`, `crush-lang-sdk/tests/crush_run_test.rs`, `crush-aot/tests/stdout_parity.rs`

## Non-goals

- Global reformatting (separate ticket).
- CHANGELOG.md edits (release-please owns it).

## Resolution

Done 2026-11-10, branch `agent/opencode/CRUSH-370` (as CRUSH-391 — see note above).

**Already on main (v0.11.2) — nothing to do:**
- `io.eprint` (CRUSH-241, stdout's stderr twin) — works on every engine, quota-respecting.
- `array.pop(a)` function form — lowers to the `array_pop` opcode + swap/pop compensation.
- `conv.ord` / `conv.chr` (CRUSH-117) — always-on portable codepoint conversion with exactly the
  error behavior the issue asks for (empty string → "single Unicode character" TypeError, invalid
  codepoint → error), implemented in PortableVM, scheduler, AOT-Rust, AOT-C and aotc (the
  FastVM/JIT take capabilities from their caller, so there the conv.* pair comes pre-supplied).

**Added (the actual gaps):**
- `a.pop()` method form: the parser emits it as a `CapabilityCall "a.pop"`; the compiler's generic
  method path lowers it to `cap_call "pop"` — which was never registered. Registered `pop`
  (argc 1, returns) in `crush-vm/src/caps.rs` and implemented the dispatch in PortableVM and
  scheduler: the receiver is consumed and mutated in place, the removed element is the value
  (null for an empty array), TypeError on a non-array receiver.
- FastVM/JIT: `cap_call "pop" argc=1` previously mapped to the two-value `ArrayPop` opcode
  (leaves `[array, value]`) — a stack imbalance that never fired because `pop` was refused before
  execution. New native `FastOp::ArrayPopValue` (+ JIT helper `OP_ARRAY_POP_VALUE`) pushes only
  the element, matching the CVM1 capability contract; both engines verified.
- AOT Rust + AOT C: cap_call `pop` arms (element-only), matching. aotc refuses `pop` loudly
  (no array model there — `new_array` is a stub), following CRUSH-241's io.eprint precedent.
- `str.ord` / `str.chr`: registered as always-on portable names that are ALIASES of
  `conv.ord` / `conv.chr` — every engine routes them to the same shared implementation
  (recorded as a dejavue decision). Semantics: single character → codepoint; empty or
  multi-character string → clear TypeError; out-of-range codepoint → clear TypeError.

**Verification (live, not just compile):**
- Issue repro runs: `e` on stderr only, `65` on stdout (shown with `2>/dev/null` / `1>/dev/null`
  splits), exit 0.
- `crush-run caps` lists `push/pop` and `str.ord/chr` as built-in portable; `crush-run caps
  repro.crush` reports `needs no grants` + `also uses (ambient): io.eprint, io.print, pop, str.ord`.
- `assert_prints_everywhere` parity test (VM, FastVM, JIT, AOT-Rust, AOT-C): pop semantics agree
  on all five engines. Found and fixed two real bugs the first AOT draft had (Option-pattern
  mismatch; wrong expected literal in my own test).
- Suites: crush-vm 348+2 doc, crush-lang-sdk (incl. 25 CLI tests), crush-jit 102, crush-aot
  (incl. stdout_parity 13), crush-aotc — all green. Clippy: no new warnings from this change
  (existing warnings untouched; the pre-existing `approx_constant` denials in old test code are
  not this ticket's scope).

**Note:** `push`/`append`/`arr_get`/`arr_set`/`conv.chr`/`conv.ord` were registered but missing
from the `caps` listing; the new listing lines cover push/pop and str.ord/chr.

## Resolution

(TBD)
