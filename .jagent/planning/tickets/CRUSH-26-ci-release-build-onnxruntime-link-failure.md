# CRUSH-26 — CI `Build (release)` fails workspace-wide: `ort-sys` can't find native `onnxruntime` static library

| Field | Value |
|-------|-------|
| **ID** | CRUSH-26 |
| **Priority** | P0 — every push to `main` has failed CI for 24+ hours |
| **Status** | Backlog |
| **Phase** | Build health |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | S |

## Problem

The `Build (release)` CI job has failed on **every one of the last 10
consecutive pushes to `main`**, going back to at least 2026-07-19T22:16
(commit `d8088ea`) through the most recent (`d5ebb3d`, Vega's CRUSH-25 fix).
This is a workspace-wide CI break, not scoped to any one branch or PR —
`agent/buffy/M2-JIT-PHASES-2-4` (PR #21) inherited it the moment `main` was
merged in; the branch's own content is unrelated.

```
error: could not find native static library `onnxruntime`, perhaps an -L flag is missing?
error: could not compile `ort-sys` (lib) due to 1 previous error
```

Fails ~20 seconds into the release build, immediately on reaching
`crush-vm`, before any of the crate's own code compiles.

## Root cause

`crates/crush-vm/Cargo.toml`:
```toml
[features]
default = ["native-plugins"]
native-plugins = ["dep:ort", "dep:libloading", "dep:crush-ffi"]

[dependencies]
ort = { version = "2.0.0-rc.12", optional = true }
```

`ort` (ONNX Runtime bindings, likely pulled in for `crush-lint`'s `AiLinter`
model-inference path — see PR #21's `Session::builder()` work) is enabled
**by default**, not behind an opt-in feature. `cargo build --release -p
crush-vm ...` (the CI job's exact invocation, no `--no-default-features`)
therefore always activates `ort-sys`, which needs a system-installed
`onnxruntime` static library at link time. GitHub's hosted runners don't
have one installed, and `ort`'s own `download-binaries` feature (which
would vendor a matching ONNX Runtime automatically, the usual fix for this
exact class of failure) is apparently not active either — check whether
it's genuinely absent from `ort`'s default features at 2.0.0-rc.12, or
whether the CI environment blocks the download.

**Not a CI-vs-local discrepancy in the code** — this box's local `cargo
check --workspace` and `cargo build` succeed, most likely because a system
`onnxruntime` happens to be installed/cached locally. That's exactly why
this went unnoticed for a day: every author testing locally sees green,
only CI (and any other clean-environment box) sees red.

## Reproduction

```bash
# on a clean environment with no system onnxruntime installed:
cargo build --release -p crush-vm
# → error: could not find native static library `onnxruntime`
```

Or just look at any of the last 10 `main` CI runs:
```bash
gh run list --repo nixpt/crush-ast --branch main --workflow CI --limit 10
```

## Technical approach (starting points, not a committed design)

1. **Preferred**: enable `ort`'s `download-binaries` feature (or whatever
   the 2.0.0-rc.12 equivalent is called) so `ort-sys`'s build script fetches
   a matching prebuilt ONNX Runtime instead of requiring one on `PATH`/`-L`.
   Smallest change, no CI workflow edits needed, fixes local-clean-checkout
   builds too.
2. **Alternative**: move `native-plugins` out of `crush-vm`'s `default`
   features, make CI's release-build job explicitly opt in with
   `--features native-plugins` only where a real consumer needs it. Larger
   blast radius — need to audit whether anything currently relies on
   `native-plugins` being on by default (feature-gates check already passes
   with `--no-default-features` per the same CI run, so likely safe, but
   verify).
3. **Alternative**: install `onnxruntime` in the CI workflow (`apt`/manual
   download) before the release-build step. Works but adds runner setup
   time to every CI run and doesn't fix the same failure for anyone else
   cloning fresh without local onnxruntime already present.

## Files to modify

- `crates/crush-vm/Cargo.toml` — `ort` dependency spec (feature list) and/or
  `default` features
- `.github/workflows/ci.yml` (or equivalent) — only if going with approach 3

## Non-goals

- Auditing what `crush-lint`'s `AiLinter` actually needs from `ort`/ONNX
  Runtime at runtime — that's a separate question from "why does the build
  fail," out of scope here.
- Fixing CRUSH-24 (JIT CALL/RETURN) or CRUSH-25 (scheduler.rs bounds guard,
  Vega/chroma) — unrelated failures in unrelated subsystems, filed
  separately.

## Done condition

`cargo build --release -p crush-aot -p crush-vm -p crush-lang-sdk -p
crush-lang-python -p crush-lang-js -p crush-lang-rust -p crush-lang-c -p
crush-lang-bash -p crush-lang-go -p crush-lang-zig -p crush-lang-dart`
succeeds on a clean checkout with no system `onnxruntime` pre-installed
(e.g. inside a fresh container or CI itself). `Build (release)` goes green
on `main` and stays green across the next several pushes.

## References

- Found while checking PR #21 (`agent/buffy/M2-JIT-PHASES-2-4`)'s CI after
  merging `main` forward (s391, foreman, 2026-07-20). Confirmed pre-existing
  by checking the last 10 `main` CI runs — all failed identically before
  this PR's merge ever touched `main`.
- First observed failing run: `d8088ea` (2026-07-19T22:16:19Z),
  `TASKS.md: mark CRUSH-18/19 done`. May have started even earlier —
  10-run window was what was checked, not exhaustively bisected.
