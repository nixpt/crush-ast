# CRUSH-18 — Polyglot block runtime errors (Python/JS/bash exceptions) aren't mapped into crush's own diagnostic system

| Field | Value |
|-------|-------|
| **ID** | CRUSH-18 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | M1 |
| **Assignee** | unassigned |
| **Dependencies** | none (adjacent to CRUSH-17 — same investigation, different error class) |
| **Estimated effort** | M |

## Problem

When a `@python { ... }` / `@javascript { ... }` / `@bash { ... }` polyglot
block's **guest code raises its own runtime exception**, crush does not
recognize this as a distinct error class at all. It's caught by the same
generic `else` branch that handles *any* non-zero subprocess exit
(`crates/crush-vm/src/scheduler.rs`, `EXEC_LANG`'s `if output.status.success()
{ ... } else { ... }`), which:

1. Labels it `VmError::UnknownCap` — the exact same variant used for "the
   capability doesn't exist" and "the capability wasn't granted". A guest
   Python program's `ZeroDivisionError` is not remotely an unknown-capability
   problem; reusing that variant is a category error, not just cosmetic.
2. Dumps the **raw, unparsed subprocess stderr verbatim** — a full Python
   traceback (with its own `File "<string>", line N` references, internal
   to the subprocess, unrelated to the `.crush` source's own line numbers)
   or a full Node.js stack trace (`at evalFunction (node:internal/...)`,
   ending with a `Node.js vX.Y.Z` version banner) — with zero crush-side
   location info tying the error back to *which* `@lang` block in the
   `.crush` source produced it.
3. Falls into the generic `E-RT05` ("Runtime VM") bucket at the outer
   diagnostic-rendering layer (`crush-lang-sdk::theme`'s `render_runtime_error`),
   which is a catch-all for *any* `VmError` — a polyglot guest exception and
   a VM stack-underflow bug currently render identically at the code-family
   level.

## Impact

Every `@python`/`@javascript`/`@bash` block that raises is currently a
UX regression relative to the rest of crush's error handling — which, per
CRUSH-17, is otherwise good (clean messages, precise `file:line:col`,
source snippets). Polyglot errors get none of that: no crush-side location,
a misleading "unknown capability" label, and a raw foreign-language
traceback dumped as if it were one flat string.

## Reproduction

```bash
cat > /tmp/py_error.crush <<'EOF'
fn main() {
    @python { 1/0 }
}
EOF
crushc /tmp/py_error.crush -o /tmp/py_error.cvm1
crush-run run /tmp/py_error.cvm1 --polyglot
```

Actual output:
```
[runtime] unknown capability: exec_lang(python): Traceback (most recent call last):
  File "<string>", line 1, in <module>
    1/0
    ~^~
ZeroDivisionError: division by zero
```

Same shape for JS (verified with `@javascript { null.foo }` — a full
Node.js stack trace + version banner dumped the same way) and presumably
bash (a bash script's own stderr, e.g. from `set -e` + a failing command,
though a bare `exit N` produces no stderr at all today, so that specific
case is at least not *actively* misleading — just uninformative).

## Technical approach (starting points, not a committed design)

1. **Give polyglot guest failures their own `VmError` variant** — something
   like `VmError::LangRuntimeError { lang: String, message: String }` —
   distinct from `UnknownCap` (capability system) and from generic VM bugs.
   Reserve `UnknownCap` for actual capability-system failures only.
2. **Consider a dedicated diagnostic code family** — e.g. `E-PG01` (polyglot
   guest error) alongside the existing `E-PP*`/`E-TP*`/`E-RT*` families in
   `crush-lang-sdk::theme`'s canonical code table, rather than folding into
   the generic `E-RT05` catch-all. Would need a corresponding
   `JsonDiagnostic::CODE_*` constant + wiring, matching the existing pattern.
3. **Location**: at minimum, surface the `.crush`-source line of the
   `@lang { ... }` block itself (the compiler already has this — it's a
   normal AST node with a source location) in the rendered error, even if
   mapping the *guest* language's own internal line numbers (Python's
   `line 1` is relative to the subprocess's `-c` argument, not the `.crush`
   file) is left as a stretch goal.
4. **Message shape**: decide whether to (a) keep the full guest traceback as
   supplementary detail but lead with a clean one-line summary (e.g.
   `` @python block raised `ZeroDivisionError: division by zero` `` — this
   would need lightly parsing the last line of a Python traceback, or
   Node's first stack-trace line), or (b) leave the full raw text but at
   least fix the mislabeling and add location. (a) is a better UX bar but
   requires per-language parsing heuristics that will need maintenance as
   each language's error format is its own moving target; (b) is a much
   smaller, safer first cut. Recommend starting with (b) and treating (a)
   as a follow-on once the basic mis-classification is fixed.
5. **Don't attempt to unify all 3 languages' error *shapes*** — Python
   tracebacks, Node stack traces, and bash's stderr conventions are
   genuinely different; the goal is consistent crush-side *framing*
   (a distinct error class, a stable code, a location), not forcing Python
   and JS errors to look identical to each other.

## Files to modify

- `crates/crush-vm/src/vm.rs` — new `VmError` variant
- `crates/crush-vm/src/scheduler.rs` / `crates/crush-vm/src/portable_vm.rs` —
  both `EXEC_LANG` handlers' failure branch (same fix needed in both — see
  CRUSH-13's note that these two files already have a history of drifting
  out of sync on `EXEC_LANG` semantics; whatever pattern is chosen here
  should probably be a shared helper, not duplicated inline twice)
- `crates/crush-lang-sdk/src/theme.rs` — new code family/constant if a
  dedicated `E-PG*` code is chosen, plus a rendering function

## Non-goals

- Structured, per-exception-type parsing of Python/JS errors (e.g.
  recognizing `ZeroDivisionError` vs `TypeError` as distinct crush-level
  types) — that's a much larger lift or a self-relearning API guessing
  game against a moving target upstream; the ask here is *framing*
  (class + code + location), not *translation* of the guest language's
  entire exception taxonomy into crush's own type system.
- Sandboxing/capability enforcement of the guest process itself — that's
  CRUSH-2 (already fixed) and the separate buckets-sandboxing opportunity
  in TASKS.md; this ticket is purely about error *reporting* quality.
- Fixing `EXEC_LANG`'s wall-clock-timeout or capability-gate paths — both
  already work correctly and are out of scope; only the "guest exited
  non-zero" branch is broken.
