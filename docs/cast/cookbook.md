# CAST Cookbook

Example corpus for authoring CAST JSON directly (the AI-native authoring
surface: an agent emits CAST instead of Crush source). There is no CLI for CAST
JSON; the Rust API is the way in:

```rust
crush_cast::validate_json(&json)?;                       // schema check, errors carry a JSON path
let program: crush_cast::Program = serde_json::from_str(&json)?;
let casm = crush_frontend::compile_cast_owned(program)?;  // CAST -> CASM
let vm = crush_lang_sdk::compile::casm_to_vm(&casm)?;    // CASM -> CVM1
crush_lang_sdk::Runtime::new().run(&vm)?;
```

The "Status" column below was checked against `3ae0b4a` (2026-10-16) with exactly
those calls (validate, compile, lower). No test in this repository walks
`examples/cast/` today.

## Language feature examples (`examples/cast/*.cast.json`)

| Concept | File | Status |
|---------|------|--------------------|
| Hello world: VarDecl, CapabilityCall print, Export | [print-hello.cast.json](../../examples/cast/print-hello.cast.json) | validates, compiles, lowers |
| If/else, While, For, Break, Continue | [control-flow.cast.json](../../examples/cast/control-flow.cast.json) | validates, compiles, lowers |
| Top-level functions, typed params, cross-function Call | [functions.cast.json](../../examples/cast/functions.cast.json) | validates, compiles, lowers |
| FunctionDef (body-level function definition) | [function-def.cast.json](../../examples/cast/function-def.cast.json) | validates; compile fails (`Undefined function: double`) |
| Lambda expression + `Lambda` type hint | [lambda.cast.json](../../examples/cast/lambda.cast.json) | validates, compiles, lowers |
| StructDef, NewStruct, SetField, GetField | [structs.cast.json](../../examples/cast/structs.cast.json) | validates, compiles, lowers |
| ArrayLiteral, ObjectLiteral, Index, Range | [collections.cast.json](../../examples/cast/collections.cast.json) | validates, compiles, lowers |
| TryCatch, Throw | [error-handling.cast.json](../../examples/cast/error-handling.cast.json) | validates, compiles, lowers |
| Match with all four Pattern kinds | [match.cast.json](../../examples/cast/match.cast.json) | validates, compiles, lowers |
| Pipeline (result threads into next segment) | [pipeline.cast.json](../../examples/cast/pipeline.cast.json) | validates, compiles, lowers |
| Spawn, Await, Yield | [concurrency.cast.json](../../examples/cast/concurrency.cast.json) | validates, compiles, lowers |
| Imports: CrushModule, PolyglotModule, External | [imports.cast.json](../../examples/cast/imports.cast.json) | validates; does not compile: an unresolved `CrushModule` and an `External` import are compile errors since CRUSH-110 (module imports resolve from `.crush` files, see `docs/design/import-system.md`) |
| LangBlock (embedded Python, variable injection) | [lang-block.cast.json](../../examples/cast/lang-block.cast.json) | validates, compiles, lowers |
| AI Query | [ai-query.cast.json](../../examples/cast/ai-query.cast.json) | validates, compiles, lowers |
| AI ToolChain (strategy + error handling) | [toolchain.cast.json](../../examples/cast/toolchain.cast.json) | validates, compiles, lowers |
| AI orchestration: AgentDelegation (Consensus), LearningLoop, ContextAware, CapabilityDiscovery, KnowledgeSharing, AdaptationRequest, ai_meta | [ai-orchestration.cast.json](../../examples/cast/ai-orchestration.cast.json) | validates; compile refuses `CapabilityDiscovery` (CRUSH-265) |

## Cookbook capsules (`examples/cast/cookbook/<name>/`)

Capsule directories (`capsule.toml` + `main.cast.json` + `README.md`) — used
where the manifest is part of the lesson.

| Capsule | Description | Primitives |
|---------|-------------|------------|
| [code-review-helper](../../examples/cast/cookbook/code-review-helper/) | Code review workflow with delegation + tool chain | AgentDelegation, ToolChain, AIStatement |
| [research-summarizer](../../examples/cast/cookbook/research-summarizer/) | Research summarization with query + chain + delegation | Query, ToolChain, AgentDelegation, AIStatement |
| [capability-log-reader](../../examples/cast/cookbook/capability-log-reader/) | Acquire a scoped capability handle and call it | Capability import, CapabilityCall |
| [secure-env-reader](../../examples/cast/cookbook/secure-env-reader/) | Read encrypted secrets (selective + bulk/alias/db_path) | SecureEnv import, LangBlock |
| [mcp-web-tools](../../examples/cast/cookbook/mcp-web-tools/) | Connect to an MCP server and bind tools | MCPImport |

## Status of the capsules

Status against `3ae0b4a`: every capsule's `main.cast.json` validates, compiles and
lowers except `research-summarizer` (refuses `CapabilityDiscovery`, CRUSH-265) — and,
since CRUSH-110, `mcp-web-tools`, whose `MCPImport` is refused at compile time ("`use @mcp`
is not supported yet") instead of compiling to an `mcp.connect` call.
Run with `Runtime::new()`, they stop at host capabilities nothing registers
(`capability-log-reader`: `unknown capability: cap.acquire`; `secure-env-reader`:
`secrets.read`), and `code-review-helper` runs
but every AI call yields `null` without an `ai_native` host (#261). They are
schema and compile examples today. (The old `ant crush cast validate` / `ant pkg run` commands were
exosphere's CLI and don't exist here.)

## Authoring rules of thumb

- **Print via `CapabilityCall`, not `Call`.** `Call` targets must exist in
  `Program.functions`; `Call("print", ...)` validates but fails to compile
  ("Undefined function"). `{"type": "CapabilityCall", "name": "print",
  "args": [...], "meta": {}}` is the production pattern.
- **`CapabilityCall.meta` is required** (the one expression without a serde
  default for `meta`). Everywhere else `meta` may be omitted.
- **Type hints are capitalized**: `"String"`, `"Int"`, `"Any"`,
  `{"Array": "Int"}`, `{"Struct": "Point"}`,
  `{"Lambda": {"params": ["Int"], "returns": "Int"}}` — lowercase `"string"`
  fails validation.
- **No assignment statement** — rebind with another `VarDecl` of the same name.
- **For-loop variables type as `Any`**: comparisons against them compile,
  arithmetic on them does not yet.
- **`Spawn` targets** must be zero-argument functions in `Program.functions`.
- **Dom\*** variants (DomMutate, DomEventListener, DomQuery) are web-target
  and intentionally not exemplified here.

## Primitives Reference

- **Query** — Single LLM round-trip for natural language queries
- **ToolChain** — Sequential/parallel execution of tool sequences with error handling
- **AgentDelegation** — Dispatch tasks to squadron agents with strategies (Best, RoundRobin, Broadcast, Consensus, etc.)
- **AIStatement** — Coordination ops: GoalDeclaration, ProgressUpdate, KnowledgeSharing, CapabilityDiscovery, AdaptationRequest
- **LearningLoop / ContextAware** — Adaptation and context-threading expressions

See `crates/core/crush-cast/src/lib.rs` + `src/ai.rs` for the full schema
(code wins over docs), and `docs/cast/schema-reference.md` for the written
reference.
