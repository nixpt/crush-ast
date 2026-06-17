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


## 2026-06-17T00:40:43-05:00 — [STRATEGIC] [VERIFIED] crush-pkg: static-site capsules (bundle a site into a signed ECAP)

Reason:
Enables 'publish a site as a portable capsule' without exosphere — the surfer sitecapsule investigation showed crush-pkg already has the ECAP format (EcapManifest + sections + Ed25519 sign) but was bytecode-only. Added a site module that bundles a directory of static web assets (each file a SHA-256 EcapSection) into a signed .ecap, plus extract (hash-verified round-trip → servable tree).

Artifacts: crates/crush-pkg/src/site.rs

Rejected alternatives:
- **Store capsule metadata in manifest.metadata.custom (HashMap<String,serde_json::Value>)**: bincode (the ECAP wire format) cannot deserialize serde_json::Value — it needs deserialize_any, which non-self-describing formats lack. Used a reserved __site__.json section (plain bytes) instead, touching no shared struct.

Outcome:
New crates/crush-pkg/src/site.rs (build/write/extract_site_capsule) + CLI 'site' and 'site-extract' subcommands. 5 site tests + CLI smoke (signed build -> extract -> byte-identical). crush-pkg 44+8 tests green, workspace check green. Hosting via openko exo-light noted as future (captured).


## 2026-06-17T02:30:51-05:00 — [STRATEGIC] [VERIFIED] Published core crates (crush-errors, crush-cast, casm) v0.2.0 to crates.io

Reason:
External dependents (openko/fabric, crush-symbols, mycelium-mobile, arniko) can now consume versioned registry deps instead of path deps. Preceded by a clean/format pass (rustfmt the never-formatted core crates, auto-safe clippy), metadata (keywords/categories/readme/homepage), and a licensing reconciliation.

Artifacts: crates/crush-cast/Cargo.toml

Rejected alternatives:
- **triple-license OCPL/MIT/Apache:OCPL is not an SPDX identifier (crates.io rejects it) and 'at your option' nullifies its protocol-protection intent; OCPL belongs on the openko protocol layer, not foundation IR crates**
- **OCPL-governed non-SPDX publish (license-file):unusual for a library, loses SPDX badge, contradicts the permissive intent for foundation crates**

Outcome:
3 crates live at 0.2.0; v0.2.0 tagged+pushed; license now clean dual MIT OR Apache-2.0; copyright 'Antarik / Exosphere Authors' -> 'The Crush Authors'; LICENSE-MIT+LICENSE-APACHE bundled per-crate; stale __pycache__ pyc untracked. Publish order crush-errors -> crush-cast/casm.

