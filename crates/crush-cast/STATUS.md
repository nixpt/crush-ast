# crush-cast — Status & Design Notes

Last reviewed: 2026-05-28

---

## Publication status

| | |
|---|---|
| Version | `1.0.0` |
| Published to crates.io | **No** |
| Blocker | `crush-errors = { path = "../base/errors" }` — path dep prevents `cargo publish` |
| Unblock sequence | Publish `crush-errors` → publish `crush-cast` → freeze schema |

Until published, all consumers must use a workspace path dep. The "CAST as
open contract" value prop (external tools generating CAST JSON) is not
deliverable until the crate is on crates.io with a stable version.

Note: crush-lang (`v0.1.0`) is also unpublished and has additional path deps
(`casm`, `nanovm`). crush-cast's publication path does not require crush-lang
to publish first.

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

`crush-lang`'s `ast.rs` is `pub use crush_cast::*` — the compiler imports the
schema wholesale rather than defining its own AST.

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
`Capability` import nodes declare required permissions. crush-lang's `Compiler`
collects all permissions into a `Manifest` in the CASM output. The runtime
checks the manifest before granting access. The chain is:
`source → compile-time collection → runtime enforcement`.

**JSON round-trip is the contract.**  
A Python or TypeScript tool can generate CAST JSON and pass it to
`crush-lang --from-cast` for compilation. This is the intended integration
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
- No runtime — nanovm is separate
- No stdlib — the Crush stdlib lives at `crates/core/base/stdlib/` (live
  workspace member; see "stdlib status" below)

---

## stdlib status (correction, 2026-05-28 foreman)

**The Crush stdlib is alive and well**, not archived. The earlier note in
this file ("stdlib is archived, nanovm API mismatch") was incorrect.

- **Live stdlib:** `crates/core/base/stdlib/` (workspace member; 27+ modules
  including `str`, `collections`, `math`, `regex`, `path`, `fs`, `http`,
  `env`, `dom`, `polyglot_bridge`, `ai_capabilities`, `async_cap`, `time_cap`).
- **Live bundling layer:** `crates/capabilities/corecaps/` — exposes
  `register_corecaps()` to install every namespace into a `nanovm::Registry`
  in one call.
- **Capability categorization:** every namespace is classified as either
  `stdcap` (pure utilities: str/collections/math/json/conv/path/regex/bytes/
  buffer/binary/result/data) or `corecap` (system access: env/time/http/fs/
  text/storage/task/gfx/ai/agent/learn/async/polyglot/python/js/dom).
- **Archived stdlib:** `archive/archived-stdlib/` — historical, retained
  for reference. The live stdlib superseded it. `corecaps/src/lib.rs`
  notes: "Follows the namespace layout from
  `archive/archived-stdlib/src/create_std_registry.rs`".

What Crush programs import today: the live `stdlib` namespaces via
`corecaps::register_corecaps()` (the canonical install path) or via direct
namespace import.

---

## Open questions (as of 2026-05-28)

- ✅ **`SecureEnv` decryption at runtime — RESOLVED s221 2026-05-28: Option B
  (pre-loaded keyring) ratified by captain.** Capsule receives decrypted env at
  spawn via `secure_env::SecureEnvBuilder` + `load_keys_to_env(&keys)`; capability
  gate on `secrets.read` is the consent layer. Pattern already in production for
  AI agent capsules at `crates/ai/core/agent-core/src/factory.rs:88-112`; EXO-143
  generalizes it to all CAST `SecureEnv` imports. The `SecureEnv { keys, alias,
  db_path }` variant matches Option B as-is — no schema change required.
  Implementation is a separate follow-up ticket. See `TASKS.md` EXO-143 +
  `.dejavue/decisions.md`.
- `meta` field key naming: is there a standardized scheme for source
  location, type hints, etc.? (Tracked as EXO-144.)
