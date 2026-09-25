# CRUSH-123: the conformance runner has no per-file timeout, and one file hangs the frontend

**Status**: open, filed 2026-09-25 (captain, from the [main] foreman's review of #61)
**Priority**: high. The full conformance corpus never finishes, so it cannot gate CI.
**Relates to**: CRUSH-122 (#61 made the runner accept `// caps:` and file selection),
M2 conformance closure.

## Problem
`xtask/src/conformance.rs` runs every corpus file **in-process**:
`compile_crush_source` → `PortableVm`. VM quotas bound execution, but nothing bounds the
**frontend**. One file whose parse or compile never terminates therefore stalls the whole run
with no output.

**Reproduced on main (`f6696ad`):**
```
conformance examples/crush/ai_agent_ops.crush   # 68 lines
→ still running, no output, killed by `timeout 60` (rc 124)
```
#61 also reports 24 other corpus files that fail identically on `main`. They are a separate
triage item.

## Fix (two parts, both needed)
1. **Runner isolation:** execute each file in a child process (re-exec `conformance --one
   <file>`) with a per-file wall-clock timeout (default 10 s, `// timeout: N` annotation to
   override). Report `TIMEOUT` as its own outcome, so one bad file costs its timeout and not
   the run.
2. **The hang itself:** bisect `ai_agent_ops.crush` to the construct the parser or compiler
   loops on, and fix the frontend. A 68-line file must compile or fail in milliseconds.
   Add the minimal repro as a frontend regression test.

## Done when
- `conformance` (full corpus) terminates. `ai_agent_ops.crush` either passes or reports a
  real error instead of hanging.
- A deliberately looping file reports `TIMEOUT` and the run continues. This is the
  mutation check for part 1.
- Runtime and the outcome counts are recorded here. The 24 known failures are triaged into
  follow-up tickets or expected-failure annotations.
