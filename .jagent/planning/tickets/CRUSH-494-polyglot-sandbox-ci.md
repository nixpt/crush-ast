# CRUSH-494 — Run the sandboxed-polyglot tests, including the live bwrap ones, in a CI job

| Field | Value |
|-------|-------|
| **ID** | CRUSH-494 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | 0.12 (freeze prep) |
| **Issue** | [#494](https://github.com/nixpt/crush-ast/issues/494) |
| **Size** | M |
| **Dependencies** | CRUSH-460, CRUSH-459 |

## Problem

No workflow builds or tests crush-vm with `sandboxed-polyglot` (grep of .github/workflows finds nothing), so the sandbox path never compiles or runs in CI. The three live end-to-end tests are #[ignore]d for network and bwrap (scheduler.rs:3063, 3160, 3247); the other sandbox tests (e.g. 3317, 3359) are cfg-gated and never built in CI. The interpreter is provisioned through buckets/pkgx (bucket_exec.rs:36-43), so a no-network sandboxed run is impossible on a cold cache. All CI jobs run on ubuntu-latest, where AppArmor blocks unprivileged user namespaces by default, so bwrap fails unless the job relaxes kernel.apparmor_restrict_unprivileged_userns (CRUSH-236 criterion 3).

## Evidence (at origin/main 7ecd2515)

- grep sandboxed-polyglot .github/workflows -> no hits
- crates/crush-vm/src/scheduler.rs:3063,3160,3247, 3317,3359
- crates/crush-vm/src/bucket_exec.rs:36-43
- crates/crush-vm/Cargo.toml:71 (buckets path dep; CI checks out nixpt/buckets, ci.yml:23-25)
- .github/workflows/ci.yml runs-on: ubuntu-latest throughout

## Goal

One new ci.yml job: checks out buckets as the other jobs do, installs bubblewrap, sets kernel.apparmor_restrict_unprivileged_userns=0, prints `bwrap --version` and runs a smoke `bwrap --ro-bind / / true` (the job fails if the smoke fails). It caches a fixed BUCKETS_CACHE_DIR with actions/cache, then runs `cargo test -p crush-vm --features sandboxed-polyglot --lib -- --include-ignored exec_lang_` so both gated and live tests run. The live bash test either keeps its forced fresh cache (network cost accepted) or gains an env override to use the cached dir. Own CARGO_TARGET_DIR, debug builds only.

## Non-goals

- A fully offline sandboxed run
- Release builds
- aarch64 CI (CRUSH-CI-AARCH64)
- Making the job required before it has a week of green runs

## Files

- .github/workflows/ci.yml
- crates/crush-vm/src/scheduler.rs

## Done when

- a CI run on the PR shows the new job green, with exec_lang_runs_through_a_real_buckets_sandbox_not_the_host_bash, the pypi test and the npm test listed as passed, not ignored
- the job log prints the bwrap version and the smoke-test result
- removing the bubblewrap install step in a throwaway commit makes the job fail at the smoke step, never skip silently
- cargo +1.100.0 clippy -p crush-vm --features sandboxed-polyglot --all-targets -- -D warnings clean if scheduler.rs tests change

## Depends on

- CRUSH-460
- CRUSH-459

## Refs

- CRUSH-236
- CRUSH-20
- CRUSH-66

Rulings: /workspace/workspace-meta/plans/2026-11-13-crush-ast-rulings.md (captain, 2026-11-13). Drafted by the ticket-draft workflow (s518), checked by Opus, final cut by Fable; captain checkpoint 2 on 2026-11-13. CI clippy is pinned to 1.100.0; engine tickets need a test per engine they claim; JIT tests assert the JIT compiled.
