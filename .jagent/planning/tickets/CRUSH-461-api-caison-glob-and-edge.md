# CRUSH-461 — Stop glob re-exporting caison, remove the crush_cast::cson alias, and break the crush-cast -> crush-caison -> crush-vm edge

| Field | Value |
|-------|-------|
| **ID** | CRUSH-461 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | 0.12 (freeze prep) |
| **Issue** | [#461](https://github.com/nixpt/crush-ast/issues/461) |
| **Size** | M |
| **Dependencies** | none |

## Problem

crush-caison does `pub use caison::*;` (lib.rs:18), putting every item of the external caison crate into its semver surface; crush-cson-shim re-exports that again (lib.rs:12). crush-cast keeps a `#[deprecated] pub mod cson` alias documented as 'removed in 0.5' (lib.rs:5-10, pinned by tests/caison_alias.rs) while the workspace is at 0.11.2. Consumers going through the glob: crush-index (index.rs:71,416-532), crush-lang-custom (lib.rs:5-6), crush-python (lib.rs:56). Separately crush-cast depends on crush-caison only to re-export 5 CAISON types (src/caison.rs:9), and crush-caison depends on crush-vm (Cargo.toml:15) only for HostCap/HostCapSpec/Value in vm_cap.rs:3-4, so the AST crate transitively pulls in the VM; crush-cosm documents avoiding crush-cast for exactly this reason (crush-cosm/Cargo.toml:18-20). Ruling 28 forbids the glob re-export and says to break this edge; ruling 34 allows removal after 2 minors.

## Evidence (at origin/main 7ecd2515)

- crates/crush-caison/src/lib.rs:18; crates/crush-caison/Cargo.toml:13-15; crates/crush-caison/src/vm_cap.rs:3-4
- crates/crush-cson-shim/src/lib.rs:12
- crates/crush-cast/src/lib.rs:5-10; crates/crush-cast/src/caison.rs:9; crates/crush-cast/tests/caison_alias.rs
- crates/crush-index/src/index.rs:71,416-532; crates/crush-lang-custom/src/lib.rs:5-6; crates/crush-python/src/lib.rs:56
- crates/crush-cosm/Cargo.toml:18-20
- crates/crush-lang-sdk/src/host_caps.rs:293,296
- Cargo.lock: caison 0.1.0 from crates.io
- rulings 28, 34; CRUSH-149

## Goal

crush-caison exports only its vm_cap module (plus at most an explicit documented list), not `*`. Consumers needing CAISON types or the parser depend on `caison` (workspace dep) directly, crush-cast included, so `cargo tree -p crush-cast -e normal` contains no crush-vm. The crush_cast::cson alias and its test are removed with a CHANGELOG note citing ruling 34. crush-cson-shim re-exports an explicit list or is documented as frozen. The `caison.parse`/`cson.parse` HostCap stays in crush-caison or moves into crush-lang-sdk; capability names, effects table and behaviour are unchanged.

## Non-goals

- Removing the `cson.parse` capability alias
- Changing CAISON types or parser behaviour
- Publishing anything
- crush-cosm (already edge-free)

## Files

- crates/crush-caison/src/lib.rs
- crates/crush-caison/Cargo.toml
- crates/crush-caison/src/vm_cap.rs
- crates/crush-cast/Cargo.toml
- crates/crush-cast/src/lib.rs
- crates/crush-cast/src/caison.rs
- crates/crush-cast/tests/caison_alias.rs
- crates/crush-cson-shim/src/lib.rs
- crates/crush-index/
- crates/crush-lang-custom/
- crates/crush-python/
- crates/crush-lang-sdk/src/host_caps.rs
- CHANGELOG.md

## Done when

- `grep -rn 'pub use caison::\*' crates` prints nothing; `grep -n 'mod cson' crates/crush-cast/src/lib.rs` prints nothing and tests/caison_alias.rs is gone
- `cargo tree -p crush-cast -e normal | grep -c 'crush-vm'` prints 0
- cargo test -p crush-lang-sdk --test caison_parse_cap passes, and a test runs a program calling `caison.parse` and `cson.parse` on the scheduler VM and on PortableVm and asserts identical results
- cargo check --workspace --all-targets is green
- cargo +1.100.0 clippy -p crush-caison -p crush-cast -p crush-cson-shim -p crush-index -p crush-lang-custom -p crush-python -p crush-lang-sdk --all-targets -- -D warnings is clean

## Depends on

- none

## Refs

- CRUSH-149
- ruling 28
- ruling 34

Rulings: /workspace/workspace-meta/plans/2026-11-13-crush-ast-rulings.md (captain, 2026-11-13). Drafted by the ticket-draft workflow (s518), checked by Opus, final cut by Fable; captain checkpoint 2 on 2026-11-13. CI clippy is pinned to 1.100.0; engine tickets need a test per engine they claim; JIT tests assert the JIT compiled.
