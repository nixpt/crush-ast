# Threads (deferred work + parking lot)


## a-4b-equivalent: workspace regression --  fails in  (rustls missing) -- OPEN

- **Date:** 2026-06-18
- **Status:** OPEN -- INVESTIGATION captured; no fix proposed (NOT YET ROOT-CAUSED).
- **Findings (this session):**
  - `cargo check --workspace` from `/workspace/projects/crush-ast` fails with a rustls-related error inside `crush-net`. The failure is reproducible.
  - `crates/crush-net/Cargo.toml` declares rustls as a dependency (per dependency scan).
  - `crates/crush-net/src` imports at least one symbol from `rustls` (per grep).
  - The error message + location is captured in `/tmp/cargo-check-workspace.log` (above).```
- **Failure verbatim (filtered to essential lines):**
```
error[E0433]: cannot find module or crate `rustls` in this scope
 --> crates/crush-net/src/caps.rs:9:5
error: could not compile `crush-net` (lib) due to 1 previous error
```
- **Pre-existing nature:** the issue was surfaced during `cargo check --workspace` regression checking after the xtask hardening pass. It is NOT caused by any of the 4 splices landed in this branch.
- **Why this ticket exists:** the user wants the workspace-level gate to pass cleanly on a follow-up `cargo check --workspace` run before any further xtask edits, so the issue must be catalogued IN dejavue (not fixed in this turn -- per scope discipline of "don't surprise the user").
- **Recommended next-step (NOT opened here):**
  1. Determine whether the rustls dep in `crush-net/Cargo.toml` is missing a feature flag (e.g. `features = ["tls12"]`) or has a wrong version pin.
  2. If the failure is referring to `rustls` as a transitive dep brought in by another crate (e.g. `reqwest`, `hyper`), pin `rustls` to a specific version at workspace level.
  3. Verify with `cargo check --workspace` post-fix.
- **Forward-tickets already captured:** all 3 of the original hardening pass items are now landed (the empty-marker guard, VARIANTS threading + byte-match + integration test, and the extended docstring rewrite).

## Resolution (2026-06-19F, FINAL -- (c)-modified landed; source-level followup flagged)
- **Status**: CLOSED/RESOLVED at Cargo.toml level. Source-level followup ticket OPEN.
- **Final Cargo.toml form (this iteration)**: rustls dep has `optional = true, default-features = false, features = [logging, std, tls12, ring]`. webpki-roots dep has `optional = true`. [features] block has `tls = ["dep:rustls", "dep:webpki-roots"]` AND `default = ["tls"]`. Atomically written via `os.replace` from /tmp/atomic_fix_cargo.py.
- **Verification**: cargo check --workspace GREEN. cargo check -p xtask GREEN. cargo test -p xtask 8 passed; 0 failed (no regression on the variant-threading + byte-match + walker-integration test suite). cargo tree -p crush-net shows rustls 0.23 + webpki-roots 0.26 both linked via the activated default `tls` feature.
- **Multi-iteration debug history**:
    - Phase 1 (drop optional = true alone): broke Cargo due to `dep:rustls` requiring optional scaffolding.
    - Phase 2 (drop tls feature entry alone): GREEN but lost downstream opt-out.
    - Phase 3 (textbook c-modified): re-add optional + restore [features] + add default = [tls] -- corrupted on first land due to shell-quoting + heredoc pitfall; recovered via the heredoc-file python with explicit string.replace + idempotent re.sub.
- **IMPORTANT FEEDBACK FROM CODE-REVIEWER (must-mitigate, followup ticket)**: The (c)-modified form preserves API-level opt-out (the [features] toggle exists with the right shape), but source-level opt-out is **NOT** in place. `crates/crush-net/src/{caps, tcp}.rs` reference rustls+webpki-roots UNCONDITIONALLY without `#[cfg(feature = "tls")]` gating. Concretely: `cargo check -p crush-net --no-default-features` returns E0433 cannot find module rustls. This proves the source needs feature-gating to make downstream `default-features = false` opt-out actually work. Future ticket: gate rustls + webpki-roots usage in src/*.rs behind `#[cfg(feature = "tls")]`.
- **Add `--no-default-features` to CI** as a separate followup: once source-level gating lands, `cargo check --workspace --no-default-features` for crush-net (or per-crate: `cargo check -p crush-net --no-default-features`) is a meaningful regression gate. Until source-level gating lands, the `--no-default-features` form will continue to E0433 fail, so this CI gate is dual-purpose: forces the source-level fix in addition to guarding future regression.
- **Other future tickets (out of scope)**:
    - Bump rustls 0.23 to 0.24+ (current at time of writing).
    - Bump webpki-roots 0.26 to 0.30+ (current at time of writing).
