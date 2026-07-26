# State

Updated: 2026-07-25T03:30:00-05:00

crush-ast `main` @ `5fb5bff` (= origin): M2 JIT Phases 2–7 merged (PR #21,
CRUSH-26..38 band). M1 correctness sweep done earlier. Buckets consumers
healthy — `crush-vm` (`sandboxed-polyglot`, CRUSH-20 ✅) and `crush-pkg` both
use `package = "crush-buckets"` path-deps and `cargo check` clean against
current sibling buckets. CRUSH-20’s deferred “numpy reframe” is now
actionable: BUCKETS-15 adds `pypi:`/`npm:` resolvers; follow-on filed as
CRUSH-66 (Ready) with design at `docs/design/lang-deps-pypi-npm.md` —
blocked only on merging buckets#4. Dejavue/planning were stale (handoff
still on 2026-07-15 Math.floor); refreshed this session. In-flight
elsewhere: panini Math.* lowering worktree. Next: merge buckets#4 →
implement CRUSH-66, or M5 AI opcodes / M3 debugger.
