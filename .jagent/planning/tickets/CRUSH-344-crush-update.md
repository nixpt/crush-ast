# CRUSH-344 — `crush update`: self-update from the GitHub release binaries

| Field | Value |
|-------|-------|
| **ID** | CRUSH-344 |
| **Priority** | P2 |
| **Status** | In review |
| **Phase** | Distribution |
| **Depends on** | CRUSH-342 (release artifacts + `SHA256SUMS`) |

## Problem

v0.9.0 is the first release with prebuilt Linux binaries (`crush-<ver>-x86_64-unknown-linux-gnu.tar.gz`
+ `SHA256SUMS`, built by `.github/workflows/artifacts.yml`), but nothing updates an install made from
them. `crush-installer` only copies binaries from a local `--bin-dir`; a user has to find the release,
download, check the hash by hand and copy eight binaries — and a half-finished copy leaves a toolchain
with mismatched versions.

## Done when

- [x] `crush-installer update` and `crush update` (forwards to it; clear hint when it is missing).
- [x] `--check`: prints installed + latest, exit 0 up to date / **10** update available, no writes.
- [x] Default: download tarball + `SHA256SUMS`, verify sha256 **before extracting** (refuse on
      mismatch or missing entry), stage on the install dir's filesystem, swap each binary with
      `rename(2)`; any failure leaves the old binaries. Works while `crush` is the running binary.
- [x] `--version X.Y.Z` pin (the only way to downgrade), `--dir DIR`, `--force`, `--yes` accepted.
- [x] Cargo-managed install refused: `installed with cargo; run: cargo install crush-lang-sdk`
      unless `--force`.
- [x] Unsupported target refused, naming it, pointing at `cargo install crush-lang-sdk`.
- [x] `GITHUB_TOKEN` used for the API call when set, never required.
- [x] Unit tests without network (`file://` fixtures); live run against the real v0.9.0 release.

## Resolution

`crates/crush-installer/src/update.rs` (module docs list the steps). Decisions:

- **Latest** = highest non-draft, non-SemVer-pre-release tag from `/releases` that carries this
  target's tarball and `SHA256SUMS`. Not `/releases/latest`: pre-1.0 releases are published as
  GitHub pre-releases (CRUSH-342), which `/latest` skips.
- **Install dir** = `--dir`, else the running toolchain's dir (the umbrella passes its own via the
  hidden `--caller-dir`) when `crush-installer` manages it *or cargo does* (so the cargo refusal fires
  instead of silently updating a different copy), else `~/.crush/bin`.
- **Cargo-managed** = the dir is `$CARGO_HOME/bin` holding a `crush`, or a `.crates2.json` in
  `$CARGO_HOME` / `dir/..` lists a toolchain binary for that dir.
- **Atomicity**: all binaries are extracted and validated first; then each old one is hard-linked
  into a backup and the new one renamed over it; a failed rename renames the replaced ones back.
  A crash (power loss) mid-swap is not covered.
- **Versions**: current = `<dir>/crush --version`. Installer manifest version refreshed after a swap.
- `CRUSH_UPDATE_RELEASES_URL` / `CRUSH_UPDATE_DOWNLOAD_BASE` override the GitHub URLs (mirrors,
  tests; `file://` works).

Follow-up (not done here): the release tarball does not contain `crush-installer`, so a pure
tarball install needs `cargo install crush-installer` before `crush update` works. Adding
`-p crush-installer` to `artifacts.yml` would close that; the updater already replaces
`crush-installer` when a tarball carries it.
