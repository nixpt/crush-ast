# Decisions


## 2026-06-12T06:17:00Z — [ADOPTED] Extract walkers from exosphere into standalone crush-ast peer repo

Reason:
walker-core was blocked on `crush-lang` which pulled in `nanovm` → `wave3-kernel`. Swapping to `crush-cast` directly unblocked the entire walker tree. Zero behavioral change because `crush-lang::ast` was `pub use crush_cast::*`.


## 2026-06-12T06:38:00Z — [ADOPTED] Subprocess walker dispatch in exosphere

Reason:
Extracted walkers are invoked as subprocess binaries by exosphere's `language_walkers.rs`. No path deps in either direction — clean peer separation.


## 2026-06-15T23:45:00Z — [ADOPTED] Use `workspace = true` for all internal crate deps

Reason:
Member crates had raw `path = "../"` dependencies that only resolve inside this workspace. Switching to `workspace = true` with `path` + `version` in `[workspace.dependencies]` enables individual crate publishing. Removed `publish = false` from `[workspace.package]` to allow per-crate opt-in.

## 2026-06-15T23:59:05-05:00 — Use workspace = true for all internal crate deps

Reason:
Member crates had raw path={../} deps that only resolve inside this workspace. Switching to workspace = true with path + version in [workspace.dependencies] enables individual crate publishing.

