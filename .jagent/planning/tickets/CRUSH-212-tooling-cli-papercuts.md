# CRUSH-212 — Tooling papercuts: crush-diff exits 0 on bad paths; crush-tui/xtask ignore `--help`; installer & versions; doc drift

| Field | Value |
|-------|-------|
| **ID** | CRUSH-212 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | M7 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | S |
| **Filed by** | claude — 2026-10-09 test-drive sweep, reproduced on `main` `554f077` (debug build) |

## Problem

- **crush-diff:** `--help`, `--version` and nonexistent paths are treated as file
  paths → "0 files", **exit 0** — a typo silently passes a CI gate. FastVM
  "ABSTAINED" on 30/38 examples (needs caps in the harness), so "38 agree" overstates coverage.
- **crush-tui:** ignores `--help`/`--version`, goes straight into `enable_raw_mode`
  (`crates/crush-tui/src/main.rs:~120`); without a TTY: "No such device or address"
  plus a backtrace, **exit 0**.
- **xtask:** `xtask/src/main.rs` is `fn main() {}`; every tool's `--message-format`
  help still lists xtask as wired.
- **crush-installer:** installs only crushc, crush-run, crush-compile, crush-repl,
  crush-pkg — not the umbrella `crush`, crush-debugger, etc. No `--dry-run`.
  `status --message-format json` tags informational lines `code: "E-INSTALL"`.
- **Versions:** `crushc --version` says `0.2.0` (hard-coded,
  `crates/crush-lang-sdk/src/bin/crushc.rs:~26`; also `crates/crush-aot/src/bin/aotc.rs:~38`);
  crush-run, crush-compile, crush-repl, crush-pkg have no `--version`, so
  `crush-installer status` reports "unknown".
- **crush-pkg:** no `init`, `deps`, `add` or `install` subcommands (clap exit 2) — fine
  if intended, but the docs should say what exists.
- **crush-compile:** help says "Compile CRUSH/CASM text"; it only accepts CASM.
- **crush-repl:** `.quit` while an expression is pending is consumed as input
  (main REPL bug is CRUSH-195).
- **Docs:** CLAUDE.md, AGENTS.md, README and CONTRIBUTING said
  `cargo run --bin crush-run -- my-program.crush`; `run` subcommand is required
  (fixed in the filing PR).
- **squeeze** (sibling repo): `cargo build` fails — the `../../crush-ast` path assumes
  the `projects/crush-workspace/` layout, and `version = "0.3.0"` no longer matches
  crush-pkg/crush-vm 0.4.0. With both fixed it builds and runs. CRUSH-167 folded its
  flow into bare `crush-pkg`, so decide whether squeeze is retired.

## Success criteria

- [ ] crush-diff, crush-tui, xtask use clap; bad input exits non-zero.
- [ ] Every binary reports the workspace version via `env!("CARGO_PKG_VERSION")`.
- [ ] Installer installs the full user-facing set and supports `--dry-run`.
- [ ] squeeze builds against 0.4 or is archived with a pointer to `crush-pkg`.
