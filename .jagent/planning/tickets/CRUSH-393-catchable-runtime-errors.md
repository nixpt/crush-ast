# CRUSH-393 — try/catch does not catch runtime errors (array index out of range, failing fs.read)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-393 |
| **Priority** | P1 |
| **Status** | Done |
| **Phase** | VM |
| **Assignee** | opencode |
| **Dependencies** | none |
| **Estimated effort** | M |

Filed by opencode — 2026-11-10, from GitHub issue #369; renumbered from CRUSH-369 (ID collision with CRUSH-369-annotation-warnings-in-toolchain.md, per CONTRIBUTING's renumber-the-newer rule).

## Problem

`try/catch` only handled `throw`; VM runtime errors aborted the program even inside `try`:

```crush
fn main() {
    let a = [1, 2]
    try {
        print(a[99])
    } catch e {
        print("caught")
    }
    print("after")
}
```

printed `[runtime] array index out of range: 99 (len 2)` and exited 1 — neither `caught` nor
`after` printed. Same for `fs.read("missing")` (with misleading `unknown capability` wording for
a *failing granted* capability, cf. #67 / CRUSH-241).

## Success criteria

- [x] The repro above prints `caught` then `after`, exit 0.
- [x] `fs.read("missing")` inside try/catch with `--fs` is caught; the message names the real problem.
- [x] Quota exhaustion (steps/call-depth) stays fatal.
- [x] FastVM agrees with the CVM1 engines; JIT refuses loudly; AOT already refused.
- [x] Existing suites (backend_agreement, examples_parity, scripting_parity) stay green.

## Resolution

**Design chosen.** A VM fault raised by an instruction now unwinds to the innermost active
`try` handler of the current task as a caught `Value::Error(message)` — the same unwind a
`THROW` of that value does (call stack and operand stack truncated to where `ENTER_TRY` ran,
handler popped, error value pushed; a fault in a callee is caught by the caller's try, per
CRUSH-126). With no handler active, the fault aborts the run with today's exact error and exit
status. One shared implementation — `crush_vm::vm::raise_fault_to_handler` plus
`VmError::is_catchable` — is called by both CVM1 engines (the scheduler, which `crush-run` /
the SDK `Runtime` actually run on, and the reference `PortableVm`), so they cannot drift. A
fault unwinding past an inline-lowered polyglot block's open capture fails the block (fatal
`LangRuntimeError`, partial output dropped), same as an escaping `THROW` (#312).

**Fault classification** (`VmError::is_catchable`, fault site by fault site):

- **Catchable — program-level faults:** `ArrayBounds`/`BadIndex` (index/slice out of range),
  `DivByZero`, `ArithmeticOverflow`, `TypeError` (runtime type errors, incl. missing map key),
  `NotCallable`/`FuncArity`/`ApplyArgsType` (call faults), `CapFailed`/`CapArity` (a failing
  **granted** capability — `fs.read` ENOENT reads as an IO/runtime error, per #67's distinction),
  `Io` (capability I/O), `Parallel` (a `parallel_map` item/worker failure, #175), `PrintFunc`.
- **Fatal — still aborts:** all quotas (`StackQuota`, `StepQuota`, `OutputQuota`,
  `WallTimeQuota`, `CallDepthQuota` — the sandbox's limits, not the program's);
  capability-security decisions (`UnknownCap`, `CapDenied`, `CapNotDeclared` — an *ungranted*
  name is not a runtime failure); `Exit` (a deliberate `sys.exit` must end the run so a CLI can
  exit with its own status); broken VM/bytecode state (`StackUnderflow`, `UnknownOpcode`,
  `TruncatedInstruction`, `ConstOutOfRange`, `UninitSlot`, `BadJump`, `UnknownFunction`,
  `Unsupported`); `Uncaught` (a throw that found no handler at the throw site — a catch-block
  rethrow to no outer handler aborts); `LangRuntimeError` (polyglot block failures stay fatal
  per #312 — guest exceptions are the guest's own to catch); `Cosm` (CRUSH-zorro-43's own
  design).

**Caught value shape.** The handler's error variable binds `Value::Error(message)` — the
existing error value type — so it prints as `error(<message>)` and `type_of` reports `error`,
distinguishing caught faults from thrown strings. The message is the fault's own text (today's
`[runtime]` message without the prefix).

**Per-engine status:**

- **Scheduler** (the engine `crush`/`crush-run`/SDK use): catches faults; full classification.
- **PortableVm** (the CVM1 reference): same shared unwinder, agrees.
- **FastVM**: agrees. `handler_frame` is shared with its `Throw` opcode; catchable FastVM
  faults (`DivisionByZero`, `ArithmeticOverflow`, `TypeMismatch`) unwind to the handler, the
  caught value is the fault message (`RuntimeValue` has no error variant, so the message binds
  as a string — its text matches the CVM1 wording for div/overflow; `TypeMismatch` carries no
  expected/got detail on FastVM, noted divergence). Quota/security/broken-state stay fatal.
- **JIT**: refuses loudly, no silent divergence. Its fault dispatch isn't wired to handlers
  (and its fault sites already diverge — OOB index yields null, a pre-existing gap the
  differential harness tracks), so when a fault lands while a handler is still active, the run
  fails with `try/catch around VM faults is not supported on this backend yet (CRUSH-369): …`.
  Converting it means handler dispatch at every checked-helper call site in Cranelift — too
  large for this ticket; throw/catch of thrown values keeps working as before.
- **AOT**: already refuses all try/catch at compile time (`UnsupportedOps` lists `enter_try`),
  so it cannot silently diverge; examples_parity skips it as a declared backend gap.

**Wording (#67):** already fixed on main by CRUSH-241/#370 — a failing granted capability
reports `fs.read: fs.read missing.txt: No such file or directory (os error 2)`, and
`unknown capability` is reserved for names the VM has no handler for (pinned by
`crush-lang-sdk/tests/stderr_and_stats.rs`). Verified against this branch; nothing further to
change.

**Relationships.** Implements the catchability core of CRUSH-zorro-34 ("VM runtime errors as
catchable typed exceptions") for the crush-level `try/catch`; its walker-level typed mapping
(`except ZeroDivisionError` → error kind) stays with that ticket — `Value::Error(message)`
carries no kind yet, so kind-tagged payloads are zorro-34's layer to add. Also resolves the
catch half of CRUSH-219 and its known gap `t11_missing_file`
(`crush-lang-sdk/tests/scripting_parity.rs`, whose KNOWN_GAPS is now empty).

## Verification evidence (2026-11-10, this branch)

- `cargo build -p crush-lang-sdk` — Finished, dev profile.
- Issue repro: `caught` / `after`, exit 0. `fs.read("missing")` under `--fs` inside try:
  `caught: error(fs.read: fs.read missing.txt: No such file or directory (os error 2))` /
  `after`, exit 0. Uncaught fault outside try: same `[runtime]` message as before, exit 1.
- Edge programs: rethrow from catch reaches the outer try; fault in a callee unwinds to the
  caller's try; fault in a catch caught by the outer try; infinite recursion inside try still
  dies with `call depth quota exceeded` (exit 1); a spawned task with its own try catches its
  own fault; a spawned task's uncaught fault is fatal; an `@python` guest failure stays fatal.
- Suites: `crush-vm` + `crush-frontend` + `crush-lang-sdk` 1052 passed / 0 failed;
  `crush-jit` 103 passed / 0 failed; `crush-aot` (backend_agreement, examples_parity,
  differential_aot) green; `scripting_parity` green.
- Clippy (`-p crush-vm -p crush-jit -p crush-lang-sdk --all-targets`): no warnings pointing at
  the changed code; per-file warning counts unchanged vs the base commit.

## Files to modify

- `crates/crush-vm/src/vm.rs` — `VmError::is_catchable` classification;
  `raise_fault_to_handler` shared unwinder. (done)
- `crates/crush-vm/src/scheduler.rs` — intercept instruction faults at the `execute_one` call
  site. (done)
- `crates/crush-vm/src/portable_vm.rs` — intercept at the `execute_instruction` call site in
  `step()`. (done)
- `crates/crush-vm/src/fastvm/{execution.rs,mod.rs}` — shared handler walk + fault recovery in
  the run loop. (done)
- `crates/crush-jit/src/lib.rs` — loud refusal message when a fault lands inside try. (done)
- `crates/crush-lang-sdk/tests/scripting_parity.rs` — remove the fixed KNOWN_GAPS entry. (done)
- Tests: `crates/crush-vm/src/tests/control_flow.rs`, `crates/crush-vm/src/portable_vm.rs`,
  `crates/crush-vm/src/fastvm/mod.rs`, `crates/crush-jit/src/lib.rs`. (done)

## Non-goals

- Python/JS walker `except TypeError`-style typed mapping (CRUSH-zorro-34).
- JIT handler dispatch for VM faults (refuses loudly instead — see Resolution).
- Global `cargo fmt` (separate ticket).
- CHANGELOG.md.
