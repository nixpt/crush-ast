# Decisions


## 2026-06-12 — [ADOPTED] Extract walkers from exosphere into standalone crush-ast peer repo

Reason: walker-core was blocked on `crush-lang` which pulled in `nanovm` → `wave3-kernel`. Swapping to `crush-cast` directly unblocked the entire walker tree.


## 2026-06-12 — [ADOPTED] Subprocess walker dispatch in exosphere

Reason: Extracted walkers are invoked as subprocess binaries by exosphere's `language_walkers.rs`.


## 2026-06-15 — [ADOPTED] Use `workspace = true` for all internal crate deps

Reason: Member crates had raw `path = "../"` dependencies. Switching to `workspace = true` with `path` + `version` enables individual crate publishing.


## 2026-06-16 — [ADOPTED] Parser-only approach for Python (no embedded RustPython VM)

Reason: Instead of embedding a full RustPython VM as a second interpreter, use `rustpython-parser` only and lower Python AST → CAST → CASM → CrushVM. One VM for everything. See `crushpython4.md`.


## 2026-06-16 — [ADOPTED] Migrate language walkers from tree-sitter to native Rust parsers

Reason: tree-sitter requires C compilation and produces syntax-only CST. Native parsers (rustpython-parser, syn, boa_parser) produce semantic ASTs, enabling better analysis and lowering. Python and Rust done; JS (boa_parser) and Bash (brush-parser) planned.


## 2026-06-16 — [ADOPTED] Frontend trait replaces Walker for native-parser languages

Reason: The tree-sitter-bound Walker trait (`tree_sitter::Language`, `tree_sitter::Tree`) doesn't fit native parsers. The Frontend trait (parse → analyze → lower) provides a clean pipeline with FeatureReport for capability analysis before lowering. See `crushpython7.md`.


## 2026-06-16 — [ADOPTED] VM type expansion: Value::{Bool, Map, Error, Bytes}

Reason: Previously bools were `Value::Int(0/1)`, maps required exosphere's object model, errors had no runtime type, and binary data was forced through `Value::Str`. Each new type eliminates a gap between the type system and runtime.

## 2026-06-17T00:40:43-05:00 — [STRATEGIC] [VERIFIED] crush-pkg: static-site capsules (bundle a site into a signed ECAP)

Reason:
Enables 'publish a site as a portable capsule' without exosphere — the surfer sitecapsule investigation showed crush-pkg already has the ECAP format (EcapManifest + sections + Ed25519 sign) but was bytecode-only. Added a site module that bundles a directory of static web assets (each file a SHA-256 EcapSection) into a signed .ecap, plus extract (hash-verified round-trip → servable tree).

Artifacts: crates/crush-pkg/src/site.rs

Rejected alternatives:
- **Store capsule metadata in manifest.metadata.custom (HashMap<String,serde_json::Value>)**: bincode (the ECAP wire format) cannot deserialize serde_json::Value — it needs deserialize_any, which non-self-describing formats lack. Used a reserved __site__.json section (plain bytes) instead, touching no shared struct.

Outcome:
New crates/crush-pkg/src/site.rs (build/write/extract_site_capsule) + CLI 'site' and 'site-extract' subcommands. 5 site tests + CLI smoke (signed build -> extract -> byte-identical). crush-pkg 44+8 tests green, workspace check green. Hosting via openko exo-light noted as future (captured).

