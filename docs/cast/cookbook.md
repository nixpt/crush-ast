# CAST Cookbook

Proof-carrying example corpus for authoring CAST JSON directly (the
CRUSH-AI-NATIVE authoring surface). Every `*.cast.json` under
`examples/cast/` is walked by the harness test
`crush-lang/tests/cast_examples.rs` (`every_cast_example_validates_and_compiles`):
each file must pass `crush_cast::validate_json`, and must compile via
`crush_lang::compile_cast` unless it is on the in-test documented
compile-exempt list.

```bash
cargo test -p crush-lang --test cast_examples
```

## Language feature examples (`examples/cast/*.cast.json`)

| Concept | File | Validate / compile |
|---------|------|--------------------|
| Hello world: VarDecl, CapabilityCall print, Export | [print-hello.cast.json](../../examples/cast/print-hello.cast.json) | `ant crush cast validate examples/cast/print-hello.cast.json` |
| If/else, While, For, Break, Continue | [control-flow.cast.json](../../examples/cast/control-flow.cast.json) | `ant crush cast validate examples/cast/control-flow.cast.json` |
| Top-level functions, typed params, cross-function Call | [functions.cast.json](../../examples/cast/functions.cast.json) | `ant crush cast validate examples/cast/functions.cast.json` |
| FunctionDef (body-level function definition) | [function-def.cast.json](../../examples/cast/function-def.cast.json) | validate only — compile-exempt¹ |
| Lambda expression + `Lambda` type hint | [lambda.cast.json](../../examples/cast/lambda.cast.json) | validate only — compile-exempt¹ |
| StructDef, NewStruct, SetField, GetField | [structs.cast.json](../../examples/cast/structs.cast.json) | `ant crush cast validate examples/cast/structs.cast.json` |
| ArrayLiteral, ObjectLiteral, Index, Range | [collections.cast.json](../../examples/cast/collections.cast.json) | `ant crush cast validate examples/cast/collections.cast.json` |
| TryCatch, Throw | [error-handling.cast.json](../../examples/cast/error-handling.cast.json) | `ant crush cast validate examples/cast/error-handling.cast.json` |
| Match with all four Pattern kinds | [match.cast.json](../../examples/cast/match.cast.json) | validate only — compile-exempt¹ |
| Pipeline (result threads into next segment) | [pipeline.cast.json](../../examples/cast/pipeline.cast.json) | `ant crush cast validate examples/cast/pipeline.cast.json` |
| Spawn, Await, Yield | [concurrency.cast.json](../../examples/cast/concurrency.cast.json) | `ant crush cast validate examples/cast/concurrency.cast.json` |
| Imports: CrushModule, PolyglotModule, External | [imports.cast.json](../../examples/cast/imports.cast.json) | `ant crush cast validate examples/cast/imports.cast.json` |
| LangBlock (embedded Python, variable injection) | [lang-block.cast.json](../../examples/cast/lang-block.cast.json) | `ant crush cast validate examples/cast/lang-block.cast.json` |
| AI Query | [ai-query.cast.json](../../examples/cast/ai-query.cast.json) | `ant crush cast validate examples/cast/ai-query.cast.json` |
| AI ToolChain (strategy + error handling) | [toolchain.cast.json](../../examples/cast/toolchain.cast.json) | `ant crush cast validate examples/cast/toolchain.cast.json` |
| AI orchestration: AgentDelegation (Consensus), LearningLoop, ContextAware, CapabilityDiscovery, KnowledgeSharing, AdaptationRequest, ai_meta | [ai-orchestration.cast.json](../../examples/cast/ai-orchestration.cast.json) | `ant crush cast validate examples/cast/ai-orchestration.cast.json` |

¹ Compile-exempt: valid CAST that `compile_cast` cannot handle yet. The
authoritative, can't-go-stale list (with reasons) lives in
`crates/core/crush-lang/tests/cast_examples.rs` (`COMPILE_EXEMPT`).

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

## Running a Cookbook Capsule

```bash
# Validate
ant crush cast validate <path>/main.cast.json

# Compile
ant crush compile --from-cast <path>/main.cast.json -o output.casmb

# Install and run
ant pkg install <path>
ant pkg run <capsule-name>
```

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