- **Idempotency note**: /tmp/atomic_fix_cargo.py is idempotent -- re-running it on the now-correct file is a no-op (all three steps check before replacing).
- **Captured in timeline.jsonl as workspace_rustls_resolved_v3_final**.

## Thread: workspace-wide `dep:<name>` audit (+ fix recommendation for followup tickets)

- **Date:** 2026-06-19
- **Motivation**: the (c)-modified rustls pivot was tripped up by Cargo's `dep:<name>` syntax requiring the dep to be `optional = true`. A future contributor could be ambushed by the same constraint elsewhere. This thread catalogs every `dep:<name>` reference in the workspace, classifies each as OK vs AT-RISK, and recommends follow-up tickets.
- **Method**: scanned every `Cargo.toml` in the workspace (excluding `target/`, `.git/`, `node_modules/`). Parsed each via `tomllib`. Walked every `[features]` block entry, classified each `dep:<name>` reference against the SAME crate's `[dependencies]` / `[dev-dependencies]` / `[build-dependencies]` (and the workspace-shared `[workspace.dependencies]`).
- **Classifier rules**:
  - `dep:<name>` AND `<name>` has `optional = true` somewhere in deps: **OK**
  - `dep:<name>?/<sub>`: **SAFE-QUERY** (the `?` syntax auto-resolves whether or not `<name>` is optional; no constraint violation)
  - `dep:<name>` WITHOUT `?` AND `<name>` is NOT optional (in any dep section): **AT-RISK** (Cargo manifest error: `feature X includes dep:Y, but Y is not an optional dependency`)
  - `dep:<name>` referring to a name that doesn't exist anywhere in deps: **AT-RISK** (Cargo error: unknown dep)
- **Audit script**: `/tmp/audit_dep_refs.py` (idempotent; passes workspace root via env-like parameter; uses `tomllib` for parse correctness so any malformed TOML surfaces immediately).


### Findings (per crate)

- `Cargo.toml`: no `[features]` block
- `crates/c_walker/Cargo.toml`: no `[features]` block
- `crates/casm/Cargo.toml`: no `[features]` block
- `crates/cli/Cargo.toml`: no `[features]` block
- **`crates/crush-cast/Cargo.toml`** — features keys: ['default', 'ts-export']
    - AT-RISK: none
    - OK (dep:<name> with optional=true):
        - feature `'ts-export'` -> `'dep:ts-rs'` (dep='ts-rs', section=dependencies)

- `crates/crush-errors/Cargo.toml`: no `[features]` block
- `crates/crush-frontend/Cargo.toml`: no `[features]` block
- `crates/crush-index/Cargo.toml`: no `[features]` block
- `crates/crush-installer/Cargo.toml`: no `[features]` block
- `crates/crush-lang-bash/Cargo.toml`: no `[features]` block
- `crates/crush-lang-js/Cargo.toml`: empty `[features]` (no entries)
- `crates/crush-lang-python/Cargo.toml`: no `[features]` block
- `crates/crush-lang-rust/Cargo.toml`: no `[features]` block
- **`crates/crush-lang-sdk/Cargo.toml`** — features keys: ['db', 'default', 'graphics', 'net', 'repl-helper', 'stdlib']
    - AT-RISK: none
    - OK (dep:<name> with optional=true):
        - feature `'net'` -> `'dep:ureq'` (dep='ureq', section=dependencies)
        - feature `'db'` -> `'dep:rusqlite'` (dep='rusqlite', section=dependencies)
        - feature `'stdlib'` -> `'dep:regex'` (dep='regex', section=dependencies)
        - feature `'repl-helper'` -> `'dep:rustyline'` (dep='rustyline', section=dependencies)

- `crates/crush-lang-zsh/Cargo.toml`: no `[features]` block
- **`crates/crush-net/Cargo.toml`** — features keys: ['default', 'tls']
    - AT-RISK: none
    - OK (dep:<name> with optional=true):
        - feature `'tls'` -> `'dep:rustls'` (dep='rustls', section=dependencies)
        - feature `'tls'` -> `'dep:webpki-roots'` (dep='webpki-roots', section=dependencies)

