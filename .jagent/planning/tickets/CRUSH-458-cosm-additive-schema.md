# CRUSH-458 — COSM additive schema: ignore unknown fields at all 9 sites, carry and check a schema version

| Field | Value |
|-------|-------|
| **ID** | CRUSH-458 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | 0.12 (freeze prep) |
| **Issue** | [#458](https://github.com/nixpt/crush-ast/issues/458) |
| **Size** | M |
| **Dependencies** | none |

## Problem

crush-cosm rejects any unknown field via #[serde(deny_unknown_fields)] at schema.rs:88,100,143,292,413,437,460,480 and span.rs:12 (9 sites), so any added optional field breaks every older reader, including the CVM1 v3 kind-table loader (crush-vm/src/bytecode.rs:598) and casm's cosm_serde. A test pins the strict behaviour (crates/crush-cosm/tests/examples.rs:173-174). COSM_VERSION = "0.1.0" is defined (lib.rs:48) but nothing reads or writes it; CosmModule (schema.rs:85-95) has only kinds and interfaces, and CVM1's kt_format byte versions the encoding, not the schema. There are two load paths: CosmModule::from_json (CVM1) and casm's cosm_serde (casm/src/lib.rs:689-691, ~1175), which never touches from_json. Ruling 13: additive, ignore unknown, new optional fields = minor (RC gates F7, F9).

## Evidence (at origin/main 7ecd2515)

- crates/crush-cosm/src/schema.rs:85-95, 88,100,143,292,413,437,460,480
- crates/crush-cosm/src/span.rs:12
- crates/crush-cosm/src/lib.rs:48
- crates/crush-cosm/tests/examples.rs:173-174
- crates/crush-vm/src/bytecode.rs:11-22, 598
- crates/casm/src/lib.rs:689-691 and cosm_serde (~1175)
- docs/planning/RELEASE-1.0-CRITERIA.md:97,99
- ruling 13

## Goal

Remove deny_unknown_fields at all 9 sites so unknown fields are skipped; invert examples.rs:173-174. Unknown enum variants and missing required fields stay errors. Add cosm_version: Option<String> to CosmModule (serde default); to_json always writes COSM_VERSION. A pub check (e.g. CosmModule::check_version) treats absent as 0.1.0, accepts the same major and rejects a different major with a CosmError naming both versions; it is called from from_json and from casm's cosm_serde so both load paths enforce it. The crush-cosm README states the evolution rule in one paragraph. A warning-level typo lint for unknown keys is optional; if not cheap, document the tradeoff and skip it.

## Non-goals

- COSM K2 and COSM dispatch (#2, no ruling)
- Changing the CVM1 container version or kt_format bytes
- Relaxing unknown enum variants or required fields

## Files

- crates/crush-cosm/src/schema.rs
- crates/crush-cosm/src/span.rs
- crates/crush-cosm/src/lib.rs
- crates/crush-cosm/tests/examples.rs
- crates/crush-cosm/tests/validate.rs
- crates/crush-cosm/README.md
- crates/casm/src/lib.rs (cosm_serde)
- crates/crush-vm/src/tests/kind_table.rs (only if byte-pinned)

## Done when

- grep -rn deny_unknown_fields crates/crush-cosm/src returns nothing
- cargo test -p crush-cosm: JSON with an extra key on CosmModule, a KindDecl, an InterfaceDecl, a Span and one nested decl parses and the round-trip drops the keys; the unknown enum variant case ('mro') is still rejected
- cargo test -p crush-cosm: a module without cosm_version loads, a matching major loads, a different major is rejected with both versions in the message
- cargo test -p casm: a casm JSON Program whose cosm carries a different-major cosm_version fails to parse; cargo test -p casm --test cosm_kind_table passes
- cargo test -p crush-vm kind_table: a v3 blob whose kind table carries a different major fails with BadKindTable; existing blobs without the field load
- cargo +1.100.0 clippy -p crush-cosm -p crush-vm -p casm --all-targets -- -D warnings clean

## Depends on

- none

## Refs

- ruling 13
- RC gates F7, F9

Rulings: /workspace/workspace-meta/plans/2026-11-13-crush-ast-rulings.md (captain, 2026-11-13). Drafted by the ticket-draft workflow (s518), checked by Opus, final cut by Fable; captain checkpoint 2 on 2026-11-13. CI clippy is pinned to 1.100.0; engine tickets need a test per engine they claim; JIT tests assert the JIT compiled.
