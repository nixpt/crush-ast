# CRUSH-231 — CI: the `1.85` toolchain pin is dead, and 24 of 42 crates never run their tests

| Field | Value |
|-------|-------|
| **ID** | CRUSH-231 |
| **Priority** | P1 |
| **Status** | Done |
| **Phase** | M2 |
| **Assignee** | naka |
| **Dependencies** | none |
| **Estimated effort** | S |
| **Filed by** | kai (foreman) — s476, 2026-10-09, on `db3e8e1`; toolchain finding from the cloud agent on PR #126 |

## Problem 1 — the toolchain pin is dead and contradicts MSRV

Eight jobs in `.github/workflows/ci.yml` run `dtolnay/rust-toolchain@stable` with
`toolchain: "1.85"`. That pin never takes effect:

- `rust-toolchain.toml` pins `channel = "stable"`. rustup gives the toolchain file
  precedence over the default that the action sets, so cargo actually runs on stable.
- `Cargo.toml` declares `rust-version = "1.95.0"`. Under rustc 1.85, cargo would refuse
  to build the workspace. CI is green, so it isn't building on 1.85.

So each job downloads a 1.85 toolchain it never uses. Anyone reading the YAML assumes
CI checks a 1.85 MSRV. Nothing checks the declared 1.95 MSRV either.

## Problem 2 — most crates are compile-only in CI

`Test (workspace)` runs `cargo test --workspace --no-run`. It compiles every test
target but runs none of them. Tests only execute in the targeted jobs:
- `Test (core)`
- `Test (walkers)`
- `Test (sdk)`

Those jobs cover 18 of the 42 members. The other 24 never run a test in CI:

`crush-aot crush-aotc crush-caison crush-cson crush-debugger crush-diagnostics
crush-errors crush-ffi crush-index crush-installer crush-lang-custom crush-lang-java
crush-lang-wasm crush-lint crush-macros crush-net crush-plugin-example crush-ptx
crush-python crush-tui crush-vm-capi crush-walker tree-sitter-crush xtask`

Concretely, PR #126 added 13 crush-aot tests:
- `names::tests`
- `tests/backend_agreement.rs`
- `tests/examples_parity.rs`, which builds every example with rustc and gcc and
  diffs the output against the VM

All 13 showed CI green while never executing. They passed when run by hand on
[main] (s476).

## Fix

1. Drop `toolchain: "1.85"` from every job and let `rust-toolchain.toml` decide.
   Add one `msrv` job that runs `cargo +1.95.0 check --workspace`, matching
   `rust-version`. Or, if MSRV isn't a promise, delete `rust-version` instead.
2. Run the tests of the untested crates. Either:
   - add a `Test (rest)` job with explicit `-p` lists; or
   - make `Test (workspace)` a real run with `--exclude` for the crates that
     genuinely can't run on the runner, each exclude with a comment.
3. **crush-aot is covered by PR #128** (CRUSH-227), which adds a `Test (aot)` job that runs its suite and reruns the C tests under `CRUSH_GC_STRESS=1`. Once #128 merges, 23 crates remain uncovered. #128's job copies the dead `toolchain: "1.85"` line too, so step 1 covers it as well. For reference, crush-aot needs `gcc` on the runner. `ubuntu-latest` already has it. Its tests
   skip the C backend when `gcc` is missing (`cc_available`). Keep
   `CRUSH_AOT_PARITY_JOBS=2` on CI, since `examples_parity` builds in parallel and
   takes about 55 s locally with 2 jobs.
4. Any crate whose tests fail once they actually run gets its own ticket. Don't
   fold those fixes into this one.

## Success criteria

- [x] No `toolchain: "1.85"` left in `.github/workflows/`. MSRV is either checked by
      a job or `rust-version` is removed.
- [x] Every workspace member's tests run in some CI job, or the member is listed as
      excluded with a reason.
- [x] The CI log of the fixing PR shows `backend_agreement` and `examples_parity`
      running (`test result: ok. … passed`), not just `Executable`.
- [x] Lint the workflow with `actionlint`.

## Related

- PR #126 (AOT fixes whose tests CI never ran), CRUSH-214/216/223.

## Resolution (naka, 2026-10-09)

- All nine `toolchain: "1.85"` lines are removed. A new `MSRV (1.95.0)` job runs
  `cargo check --workspace` with `RUSTUP_TOOLCHAIN=1.95.0`, which overrides
  `rust-toolchain.toml`. Run locally on 1.95.0 with the same excludes: green.
- A new `Test (rest)` job runs `cargo test --tests` on the 23 members that
  no job tested. With crush-aot in `Test (aot)`, all 42 members now run their
  tests in a `test-*` job. Each crate passed locally, run alone and in the
  combined invocation. None was red, so no follow-up tickets.
- No tests exist in crush-cson, crush-ffi, crush-plugin-example, crush-tui,
  or tree-sitter-crush. They stay in the list so new tests run. The
  tree-sitter grammar corpus is a separate gap, logged in TASKS.md.
- crush-lang-custom builds and passes. The "pre-existing issues" exclude
  from 2026-07 on Check/Clippy/Feature gates looks stale. Those excludes are
  left alone here.
- actionlint v1.7.12, with shellcheck 0.11.0 active: 0 errors.
