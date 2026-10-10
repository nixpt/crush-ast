# CRUSH-247 — `.wasm` input never reaches the walker: read as UTF-8 text

| Field | Value |
|-------|-------|
| **ID** | CRUSH-247 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | M11 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | S |
| **Filed by** | claude — 2026-10-09 CASM↔WASM test drive |

## Problem

`crush-walk-run` and `crush-aotc` read every input with `std::fs::read_to_string`, so a real
`.wasm` binary (a WASI hello world, clang `--target=wasm32` output) fails before walking:
`Error: stream did not contain valid UTF-8`. Only binaries that happen to be valid UTF-8
get through. The root is `LanguageAdapter::walk(&self, source: &str, …)`: the shared walker
interface takes text, and `WasmAdapter` does `source.as_bytes()`. Only the standalone
`wasm_walker` binary reads bytes.

## Success criteria

- [ ] Adapters can receive bytes (e.g. `walk_bytes(&[u8], …)` with a default that decodes
      UTF-8 for text languages); `WasmAdapter` implements it directly.
- [ ] `crush-walk-run`/`crush-aotc` read files as bytes and pass them through.
- [ ] A clang-compiled `.wasm` reaches `walk_wasm`.
