# Handoff

Updated: 2026-07-25T03:30:00-05:00

## Summary
Docs/planning refresh + CRUSH-66 filing (no code). Synced memory to `main`
`5fb5bff` (M2 JIT merge). Marked CRUSH-20 ticket Done. Designed and filed
CRUSH-66: wire `@lang[pypi:/npm:]` through existing `bucket_exec` /
`resolve_multi` once BUCKETS-15 lands on sibling buckets.

## Next Steps
1. Merge [nixpt/buckets#4](https://github.com/nixpt/buckets/pull/4) (BUCKETS-15).  
2. Implement [CRUSH-66](../.jagent/planning/tickets/CRUSH-66-lang-deps-pypi-npm.md) per [design](../docs/design/lang-deps-pypi-npm.md) — likely small: deps already pass to `resolve_multi`; verify PYTHONPATH/NODE_PATH + live tests + doc comment fixes.  
3. Optional: review panini Math.* fix worktree; or start M5 (CRUSH-1 AI opcodes).

## Boot Instructions
Read `.dejavue/handoff.md`, `.dejavue/state.md`, `.dejavue/decisions.md`, and `.dejavue/timeline.jsonl` before making changes.

```bash
cd /workspace/projects/crush-ast && dejavue context
cat .jagent/planning/STATE.md .jagent/planning/TASKS.md
# buckets consumers
rg -n 'crush-buckets|sandboxed-polyglot' crates/*/Cargo.toml
```

## Key paths

| What | Where |
|------|--------|
| CRUSH-66 ticket | `.jagent/planning/tickets/CRUSH-66-lang-deps-pypi-npm.md` |
| Design | `docs/design/lang-deps-pypi-npm.md` |
| Sandbox wiring | `crates/crush-vm/src/bucket_exec.rs` |
| crush-pkg runners | `crates/crush-pkg/src/runners.rs` |