- `crates/crush-pkg/Cargo.toml`: no `[features]` block
- `crates/crush-python/Cargo.toml`: no `[features]` block
- `crates/crush-vm/Cargo.toml`: no `[features]` block
- `crates/go_walker/Cargo.toml`: no `[features]` block
- `crates/tree-sitter-crush/Cargo.toml`: no `[features]` block
- `crates/walker-core/Cargo.toml`: no `[features]` block
- `crates/wasm_walker/Cargo.toml`: no `[features]` block
- `crates/zig_walker/Cargo.toml`: no `[features]` block
- `xtask/Cargo.toml`: no `[features]` block

### Workspace totals

- Cargo.toml files scanned: **25**
- Files with `[features]` blocks: **4**
- AT-RISK references (require dep to be optional, currently NOT): **0**
- OK references (dep:<name> with optional=true): **7**
- SAFE-QUERY references (dep:?/ syntax): **0**

### Follow-up tickets (if any AT-RISK found)

- None. All `dep:<name>` references in the workspace are correctly scaffolded.

### Tooling note

- `/tmp/audit_dep_refs.py` is idempotent + systematic. Recommend committing it to `xtask/` as a workspace lint (e.g., `cargo run -p xtask audit-deps`) so any future PR adding a `dep:<name>` reference without `optional = true` fails CI before merge.

## Thread: dejavue lint-dejavue future-preventive landed (2026-06-19)

- **Status:** LANDED; available today via `cargo run -p xtask --bin lint-dejavue [.dejavue/timeline.jsonl]`.
- **Motivation:** the (c)-modified rustls workspace-build fix required multi-iteration debugging; an implicit-via-chronology correction between Phase-1 and Phase-2 was missed because nothing in the workspace flagged it on write. Per code-reviewer point #2: any RESOLVED-named event in `timeline.jsonl` that lacks an explicit supersession marker within 1 hour is a future-preventive failure mode.
- **Implementation:**
    - New file `xtask/src/lint_dejavue.rs` (~340 lines, std-only, no new deps).
    - `xtask/Cargo.toml` adds `[[bin]] name = "lint-dejavue" path = "src/lint_dejavue.rs"`. No new crate-level deps.
    - The pre-existing `xtask` audit binary is unchanged; lint-dejavue is a sibling binary in the same `xtask` package.
- **Lint semantics:**
    - Pattern detected: event name contains `resolved` AND does NOT end in `_superseded` AND does NOT contain `_final` AND does NOT start with `supersedes_`.
    - Dual-form acceptance: both `<event>_superseded` (current/target convention) and `supersedes_<event>` (legacy convention) are valid supersession markers. The acceptance was needed after empirical verification that the existing `.dejavue/timeline.jsonl` uses both forms (Phase-1 markers as `supersedes_*`, V2+ markers as `*_superseded`).
    - Window: 1 hour (3600 sec). Configurable via `WINDOW_SECONDS` constant; future-friendly.
    - Exit code 0 on clean; non-zero with human-readable violation report on stderr otherwise.
- **Tests:** 13 tests pass (12 unit + 1 real-timeline smoke). The smoke test resolves `.dejavue/timeline.jsonl` via `CARGO_MANIFEST_DIR` parent so it works from any cargo-test working directory.
- **Verification cascade verified green at landing time:** `cargo build -p xtask` clean; `cargo test -p xtask --bin lint-dejavue --verbose` 13 passed; `cargo test -p xtask --bin xtask` regression 8 audit tests still pass; positive smoke on real `.dejavue/timeline.jsonl` exits 0; negative smoke on a synthetic 4-event bad timeline reports 2 violations exits non-zero.
- **Code-reviewer-minimax-m3 follow-up issues** (not auto-applied per scope discipline, captured here for next-pass application):
    1. Replace the hand-rolled `extract_field` with a `regex::Regex` call (xtask already has regex = "1" in deps). Avoids fragility under escaped quotes; matches existing codebase style.
    2. Make the `current_real_timeline_passes` smoke test early-return impossible by `panic!`ing when the dejavue timeline is missing -- the silent `if !path.exists() { return; }` early-return is the opposite of future-preventive.
    3. Optional: add a `--strict-suffix` CLI flag so workspaces can opt into rejecting the legacy `supersedes_*` form when they prefer the suffix-only convention.
- **Future-ticket priority**: docs/CI integration. The lint has not yet been wired into the workspace CI matrix -- a `lint-dejavue` step in `.github/workflows/ci.yml` (or equivalent) is the next natural followup so future contributors cant accidentally land implicit-via-chronology corrections without the lint flagging them at PR time.

