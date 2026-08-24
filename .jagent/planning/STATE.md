# Planning state — crush-ast

**Updated:** 2026-08-24 (v0.3.6 backlog reconciliation: CRUSH-80, CRUSH-115,
CRUSH-116, CRUSH-117, CRUSH-119, and CRUSH-120 marked complete where the merge
history proves it)
**Milestone focus:** M2 implementation substantially landed (Phases 1-5); M2 closure gates remain in conformance/optimization/AOT work; CRUSH-66 done; M3 debugger / M5 AI-native next
**Branch:** `main` (= `origin/main` @ `d755781` / tag `v0.3.6`, 2026-08-24)

## Since 2026-07-25 (this session's verified activity — not yet folded into the snapshot below)

- `examples/crush/` grew from a handful of language smoke tests to 40+ programs,
  including a wave of full self-playing programs written by different LLMs given
  the same "learn Crush" prompt (games + two hosted-language interpreters,
  `forth.crush`/`brainfuck.crush`) — story + per-model findings in `nixpt/awesome-crush`.
- Two real compiler bugs found and filed as GitHub issues (#37 trailing-return
  codegen, #38 `revisit-if` parser hang), plus tickets **CRUSH-108..118** minted
  (stdlib source reconciliation, toolchain gaps found building the interpreters,
  and new capability requests: `io.read`/`math.random`/`conv.chr`/`conv.ord`;
  CRUSH-115/116/117 are now merged, while CRUSH-118 remains the interactive
  demo/proof ticket).
- **`crates-publish-sync`'s systemd timer had been failing 52+ consecutive runs**
  (crates.io 400: missing description/license on `crush-ptx`) — silently stalling
  ALL remaining crates.io publishes behind it. Fixed (PR #48); verified via
  `cargo publish --dry-run`.
- `scripts/bump-version.sh` + `.github/workflows/release.yml` installed (PR #49) —
  auto-bumps + tags on every push to main from here on. Note: last git tag is
  `v0.2.0` but Cargo.toml/crates.io are already at `0.3.0` — `v0.3.0` itself was
  never tagged; flagged, not silently backfilled.
- Crate count: **43** in `crates/` (was "35+" below) — 7 previously undocumented
  in this file's own metrics and in `README.md`'s repo structure (now fixed in
  README): `crush-aotc`, `crush-ptx`, `crush-vm-capi`, `crush-vm-py`, `crush-web`,
  `crush-lang-java`, `crush-bucketspike` (the last deliberately excluded from
  README's list — internal spike, not release surface).

## Delivery snapshot

| Track | Status | Notes |
|-------|--------|--------|
| Core compiler pipeline | **shipped** | Parser → CAST → Semantics → Optimizer → Compiler → CASM |
| CVM1 PortableVm | **shipped** | 40+ opcodes, debugger-aware |
| FastVM | **shipped** | 84 FastOp instructions |
| crush-jit (Cranelift) | **substantially implemented** | M2 Phases 1-5 landed; full conformance, optimization, and AOT-from-JIT closure remain |
| AOT C / AOT Rust | **shipped** | Polyglot walker→AOT for C/Python/JS/TS/Rust |
| Polyglot + buckets sandbox | **shipped** | CRUSH-20: `sandboxed-polyglot` + `bucket_exec` (bare runtimes only) |
| crush-pkg ↔ buckets | **shipped** | Script capsules via `crush-buckets` path-dep |
| AI-native / async opcodes | **partial / stubbed** | Annotation/index work is partly landed; VM-side AI opcode execution and spawn/await/yield remain open (CRUSH-1 / CRUSH-32–34) |
| Annotations / crush-index / dejavue | **partial / active** | Annotation/index foundations and M5 tickets exist; full M5 done condition is not met |
| Debugger | **partial** | Breakpoints/REPL; variable inspection open |

## Active work

| Item | Status |
|------|--------|
| CRUSH-66 `@lang[pypi:/npm:]` deps | **done 2026-08-01** — BUCKETS-15 merged; lexer, dependency validation, sandbox provisioning, and live proof landed. |
| panini `CRUSH-39` / Math.* lowering | separate worktree (`agent/panini-crush/CRUSH-39`); not re-checked this pass |
| Open capability follow-up | CRUSH-118 (interactive-input demo) remains open; CRUSH-115 `io.read`, CRUSH-116 RNG, and CRUSH-117 `conv.chr`/`conv.ord` are merged. |
| CRUSH-80 casm dead code | **done in v0.3.6** — `ecasm.rs` and orphaned `CachedProgram`/`to_cached` remnants deleted; CRUSH-83 remains the real cache design. |
| CRUSH-119 FastVM array/loop parity | **done in v0.3.6**; native array mutation contracts and compiled range/array-loop regressions pass. |
| CRUSH-120 optimizer self-reference | **done in v0.3.6**; assignment invalidation now precedes RHS constant propagation, preserving loop-carried accumulators. |

## Buckets consumer reality

| Crate | Path-dep | Role |
|-------|----------|------|
| `crush-vm` | `crush-buckets` @ `../../../buckets`, feature `sandboxed-polyglot` | CRUSH-20 EXEC_LANG sandbox |
| `crush-pkg` | same alias | pinned sandboxed script toolchains |
| `crush-bucketspike` | throwaway; **broken absolute path** | spike only — ignore |

**CRUSH-66 Done:** `@lang[pypi:…]` / `@lang[npm:…]` dependency resolution
is wired through `resolve_multi`, with BUCKETS-15 merged and the sandbox path
verified live. Design: `docs/design/lang-deps-pypi-npm.md`.

## Blockers

| ID | Blocker | Unblock |
|----|---------|---------|
| B1 | (soft) dejavue was stale vs main through 2026-07-15 | refreshed this session |

## Metrics (indicative — re-measure before claiming)

| Metric | Value |
|--------|--------|
| Crates | 43 (+ xtask) — see "Since 2026-07-25" above |
| `crush-vm` tests (default) | ~128 (per CRUSH-20 verify note) |
| Walker frontends | 9+ (Java skeleton CRUSH-37 Commit 1 on main) |
| AI opcodes executable | 0 |
| Release CI on main | **green** at `5fb5bff` (2026-07-25) |

## Next 3 (suggested)

1. Build the CRUSH-118 interactive `io.read` demo now that CRUSH-115 is merged.
2. Land or review panini Math.* fix (CRUSH-39 / CRUSH-65 naming).
3. Pick M5 start (CRUSH-1 / CRUSH-32 AI opcodes) or M3 debugger variable inspection.

## Memory split

| Concern | Path |
|---------|------|
| *Why* | `.dejavue/` |
| *What / when* | `.jagent/planning/` (this file, ROADMAP, TASKS, tickets) |
| *How* | `.jagent/planning/RULES.md` |
| Buckets lang-deps design | `docs/design/lang-deps-pypi-npm.md` |
