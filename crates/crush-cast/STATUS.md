# crush-cast — Status & Design Notes

Last reviewed: 2026-10-07 (CRUSH-168). Sections below that describe the
pre-extraction exosphere tree were rewritten for crush-ast; history of where
things came from is in `docs/planning/MIGRATION-INVENTORY.md`.

---

## Publication status

| | |
|---|---|
| Version | workspace version (`version.workspace = true`, see root `Cargo.toml`) |
| Published to crates.io | **Yes** — `crush-cast` (with `crush-errors`) |
| Wire format version | `CAST_VERSION` in `src/pack.rs` (separate from the crate version) |

External tools can depend on `crush-cast` from crates.io; inside this
workspace it is a `workspace = true` dependency like every other internal crate.

---

## What this crate is

The stable intermediate representation (IR) for the Crush language. It is the
lingua franca between all Crush tooling: parsers, compilers, language servers,
code generators, and indexers all speak CAST.

The canonical representation is **JSON, not Rust structs**. This is deliberate:
external tools in Python, TypeScript, or any language can produce or consume
CAST without touching Rust. Two codegen binaries ship with the crate:

- `export-ts` — TypeScript bindings via `ts-rs` (`--features ts-export`)
- `export-py` — Python dataclasses

`crush-frontend` uses `crush_cast` types directly (`parse_source` returns a
`crush_cast::Program`, `compile_cast` takes one) — the compiler has no AST of
its own.

---

## Key design decisions

**AI constructs are first-class enum variants.**  
`AIExpression` (Query, ToolChain, AgentDelegation, LearningLoop, ContextAware)
and `AIStatement` are full enum arms alongside `If`, `While`, `Return`. The
parser must understand them; the compiler must emit them. AI orchestration is a
core language feature, not an annotation layer.

**LangBlock avoids VM merging.**  
Instead of "run Python inside the Rust VM", a `LangBlock` node contains raw
foreign source with variable injection metadata. Walker crates execute it in a
sandbox at runtime. No cross-VM interop complexity.

**Security flows through the IR.**  
`Capability` import nodes declare required permissions. `crush-frontend`'s
compiler collects them into the `Manifest` of the CASM output. The runtime
checks the manifest before granting access. The chain is:
`source → compile-time collection → runtime enforcement`.

**JSON round-trip is the contract.**  
A Python or TypeScript tool can generate CAST JSON; a Rust host deserializes
it into `crush_cast::Program` and compiles it with
`crush_frontend::compile_cast`. (There is no CLI flag that reads CAST JSON
today — `crushc --emit ast` only dumps it.) This is the intended integration
path for external code generators and AI-assisted code synthesis.

---

## Schema overview

See `src/` for the full Rust definitions. Key types:

- `Program` — `cast_version`, `entry`, `functions: HashMap<String, Function>`, `ai_meta`
- `Function` — params with type hints, `body: Vec<Statement>`, `meta`
- `Statement` — variable decl/export, control flow, `TryCatch`/`Throw`, **`LangBlock`**, `Import`, DOM mutations, **`AI(AIStatement)`**
- `Expression` — literals, ops, calls, `CapabilityCall`, `Pipeline`, `Spawn`, `Match`, DOM queries, **`AI(AIExpression)`**
- `ImportStatement` — `CrushModule`, `PolyglotModule`, `MCPImport`, `Capability`, `External`, `SecureEnv`

Every `Statement` and `Expression` carries `meta: Option<HashMap<String, Value>>`
for source location, compiler hints, and custom tool data.

For the CASM bytecode output schema (the compiled form), see `SCHEMA.md`.

---

## What is NOT in this crate

- No execution logic — pure data + serde
- No CASM bytecode — lives in the `casm` crate
- No runtime — `crush-vm` (CVM1 scheduler, PortableVm, FastVM) is separate
- No stdlib — see below

---

## Where the stdlib lives

The Crush standard library is part of `crush-lang-sdk`, not this crate:

- **stdcaps** (pure: `str`, `collections`, `math`, `conv`, `json`, `path`,
  `regex`, `bytes`/`buffer`/`binary`, `result`, `text.sort/uniq`,
  `time.format/parse`, `env.os/arch`) — `crates/crush-lang-sdk/src/stdlib.rs`
  and `crates/crush-lang-sdk/src/stdlib/`, registered by
  `HostCapsBuilder::stdlib(true)` (cargo feature `stdlib`).
- **`system.*`** (the System Bytecode Layer, written in Crush) —
  `crates/crush-lang-sdk/sbl/sbl_core.crush`, run by `src/sbl.rs`.
- **corecaps** (I/O behind a named grant: `fs.*`, `time.now/sleep`, `env.get`,
  `net.*`, `text.head/tail/...`) — `crates/crush-lang-sdk/src/host_caps.rs`,
  `src/text_tools.rs`, `src/net.rs`.

The stdcap/corecap split, and what was (and was not) carried over from the
exosphere stdlib this one replaced, is in
[`docs/planning/MIGRATION-INVENTORY.md`](../../docs/planning/MIGRATION-INVENTORY.md) §2.

---

## Open questions

- `SecureEnv` runtime decryption was settled in exosphere (pre-loaded keyring,
  gated on a `secrets.read` grant); nothing in crush-ast implements it yet. The
  `SecureEnv { keys, alias, db_path }` variant needs no schema change for that
  design.
- `meta` field key naming: there is no standardized scheme for source
  location, type hints, etc. yet.
