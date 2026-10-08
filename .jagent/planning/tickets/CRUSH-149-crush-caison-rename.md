# CRUSH-149 — rename crush-cson → crush-caison

**Status:** PR open. **Branch:** `agent/panini/CRUSH-149`.

CAISON was renamed from CSON on 2026-09-27; crush-cson was a 76-line shell
re-exporting `caison` plus the `cson.parse` VM capability. Mechanical rename:

- `crates/crush-cson` → `crates/crush-caison` (`git mv`), `CsonParseCap` → `CaisonParseCap`.
- `crush_cast::cson` → `crush_cast::caison`; `#[deprecated] pub mod cson` re-exports it.
- Capability `caison.parse`; `cson.parse` registered as the same handler, removed in 0.4.
- `crates/crush-cson-shim` (package `crush-cson`, nothing depends on it) is the
  final re-export-only release of the old name.

## Publish order (foreman)

1. `crush-caison` (needs `caison` 0.1.0 on crates.io first)
2. changed dependents: `crush-cast`, `crush-index`, `crush-lang-custom`, `crush-python`, `crush-lang-sdk` (and anything above them in dep order)
3. `crush-cson` (the shim) last

## Not here

CRUSH-177 (delete frontend's duplicate parser), CRUSH-178 (value mapping).
