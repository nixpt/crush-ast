# CRUSH-xps-1 — Host-approved Rust plugins for Crush packages

| Field | Value |
|-------|-------|
| **ID** | CRUSH-xps-1 |
| **Priority** | P1 |
| **Status** | In Review |
| **Phase** | M0 |
| **Assignee** | nixp |
| **Branch** | agent/nixp/CRUSH-xps-1 |
| **Dependencies** | none |
| **Estimated effort** | M |

## Problem

`crush-vm` has a native plugin ABI and an allowlisted export loader, but `crush-pkg` does not provide a host-controlled way to make a manually built Rust `cdylib` available to a packaged Crush program. Package authors therefore cannot call a Rust crate through a stable Crush wrapper without writing a custom host. Plugin loading executes native code, so the package itself must not select arbitrary library paths or grant its own exports.

## Success criteria

- [x] A host can explicitly provide a Rust plugin library and export allowlist when running a Crush package; the package manifest cannot grant either one.
- [x] Only selected plugin exports are registered as Crush capabilities, and the library remains loaded for the full execution.
- [x] With no host plugin grant, a package cannot call plugin exports. Missing libraries, invalid ABI tables, and missing allowlisted exports fail with a clear error.
- [x] Documentation includes a small manually built Rust `cdylib` wrapper and Crush package example using `crush-ffi`.

## Technical approach

- Reuse `crush-vm::plugin::load_plugin_exports`; keep the `crush-ffi` ABI and wrapper crate outside Crush's package build.
- Add explicit host CLI options for a plugin library path and its allowed export names, and thread the loaded capabilities into the existing Crush package runner.
- Keep plugin handles alive until VM execution ends; preserve current behavior when no plugin options are supplied.
- Document that plugins are trusted native code and require an explicit host-supplied path and export allowlist.

## Files to modify

- `crates/crush-pkg/src/main.rs` — parse host plugin grants for package execution.
- `crates/crush-pkg/src/runners.rs` — retain loaded libraries and register only the host allowlist.
- `crates/crush-pkg/tests/rust_plugins.rs` — exercise host grants and errors through both package launch flows.
- `crates/crush-vm/src/plugin.rs` — refuse plugin exports that shadow built-in or already granted host capabilities.
- `crates/crush-vm/src/plugin.rs` — unit-test collisions with built-in and host-registered capabilities.
- `crates/crush-pkg/MANIFEST.md` — document the manual Rust wrapper and host grant flow.

## Non-goals

- Cargo-managed plugin compilation or dependency resolution.
- Loading plugin paths or export grants declared by `capsule.toml`.
- Sandboxing or claiming that native Rust plugins are safe to load from untrusted sources.
- Changing the FFI ABI or adding general Rust-to-Crush source translation.
