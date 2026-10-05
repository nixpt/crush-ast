# CRUSH-147: salvage + triage of the dirty shared checkout

**Agent:** panini · **Date:** 2026-10-05 · **Foreman session:** s473 · **Status:** done (triage only; nothing ported)

## Salvage

- Branch `salvage/panini/CRUSH-147-20261005-0848`, SHA `c302e83915a1197273db9d9e10daba67d55f41cc` (parent `0d17b54`), pushed.
- 196 files, +9304/-4530.
- The 196 paths in the commit match the checkout's `git status` exactly (`diff` of the two sorted lists is empty).

## Fingerprints (shared checkout `/workspace/projects/crush-ast`, never modified)

| | HEAD | branch | dirty files | `git diff HEAD --binary` sha256[:16] | stashes | staged |
|---|---|---|---|---|---|---|
| foreman, at dispatch | 0d17b54 | main | 195 | `f47a56ee35fed7e5` | 6 | `.dejavue/timeline.jsonl` |
| panini, before salvage | 0d17b54 | main | **196** | `bf7fb02df0a6e7a8` | 6 | same |
| panini, after salvage | 0d17b54 | main | 196 | `bf7fb02df0a6e7a8` | 6 | same |

The dispatch→arrival difference is one file: `.gitignore` gained `.jagent/worktrees/` (SQ-204), written at 03:47:59 on 2026-10-05 by the agent-launch worktree setup for this very dispatch. `agent-live` showed no other agent in the checkout, and the other 195 files all carry mtime 2026-09-17 17:37:37. Before/after salvage are identical, so the salvage did not touch the checkout.

## Finding: the dirty tree is a `cargo fmt --all` run, not feature work

All 195 pre-existing files share one mtime instant (2026-09-17 17:37:37.26x), the signature of a single bulk write. On a clean `0d17b54` worktree, `rustfmt --edition 2024` over the dirty `.rs` files reproduces S byte-for-byte for 188 of 193. The other 5 differ by a few lines of rustfmt-version nuance (comment alignment, import ordering, trailing `return ...;`), all formatting-only. **The salvage holds no functional code change.**

Consequence for the suspects: the "dispatches claimed done with zero commits" (CRUSH-73 / 19→72 / 11) did **not** leave their work in this tree. Their code is gone, was never written here, or lives in another worktree. This triage cannot say which. No attribution to a specific dispatch is evidence-backed. Candidates are guesses, and the timestamp/stash evidence is only consistent with an unattributed fmt run on 2026-09-17. The 6 stashes all predate it (2026-06-27 … 2026-07-24) and are unrelated to it.

## Triage (196 paths, all accounted for)

| class | paths | what |
|---|---|---|
| landed | 3 | `crates/crush-cson/src/lib.rs`, `parser.rs`: already fmt-clean on `origin/main`; `.gitignore`: main already has the `.jagent/worktrees/` line |
| superseded | 1 | `Cargo.lock`: 0.3.0→0.3.7 version sync; main is now 0.3.8 and its lock moved on |
| noise (fmt output) | 191 | `.rs` files that are rustfmt-reproducible and still not fmt-clean on `origin/main` (main has 197 unformatted `.rs` files in total). Regenerable with one command; 147 of these were untouched on main, 44 were also edited on main |
| noise (timeline) | 1 | `.dejavue/timeline.jsonl`: two git-hook lines for commit 0d17b54, staged |
| unique | **0** | nothing here exists nowhere else |

Of the 191 fmt files, 49 paths in the dirty set were also edited by main since P (the #79–#87 stack and others), so a patch of S would conflict there; regenerating is cleaner.

## Recommendations

- **Discard the dirty tree** once foreman is satisfied: `git restore --staged . && git checkout -- .` in the shared checkout (foreman's call, not done here). Nothing in it needs porting.
- **Optional follow-up ticket**: a mechanical `cargo fmt --all` commit on main plus a CI `cargo fmt --check` gate. The repo has no fmt gate today, and workspace formatting drift (197 files) is a recurring blocker in `.dejavue/handoff.md` ("workspace-wide rustfmt remains blocked by unrelated drift"). Do it by running fmt on a fresh worktree, not by porting S. Note rustfmt version drift: 5 files differed between the rustfmt used on 2026-09-17 and 1.9.0 today. Pin a toolchain/`rustfmt.toml` first.
- **Chase the missing CRUSH-73/19/11 work separately** (other worktrees, `salvage/*`, agent branches); this salvage cannot recover it.

## Not done

- `cargo check --workspace` on S: skipped. S is formatting-only, so compile state equals `0d17b54`, and the box has been prone to resource limits.
- No code ported.
