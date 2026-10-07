# Migration inventory — what is left to move into crush-ast

**Ticket:** CRUSH-150 (derby relay, phase 1 — scout) · **Date:** 2026-10-07 ·
**Baseline:** crush-ast `origin/main` `4034d92` (tag `v0.3.9`) · exosphere `origin/main` `c7ee194c`

This is the baton for phase 2: a builder should be able to pick up a CRUSH-151+ ticket from §5
and start without re-reading the sources. Method: read-only (`git show` / `git ls-tree` /
`git grep` against each source's tracked tree); nothing was built or run unless a row says so.
LOC = `wc -l` of tracked source files (tests counted separately where it matters).

Paths are repo-relative. `ast:` = crush-ast, `exo:` = exosphere, `anc:<repo>/` = the
pre-crush-ast lineage repos (`nixpt/crush` and siblings, all archived/dormant), `caps:` =
`nixpt/crush-capsules`, `sq:` = `nixpt/squeeze`.

## 0. Prior art this builds on (read, not redone)

| Doc | What it settled | Still true at `4034d92`? |
|---|---|---|
| `.jagent/planning/research/2026-09-25-CRUSH-55-delta-inventory.md` | exo front end / CAST / casm / nanovm / vm-runtime / runtimes / crush-common vs ast, per module. Headline: **exo's fork ⊆ ast** for language surface; real ports are engine behaviour behind ast stubs | Yes for the verdicts. Its slice 1 has **landed** (see below) |
| `.jagent/planning/tickets/CRUSH-122.md` | exo `core/base/stdlib` families + nanovm SBL ported (steps 2–4, #61) | Yes — re-verified in §2 |
| `.jagent/planning/tickets/CRUSH-56/57/88–97/108` | archive-zip "137 caps, 103 clean / 46 mock" restoration plan | **Mostly superseded** by CRUSH-122 — see §2.5 |

CRUSH-55 claims re-checked against today's `ast:`:

- Walker binary-name mismatch (`crush_lang_go/zig/wasm`) — **fixed** in #62 (`90fc53f`,
  "go/zig/wasm walker binaries were never found; add PATH fallback"); the `[[bin]]` names are now
  `crush_lang_go`/`crush_lang_zig` (`ast:crates/crush-lang-go/Cargo.toml:9`, `crush-lang-zig/Cargo.toml:10`).
- `SubprocessWalker::resolve_binary` PATH + well-known-dir fallback (CRUSH-55 PORT #3) — **ported**
  (`ast:crates/crush-frontend/src/language_walkers.rs:179`, test `:522`).
- Orphaned `ast:crates/crush-vm/src/polyglot/` (2.2k uncompiled lines) — **deleted** (no longer in
  `ast:crates/crush-vm/src/`).
- AI tool-chain engine + arg plumbing (CRUSH-55 PORT #1) — **still open**: `ai_native.*` caps still
  declare `argc: Some(0)` (`ast:crates/crush-lang-sdk/src/ai_native.rs:87-103`).
- Debugger semantics (CRUSH-55 PORT #2) — **still open**: `crush-debugger` still documents its
  hook points as deliberate `todo!()` (`ast:crates/crush-debugger/src/lib.rs:27`).
- CRUSH-113 — **still open**: `stdlib` is not in `crush-lang-sdk`'s `default` features
  (`ast:crates/crush-lang-sdk/Cargo.toml` `[features]`), and `crush-run --stdlib` without it only
  warns (`ast:crates/crush-lang-sdk/src/bin/crush-run.rs:368-370`).

<!-- sections 1-6 follow -->