## Thread: dejavue lint-dejavue regex-followup landed (2026-06-19)

- **Status:** LANDED end-to-end. `cargo build -p xtask` clean; `cargo test --bin lint-dejavue --verbose` 19 passed; `cargo test --bin xtask` regression 8 passed; positive smoke on real timeline exits 0 with OK message; negative smoke on a synthetic 2-event bad timeline prints `FAIL: 2 violation(s):` and returns ExitCode 1.
- **What changed this round:** the hand-rolled `extract_field` was replaced with a `regex::Regex::captures_iter`-based extractor whose value pattern `((?:[^"\\]|\\.)*)` correctly captures escaped `\"`/`\\`/`\n`/`\t` rather than truncating at the first literal `"`. A bug surfaced during the spawn: the first attempt used `captures()` (single match only) which silently returned None whenever the requested key wasn't the first field in the JSONL line; this caused 4 test failures + the negative-smoke to print OK on a synthetic bad timeline. Fixed by switching to `captures_iter()` and iterating to find the requested key. The hand-rolled RFC3339 parser (`time_to_unix_seconds`) was upgraded to handle explicit offsets `Z`/`+HH:MM`/`-HH:MM` (verified by `time_to_unix_seconds_offset_correctness` test which asserts 4 different offset representations of the same instant produce identical Unix seconds).
- **Reviewer-pending concerns (NOT auto-applied, captured here as future tickets):**
  - Fractional seconds not handled in `time_to_unix_seconds` (`.123` formatter would currently return None and silently upgrade to "flagged violation"). Future-preventive ticket.
  - `current_real_timeline_passes` smoke test still silently `eprintln!`+return on missing timeline instead of `panic!`. Per prior review point #4.
  - No test asserts `violations.sort_by` ordering is chronological. Silent regression mode for inverted sort.
- **Tradeoff:** the lint is now functionally complete. Any of those three issues being hit would yield a *misclassification* (false positive or false negative) rather than a *crash*. They're tracked here so future iterations don't re-litigate them.

## Thread: lint-dejavue smoke-missing now panics (2026-06-19)

- **Status:** LANDED.  `xtask/src/lint_dejavue.rs::current_real_timeline_passes` no longer silently `eprintln!`+`return` on missing `.dejavue/timeline.jsonl`; now `panic!("smoke test: dejavue timeline missing at {}", p.display())`.  Verification: cargo build clean; 19/19 + 8/8 tests pass; positive smoke exit 0; negative smoke (synthetic 2-event bad timeline) prints 2 violations via FAIL arm and exits non-zero (verified via PIPESTATUS).
- **Rationale:** the lint is future-preventive -- its job is to fail loud on implicit-via-chronology corrections.  A silent skip in CI standalone-build scenarios defeats that promise entirely (the regression would go undetected).  Switching to panic!() promotes missing-timeline from a "harmless skip" to a "build is broken".

## Thread: lint-dejavue wired into CI (2026-06-19, FINAL after YAML repair)

- **Status:** LANDED in `/workspace/projects/crush-ast/.github/workflows/ci.yml`. New `lint-dejavue` job runs in parallel with check/test/wasm; runs `cargo run -p xtask --bin lint-dejavue` after minimum template (checkout + exosphere clone + rustup show + Swatinem/rust-cache v2 + lint run). Verified locally: lint exits 0 with OK on real timeline; lint exits 1 with FAIL on synthetic bad timeline.
- **Repair note:** an earlier splice attempt corrupted the YAML via over-aggressive regex cleanup of orphan `with:` blocks (after deleting `fetch-depth: 0`). The current state was restored from `/tmp/ci.yml.pre-lint-step.bak` and the job was re-appended with code-reviewer followup #1 (no fetch-depth: 0) already in place. Final YAML parses cleanly; verification confirmed.
- **Code-reviewer followups (deferred to future tickets):**
  1. Explicit Rust toolchain pinning via `dtolnay/rust-toolchain@stable` + a workspace `rust-toolchain.toml`. The lint-dejavue job (and check/test/wasm too) all rely on `rustup show` rather than an explicit pin. If rustup's ubuntu-latest default-toolchain drifts, all jobs break at once.
  2. Document in `CONTRIBUTING.md` that any new `RESOLVED`-named event will cause CI to fail loudly. Intended future-preventive behavior but worth telegraphing so PR authors aren't surprised.
