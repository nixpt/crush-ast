# CRUSH-209 — Build-from-clone: undocumented `../buckets` sibling; bucketspike absolute path; crush-cast pulls in the VM

| Field | Value |
|-------|-------|
| **ID** | CRUSH-209 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | M8 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | S |
| **Filed by** | claude — 2026-10-09 test-drive sweep, reproduced on `main` `554f077` (debug build) |

## Problem

- `crush-pkg/Cargo.toml:~45` (non-optional) and `crush-vm/Cargo.toml:~81` use
  `path = "../../../buckets"`. A fresh clone of crush-ast fails `cargo build`
  (`failed to read .../buckets/Cargo.toml`) until `nixpt/buckets` is cloned next to
  it. Only `.github/workflows/ci.yml:~23-25` (`mv buckets ..`) knows; README,
  CONTRIBUTING, CLAUDE.md and AGENTS.md don't mention it.
- `crates/crush-bucketspike/Cargo.toml:~41` has an absolute path
  `/home/nixp/WORKSPACE/projects/buckets` and a bare-path `crush-vm` dep; the crate is
  neither a member nor in `exclude`. (Labelled throwaway; delete or exclude it.)
- Layering: `crush-cast` → `crush-caison` → `crush-vm`, so the IR crate transitively
  depends on the whole VM. `crush-caison` only needs `HostCap`/`Value` for `vm_cap.rs` —
  move that adapter behind a feature or into crush-vm. Not a Cargo cycle, but against
  the documented frontend → cast → casm → errors layering.
- ~30 internal deps spell out `path` + `version` instead of `.workspace = true`
  (all of crush-aot's lang deps, crush-aotc, crush-ptx, crush-lint, crush-tui, …).
  Publishing isn't blocked (versions present); style only.

## Success criteria

- [x] README "Building from Source" documents the sibling clone (done in the filing PR); longer term,
      depend on the published `crush-buckets` crate by version.
- [ ] `crush-bucketspike` removed or excluded.
- [ ] `cargo tree -p crush-cast` doesn't contain `crush-vm`.
