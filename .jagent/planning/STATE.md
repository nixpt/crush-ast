# Planning state — crush-ast

**Updated:** 2026-07-25T03:30:00-05:00  
**Milestone focus:** Post-M2 — JIT Phases 2–7 merged; buckets consumer follow-on (CRUSH-66) Ready; M5 AI-native / M3 debugger next  
**Branch:** `main` (= `origin/main` @ `5fb5bff`)

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
| Docs + CRUSH-66 filing (this session) | in flight — design + ticket, not impl |
| panini `CRUSH-39` / Math.* lowering | separate worktree (`agent/panini-crush/CRUSH-39`) |
| Open GitHub PRs | none at refresh time |

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
| Crates | 35+ (+ xtask) |
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
