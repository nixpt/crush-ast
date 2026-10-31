# AI-native: what works today

Written 2026-10-31 (CRUSH-371) from the code, `main` at the time of the AI-native
wave (CRUSH-234, #375, #258, CRUSH-368..370). Re-verify before relying on a row.

**The one-line truth: no model is ever called.** Every AI surface is syntax, a CAST
node, a capability gate, and an echo stub or a local tool runner.

| Surface | State | Notes |
|---|---|---|
| Annotations (`@module @wip @temporary @decision @invariant @errors @reads @writes @covers @exhaustive-match-sites @must-call-*`) | **works** | Parsed into CAST nodes, carried into CASM; `codebase.*` reads them. |
| `crush-index` + `codebase.*` caps (11) | **works** | `modules definition callers invariants exhaustive_sites uncovered_paths wip temporaries stale_temporaries decisions annotation_history`. From the CLI: `crush-run --codebase FILE_OR_DIR` (CRUSH-368). **No** `semantic_search`. |
| Annotation warnings (W-WIP-001, W-TMP-001, E-EXH-001, E-MUT-00x) | **partial** | Warnings only, never blocking. `crush-lint` always; `crushc`/`crush-run` from CRUSH-369; `crushc --deny-warnings` to fail. Nothing requires an annotation to exist. |
| `@invariant` `check` | **stub** | Stored, never evaluated. `--invariant-runtime` removed (CRUSH-370). |
| AI opcodes (10) from CAST | **partial** | Compile and dispatch to `ai_native.<kind>` caps through one contract (`crush-vm/src/ai_args.rs`) on scheduler, PortableVm and FastVM (embedder seam). Ungranted = named `CapDenied` error (CRUSH-234). |
| `ai_semantic_match(x, "concept"[, t])` in `.crush` source | **stub** | Only AI op with source syntax. Not vector similarity: it calls the host cap, which is an echo stub (#375 fixed its stack leak). |
| `ai_native.*` default caps (9 of 10) | **stub** | Echo `{ok, kind, echo}` objects (`crush-lang-sdk/src/ai_native.rs`). |
| `ToolChain` | **partial** | Real local engine (`ai_native/toolchain.rs`): sequential/parallel/conditional/retry/fallback over registered tools. |
| `QueryProvider` / `DelegationBackend` | **stub** | Traits + test implementations only; no HTTP/model implementation. |
| Query, Synthesize, AgentDelegation, LearningLoop, ContextAware, GoalDeclaration, ProgressUpdate, KnowledgeSharing from source | **missing** | No `.crush` syntax; reachable only via CAST JSON / walkers. |
| `CapabilityDiscovery`, `AdaptationRequest`, `SemanticSwitch` | **missing** | Compile error naming the statement (CRUSH-265). |
| Granting AI from the CLI | **missing** | `crush-run` has no `--ai`; `crush-run`/`crush` refuse any program needing `ai_native.*` up front. Only embedders (`HostCapsBuilder::ai_native(true)`) can grant it. |
| AOT (Rust/C) of AI ops | **refused** | Compile error (CRUSH-234); the old `{ok,kind,echo}` stub is gone. |
| CSON `@synthesize` desugar, `crush-frontend::ai_runtime`, | **removed** | Dead code (nothing used them). |
| `Program.ai_meta` | **dead field** | Never populated; kept because it is in the serialized CAST schema, bindings and fixtures. |

## What a real provider needs

1. A captain decision on the grant: a `--ai` flag (or per-kind), and what `crush-run caps` shows for it.
2. A `QueryProvider` implementation (HTTP to a configured endpoint, or a local engine) and `DelegationBackend`, registered via `HostCapsBuilder::query_provider` / `delegation_backend` behind that grant.
3. Source syntax for query / delegation / toolchain (today CAST-only), so programs can use it without hand-built CAST.
4. Decide semantics for failures and nondeterminism (timeouts exist via cap deadlines; transcripts/replay hooks exist, CRUSH-165).
5. Real `semantic_match` (embedding or model) behind the same cap, if wanted.
