# Planning state — crush-ast

**Updated:** 2026-08-21 (header + Active work + PR/crate-count refreshed this pass;
the Delivery snapshot table below is UNCHANGED since 2026-07-25 and was NOT
re-verified this pass — treat those rows as of that date, not current)  
**Milestone focus:** Post-M2 — JIT Phases 2–7 merged; buckets consumer follow-on (CRUSH-66) Ready; M5 AI-native / M3 debugger next  
**Branch:** `main` (= `origin/main` @ `cd0f497`, 2026-08-21)

## Since 2026-07-25 (this session's verified activity — not yet folded into the snapshot below)

- `examples/crush/` grew from a handful of language smoke tests to 40+ programs,
  including a wave of full self-playing programs written by different LLMs given
  the same "learn Crush" prompt (games + two hosted-language interpreters,
  `forth.crush`/`brainfuck.crush`) — story + per-model findings in `nixpt/awesome-crush`.
- Two real compiler bugs found and filed as GitHub issues (#37 trailing-return
  codegen, #38 `revisit-if` parser hang), plus tickets **CRUSH-108..118** minted
  (stdlib source reconciliation, toolchain gaps found building the interpreters,
  and new capability requests: `io.read`/`math.random`/`conv.chr`/`conv.ord`,
  115/46 and 116/47 already merged).
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
| crush-jit (Cranelift) | **partial→expanded** | M2 Phases 2–7 landed via PR #21 (CRUSH-26..38 band on that arc) |
| AOT C / AOT Rust | **shipped** | Polyglot walker→AOT for C/Python/JS/TS/Rust |
| Polyglot + buckets sandbox | **shipped** | CRUSH-20: `sandboxed-polyglot` + `bucket_exec` (bare runtimes only) |
| crush-pkg ↔ buckets | **shipped** | Script capsules via `crush-buckets` path-dep |
| AI-native / async opcodes | **stub** | Still NOP at runtime (CRUSH-1 / CRUSH-32–34) |
| Annotations / crush-index / dejavue | **shipped** | M5 tickets filed CRUSH-27..34 |
| Debugger | **partial** | Breakpoints/REPL; variable inspection open |

## Active work

| Item | Status |
|------|--------|
| Docs + CRUSH-66 filing (2026-07-25 session) | in flight — design + ticket, not impl; not re-checked this pass |
| panini `CRUSH-39` / Math.* lowering | separate worktree (`agent/panini-crush/CRUSH-39`); not re-checked this pass |
| Open GitHub PRs | CRUSH-117/118 (conv.chr/ord, interactive-input demo) still open/unstarted as of 2026-08-21; #45-49 merged this session (see "Since 2026-07-25" above) |

## Buckets consumer reality

| Crate | Path-dep | Role |
|-------|----------|------|
| `crush-vm` | `crush-buckets` @ `../../../buckets`, feature `sandboxed-polyglot` | CRUSH-20 EXEC_LANG sandbox |
| `crush-pkg` | same alias | pinned sandboxed script toolchains |
| `crush-bucketspike` | throwaway; **broken absolute path** | spike only — ignore |

**CRUSH-66 Ready:** wire `@lang[pypi:…]` / `@lang[npm:…]` through existing
`resolve_multi` once [buckets#4](https://github.com/nixpt/buckets/pull/4)
(BUCKETS-15) is on buckets `main`. Design: `docs/design/lang-deps-pypi-npm.md`.

## Blockers

| ID | Blocker | Unblock |
|----|---------|---------|
| B1 | CRUSH-66 needs BUCKETS-15 on sibling `buckets` checkout | Merge buckets#4 |
| B2 | (soft) dejavue was stale vs main through 2026-07-15 | refreshed this session |

## Metrics (indicative — re-measure before claiming)

| Metric | Value |
|--------|--------|
| Crates | 43 (+ xtask) — see "Since 2026-07-25" above |
| `crush-vm` tests (default) | ~128 (per CRUSH-20 verify note) |
| Walker frontends | 9+ (Java skeleton CRUSH-37 Commit 1 on main) |
| AI opcodes executable | 0 |
| Release CI on main | **green** at `5fb5bff` (2026-07-25) |

## Next 3 (suggested)

1. Merge buckets#4 → implement [CRUSH-66](./tickets/CRUSH-66-lang-deps-pypi-npm.md)  
2. Land or review panini Math.* fix (CRUSH-39 / CRUSH-65 naming)  
3. Pick M5 start (CRUSH-1 / CRUSH-32 AI opcodes) or M3 debugger variable inspection  

## Memory split

| Concern | Path |
|---------|------|
| *Why* | `.dejavue/` |
| *What / when* | `.jagent/planning/` (this file, ROADMAP, TASKS, tickets) |
| *How* | `.jagent/planning/RULES.md` |
| Buckets lang-deps design | `docs/design/lang-deps-pypi-npm.md` |
