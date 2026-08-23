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

## CRUSH-117 handoff

Implemented `conv.chr` and `conv.ord` as always-on portable capabilities.

### Contract

- `conv.chr(int)` returns a Unicode scalar string and rejects negative, out-of-range, and surrogate codepoints.
- `conv.ord(string)` returns the scalar codepoint and rejects empty or multi-scalar strings.
- Scheduler and PortableVM share the same Unicode-scalar behavior and semantic result types (`String`/`Int`).

### Backend coverage

- Rust AOT emits and uses conversion helpers.
- AOT C emits UTF-8 conversion helpers and cap dispatch.
- AOT-C emits the corresponding runtime helpers and cap calls.
- VM direct and PortableVM tests cover ASCII/non-ASCII round trips and invalid inputs.
- C AOT integration covers a GCC-compiled Unicode round trip.

Focused verification passed:

- `cargo test -p crush-vm cap_conv -- --nocapture`
- `cargo test -p crush-vm test_portable_conv_chr_ord -- --nocapture`
- `cargo check -p crush-aot`
- `cargo test -p crush-aot rust_aot -- --nocapture`
- `cargo test -p crush-aot --test integration_c test_c_gcc -- --nocapture`
- `cargo test -p crush-aotc test_emit_conv_caps -- --nocapture`
- `git diff --check`

Workspace-wide rustfmt remains blocked by unrelated formatting drift in the sibling `buckets` repository. Porting the brainfuck ASCII lookup table to use `conv.chr` remains optional and is intentionally out of scope.
