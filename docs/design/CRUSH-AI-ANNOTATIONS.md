# Crush AI-Native Annotations × Notebook Cells

## The Full Annotation Taxonomy (from crush-ast codebase)

crush has **7 annotation types** that turn code into a queryable, agent-friendly knowledge graph:

| Annotation | CAST type | File | Purpose |
|-----------|-----------|------|---------|
| `@module { purpose, exports, invariants, related }` | `ModuleManifest` | `manifest.rs:25` | Self-describing module — what it does, what it exports |
| `@wip { intent, done[], todo[], unresolved[] }` | `WipNode` | `manifest.rs:140` | Work-in-progress tracking — agents query `codebase.wips()` |
| `@temporary { reason, expires-when, added }` | `TemporaryNode` | `manifest.rs:161` | Technical debt with expiry — compiler warns W-TMP-001 after STALE_DAYS |
| `@decision { chose, over[], because, revisit-if[] }` | `Decision` | `manifest.rs:185` | Architectural decision record — queryable rationale |
| `@invariant { description, applies_to[], check }` | `Invariant` | `manifest.rs:50` | Contracts — check expressions fire in debug mode |
| `@errors { Variant: likely\|possible\|rare }` | `WeightedError` | `manifest.rs:129` | Probabilistic error declarations |
| `@covers { target, percentage }` | `CoverageClaim` | `manifest.rs:80` | Test coverage declarations |

## AI-Native Expression Types (runtime layer)

From `crush-cast/src/ai.rs`:

| Expression | What it does |
|-----------|-------------|
| `Query { query, result_type, context }` | Natural language query with expected return type |
| `ToolChain { tools[], strategy, error_handling }` | Chained tool calls (sequential/parallel/conditional) |
| `AgentDelegation { task, agents[], delegation_strategy }` | Dispatch to sub-agents by pattern or ID |
| `LearningLoop { learning_target, strategy, adaptations[] }` | Self-improving loop with adaptation actions |
| `ContextAware { expression, requires_context[], provides_context[] }` | Context-scoped expression |
| `SemanticMatch { target, concept, confidence_threshold }` | Vector-similarity matching |
| `Synthesize { output_type, constraints[], context_refs[], examples[] }` | AI code generation with constraints |

## AI-Native Statement Types

| Statement | What it does |
|-----------|-------------|
| `GoalDeclaration { goal, deadline, milestones, dependencies }` | Define a task to be completed |
| `ProgressUpdate { task, status, completed, blocked }` | Update task progress |
| `KnowledgeSharing { topic, summary, source, confidence }` | Share knowledge with agents |

## Compiler Warning Passes

| Warning | What triggers it |
|---------|-----------------|
| **W-WIP-001** | `@wip` with non-empty `todo` or `unresolved` — blocks shipping |
| **W-TMP-001** | `@temporary` that has exceeded STALE_DAYS — stale technical debt |
| **W-WIP-002** (planned) | `@wip` without `started_by` or `intent` |

## CSON (Crush Semantic Object Notation)

From `docs/design/cson_proposal.md`:

```
~0.95 confidence weights           → age: 34 ~0.95
~"semantic key" fuzzy matching     → ~"Billing issues": "queue_billing"
@annotations in data               → @wip { owner: "foreman" }
...probability ranges              → sentiment: "angry" ~0.7..0.9
?optional uncertainty               → cost: 50 ?0.2
&agent lock on values               → &foreman api_key: "..."
```

## Notebook Cell Integration

Every notebook cell is a self-contained crush/Sona/@python program. But with the annotation system, cells become **persistent, queryable knowledge units**:

```crush
// Cell 5 — Data pipeline
@wip {
    intent: "Build ETL pipeline for user analytics"
    started_by: "cece"
    done: ["connect to API", "parse JSON response"]
    todo: ["add error handling for network timeout", "write unit tests"]
    unresolved: ["API returns nested arrays inconsistently"]
}

@decision "use-parallel-map-not-for-loop" {
    chose: "arr.par_map(fn)"
    over: ["for loop with mutable accumulator", "fold"]
    because: "64-core machine, 40x observed speedup on 10k items"
    revisit-if: ["single-core deployment req changes", "map body uses non-threadsafe caps"]
}

@temporary {
    reason: "API key hardcoded until secrets manager ships"
    expires-when: "std::secrets module is available"
    added: "2026-07-10"
}

@invariant "no-duplicate-records" {
    description: "Pipeline must never produce duplicate user IDs"
    applies_to: [transform_step, dedup_step]
}

@errors {
    NetworkTimeout: likely
    ParseError: possible
    DuplicateIdError: rare
}

fn process_users(raw) {
    let filtered = raw |> filter_active |> dedup
    return filtered
}
```

