# Handoff

Updated: 2026-08-23

## Summary
CRUSH-115 is complete on `agent/buffy/CRUSH-115`. `io.read` is registered as a
zero-argument, non-privileged capability and reads one line from stdin through
the shared `crush-vm::io_read` helper. LF and CRLF endings are stripped and EOF
or read errors return the empty string.

The capability is wired through the scheduler/CVM1, PortableVM, Rust AOT, C
AOT, and AOT-C paths. The JIT intentionally uses its existing unsupported-op
fallback rather than attempting to compile blocking stdin I/O.

## Verification
- `crush-vm` io-read tests: 3 passed.
- `crush-vm` capability tests: 10 passed.
- Real `crush-run` source pipeline with piped stdin: 1 passed.
- Rust AOT io-read codegen/compile test: 1 passed.
- C AOT io-read codegen test: 1 passed.
- C AOT gcc compile/load test: 1 passed.
- AOT-C stdin-helper test: 1 passed.
- `git diff --check`: passed before ticket-memory edits.

## Remaining work
CRUSH-118 remains open: add a user-facing interactive example under
`examples/crush/` and verify it with piped input. Do not mark CRUSH-118 done
from capability-level tests alone.

## Next Steps
1. Ship/merge CRUSH-115.
2. Implement CRUSH-118 as a separate example/demo change.
3. Continue with the next correctness-spine ticket after the demo.
