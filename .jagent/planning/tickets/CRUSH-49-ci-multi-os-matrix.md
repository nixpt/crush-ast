# CRUSH-49 — CI multi-OS matrix (ubuntu + macos + windows)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-49 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | M8 |

## Problem

CI builds/tests on `ubuntu-latest` only (.github/workflows/ci.yml — verify at
dispatch). The AOT backends branch on target_os (`.so`/`.dylib`/`.dll`) with
zero CI signal on 2 of 3 branches. **Gate status: ROADMAP's precondition
(CRUSH-26 red release build) is SATISFIED** — 008bf91 (ort download-binaries)
is on HEAD, verified s412.

## Approach

Matrix `ubuntu-latest` + `macos-latest` + `windows-latest`, each running
`cargo test --workspace` + `cargo check --all-features` + the differential
suite. ⚠ Warm-cache trap (dejavue CRUSH-CI-CACHE-1, 2026-07-26): rust-cache
restore-key fallback can fake-green a lane — per-OS cache keys that cannot
cross-pollinate, and at least the first run of each new lane genuinely cold.
Expect real platform failures (path handling, `cc` assumptions — CRUSH-51's
sites); file each as its own ticket, xfail the lane item, don't block the
matrix landing on fixing them all.

## Definition of done

- [ ] Three OS lanes live; per-OS cache keys; failures filed as tickets
      (matrix may land with named xfails, not silent skips)
- [ ] A change that breaks macos/windows-only code now fails CI (demonstrated)

## Files in scope

- `.github/workflows/ci.yml`
- ⚠ workflow-scope push block: pushing `.github/workflows` changes needs the
  `workflow` OAuth scope — land via PR from a branch pushed with proper scope
  (see workspace memory on this trap)

## Gates

None (CRUSH-26 cleared). Gates CRUSH-50.