### What the notebook gets for free

| Capability | Annotation source | Value |
|-----------|------------------|-------|
| Track what's in progress | `@wip` | Per-cell task status, visible in UI |
| Know why a cell exists | `@decision` | Design rationale, revisited when conditions change |
| See what's temporary | `@temporary` | Stale warning indicator in cell header |
| Know what can fail | `@errors` | Error badges on cell (likely=orange, rare=gray) |
| See what invariants hold | `@invariant` | Debug-mode checks fire automatically |
| Find cells semantically | `codebase.*` caps | "find cells about ETL" → returns this cell |
| AI agent context | All annotations | Agent reads cell manifest before touching code |

### Notebook Cell UI with Annotations

```
┌─ Cell 5 [@wip] [@temporary] ─────── [sona] [FastVM] ───────────────┐
│ ⚡ W-WIP-001: 2 todo, 1 unresolved                                    │
│ 📋 W-TMP-001: stale on 2026-08-10                                    │
│                                                                      │
│ @wip { intent: "Build ETL pipeline", todo: [2], unresolved: [1] }   │
│ @temporary { reason: "hardcoded API key", expires: "std::secrets" } │
│ @decision "use-parallel-map-not-for-loop"                            │
│ @invariant "no-duplicate-records"                                    │
│ @errors { NetworkTimeout: likely, ParseError: possible }             │
│                                                                      │
│ fn process_users(raw) { ... }                                        │
│                                                                      │
│ [Expand Annotations] [View Decisions] [Show Codebase Links]         │
└──────────────────────────────────────────────────────────────────────┘
│
└─ Cell 6 [@ai.generate] ─────────────────────���───────────────────────┐
│ @ai.synthesize("NetworkTimeout error handler for process_users")     │
│   output_type: Function                                              │
│   constraints: ["handle retry with exponential backoff"]             │
│   context_refs: [process_users]                                      │
└──────────────────────────────────────────────────────────────────────┘
```

### Notebook → Codebase Index

The notebook format (.crush-nb) becomes a first-class codebase citizen:

```crush
// Crush program queries the notebook as a codebase
let wip_cells = @codebase.wips()
// → [{ cell: "Cell 5", intent: "Build ETL pipeline", todo: 2, unresolved: 1 }, ...]

let stale_cells = @codebase.stale_temporary()
// → [{ cell: "Cell 5", reason: "hardcoded API key", added: "2026-07-10" }]

let decisions = @codebase.decisions("notebook.crush-nb")
// → [{ name: "use-parallel-map-not-for-loop", chose: "arr.par_map(fn)", ... }]

let related = @codebase.modules_related_to("ETL pipeline")
// → finds cells by semantic title match, invariant keyword, or @module.purpose
```

### CSON-Native Notebook Config

The notebook manifest itself becomes a CSON file, leveraging confidence weights:

```cson
@module {
    purpose: "User analytics ETL pipeline — extracts, transforms, and deduplicates"
    exports: [process_users, dedup_step, transform_step]
    related: ["visualization-notebook", "api-auth"]
}

[session]
name: "User Analytics Exploration"
created: "2026-07-12"
owner: "cece"

[cells.5]
status: "done" ~1.0
runtime_tier: "FastVM" ~1.0
quality: "working" ~0.85  # AI estimates 85% confidence cell is correct
```

## Implementation Plan

### Phase 1 — Annotation Renderer in crush-visuals (1 session)

| Step | What |
|------|------|
| 1a | `crush-visuals-source-bridge` parses annotations from notebook cells into VisualGraph node data |
| 1b | `crush-visuals-egui` renders W-WIP-001 badges, W-TMP-001 badges, error likelihood colors |
| 1c | `crush-visuals-egui` renders @decision expander, @invariant check results |
| 1d | Cell header shows aggregated annotation status (todo count, stale count) |

### Phase 2 — Notebook Codebase Index (1 session)

| Step | What |
|------|------|
| 2a | `crush-index` registers .crush-nb files as indexable codebase modules |
| 2b | `codebase.wips()`, `codebase.stale_temporary()`, `codebase.decisions()` work on notebooks |
| 2c | `codebase.modules_related_to(query)` searches across all indexed notebooks |

### Phase 3 ��� AI-Native Cell Generation (1 session)

| Step | What |
|------|------|
| 3a | `@ai.synthesize` in a cell generates crush/Sona code → inserts as next cell |
| 3b | `@ai.agent_delegate` dispatches a sub-agent → agent output appears as a cell |
| 3c | `@ai.knowledge_sharing` pushes cell output to the capsule's memory service |
| 3d | AI-proposed cells have `~0.0..1.0` confidence weights visible in the UI |
