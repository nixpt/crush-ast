# Crush AI-Native Language Design — Extended Findings

> **Context**: Steps 1–7 of the [original roadmap](./ai-native-roadmap.md) are
> complete (s302, 2026-06-17).  This document captures the design space beyond
> Step 8 — what a language written *at the dawn of the AI era*, by AI/human
> collaboration, should bring to the table that no existing language does.
>
> These ideas emerged from watching the Phase 1 work succeed and asking what
> the foundation still leaves unsolved.

---

## The Connecting Thread

Every idea below has the same shape: take something that today lives **outside
the code** — design rationale, probability estimates, session notes, expiry
dates, verbal agreements — and make it a **first-class AST node** with three
properties:

1. **Queryable by machines** — `codebase.*` caps return it
2. **Checkable by the compiler** — warnings/errors enforced, not advisory
3. **Renderable for humans** — `extract_symbol`, `extract_manifest` show it inline

Phase 1 did this for contracts (`@errors`, `@invariant`, `@covers`).
Phase 2 does it for the rest of the collaboration surface.

---

## Idea 1 — Executable Contracts

**The gap**: `@errors [NotFound]` is a hint today. An agent reads it; a human
reads it; nobody checks it at runtime.

**The proposal**: invariants become falsifiable in debug mode.

```crush
@invariant "stack-never-empty" {
    description: "executor stack must have at least one frame at call boundaries"
    applies_to: [execute_one, run_scheduled]
    consequence: "StackUnderflow panics the executor at runtime"
    check: fn(ctx) { ctx.stack.len() > 0 }
}
```

When `check` is present, the compiler generates a debug-mode assertion at every
entry point listed in `applies_to`.  The human writes the constraint once; every
future agent that touches those functions sees it fire in tests before it silently
breaks production.

**Status**: buildable now.  The `Invariant` CAST node already exists in
`crush-cast/src/manifest.rs`.  Adding a `check_expr: Option<Expression>` field
and emitting an assertion call in the compiler is a contained change.

---

## Idea 2 — Reasoning Traces

**The gap**: code encodes WHAT; git commits partially encode WHY.  But commits
are external, per-line, and lossy.  An agent hitting `Rc<RefCell<...>>` still
has to reconstruct the reason from context.

**The proposal**: decisions are language nodes, not comments.

```crush
@decision "use-rc-refcell-not-arc-mutex" {
    chose: "Rc<RefCell>"
    over: ["Arc<Mutex>", "raw-pointer"]
    because: "single-threaded executor; Arc overhead buys nothing"
    revisit-if: [
        "multi-threaded execution lands",
        "perf profiling shows contention"
    ]
}
```

`codebase.decisions("module")` returns these.  `revisit-if` is the key field:
when an agent (or the compiler) detects that a listed condition is met — e.g.
`multi-threaded` appears in a new `@invariant` — it emits `W-DEC-001: decision
'use-rc-refcell-not-arc-mutex' may need revisiting`.

**Status**: CAST node addition + new `codebase.decisions()` cap.  No compiler
logic needed initially (the `revisit-if` check is a future lint).

---

## Idea 3 — Mutation Surface Tracking

**The gap**: `@writes [thread.ip, thread.stack]` is recorded but not enforced.
Callers don't know the function invalidates state they're holding.

**The proposal**: mutation surface becomes a constraint the compiler cross-checks.

```crush
@invalidates [thread.stack]     # callers must not hold refs across this call
@must-call-before [recover]     # compile error if recover runs without execute_one first
@must-call-after  [flush]       # compile error if flush is not called after this
```

The compiler builds a happens-before graph from `@must-call-before` /
`@must-call-after` across the call graph.  Violations are `E-MUT-001`.
`@invalidates` is a softer lint: warn when the same local is used both before
and after a call to an `@invalidates` function without a re-read.

**Status**: medium complexity.  Requires call-graph analysis in `crush-frontend`
and annotation additions.  The `crush-index` call graph already exists as
the foundation.

---

## Idea 4 — Session Continuity Node (`@wip`)

**The gap**: multi-session development (human + AI across days, or agent
handoffs mid-task) has no language representation.  The next session starts
blind, reading git log and grepping for TODO comments.

**The proposal**: work-in-progress state is a language node.

```crush
@wip {
    intent: "add batch dispatch to scheduler"
    started-by: "agent/crusher-42"
    done:   ["parse", "index build"]
    todo:   ["emit bytecode", "test multi-frame"]
    unresolved: [
        "how to handle recursive dispatch — see @decision above",
        "batch size limit needs benchmarking"
    ]
}
```

`check_source()` emits `W-WIP-001` when a file has `@wip` nodes with non-empty
`todo` or `unresolved` fields.  This is your CI gate for "don't ship
half-done work."  An agent resuming work calls `codebase.wip()` to orient
before reading any source — ~5 lines vs. a full file read.

**Status**: buildable now.  New CAST node + `codebase.wip()` cap +
`W-WIP-001` diagnostic in `exhaustive_check.rs` (or a sibling pass).

---

## Idea 5 — Probabilistic Error Annotations

**The gap**: `@errors [NetworkTimeout, ParseError]` is boolean — both errors
exist.  Reality is probabilistic.  An agent generating retry logic or a human
writing error handling can't tell which path is worth optimizing.

**The proposal**: error likelihood is part of the annotation.

```crush
@errors {
    NetworkTimeout: likely      # >50% of failure cases in production
    ParseError:     rare        # <5%
    Unauthorized:   possible    # 5–50%
}
```

`codebase.definition("fn")` returns likelihood alongside variant names.  An
agent writing retry logic uses `likely` to decide where to add exponential
backoff.  A human writing a user-facing error message uses `rare` to decide
whether to show the full traceback or just "something went wrong."

**Status**: additive to `FunctionAnnotations` — new `errors_weighted` field as
`Vec<(String, ErrorLikelihood)>` where `ErrorLikelihood` is a 3-variant enum.
`@errors [...]` and `@errors { ... }` coexist for backward compat.

---

## Idea 6 — Bidirectional Documentation

**The gap**: documentation and code drift because they are separate artifacts
with no enforced coupling.  `@module { purpose: "..." }` (Phase 1) is the start,
but it's one-way: code → doc.

**The proposal**: the spec *is* the source skeleton.

```bash
# Forward: generate human docs from annotations (already implied by Phase 1)
crush doc generate src/scheduler.crush

# Reverse: generate source skeleton from a natural-language spec
crush scaffold --from-spec docs/scheduler-spec.md
# emits @module, @invariant, @errors skeletons from the spec text
```

The `crush scaffold` command uses an LLM to extract structure from the spec doc
and emit stub Crush source with annotations pre-filled.  The human or agent
fills in the bodies.  The spec and the source converge on the same representation
instead of diverging.

**Status**: CLI tooling (new `bin/crush-scaffold`), not a language change.
Depends on an LLM call (either local via `ai.query` cap or external).  Medium
complexity.

---

## Idea 7 — Temporal Validity (`@temporary`)

**The gap**: workarounds become permanent.  Technical debt has no expiry date.
TODO comments outlive their context by years.

**The proposal**: temporary decisions carry an explicit expiry condition.

```crush
@temporary {
    reason: "linear scan until sorted-index lands in crush-index"
    expires-when: "crush-index adds BTree backend"
    owner: "captain"
    added: "2026-06-17"
}
```

`check_source()` emits `W-TMP-001` when the compiler can detect the expiry
condition is met (e.g. `BTree` appears in the index schema, or a listed crate
version is available), or `W-TMP-002` when `added` is more than 90 days ago
with no `expires-when` resolution.

An agent scheduled to run weekly can call `codebase.stale_temporaries()` to
surface debt that's due for removal.

**Status**: CAST node addition is straightforward.  The compiler date-check
(`added` > 90 days) is trivial.  The condition-detection lint (`expires-when`
string matching against the index) is a heuristic and ships later.

---

## Buildability Matrix

| Idea | Effort | Blocks on | Phase |
|------|--------|-----------|-------|
| Executable contracts (`check` field on `@invariant`) | Low | nothing — extends existing node | 2a |
| `@wip` continuity node | Low | nothing — new node + cap + W-WIP-001 | 2a |
| `@temporary` validity | Low | date introspection in diagnostic pass | 2a |
| Probabilistic `@errors` | Low-Med | `FunctionAnnotations` extension | 2a |
| Reasoning traces (`@decision`) | Med | new node + `codebase.decisions()` | 2b |
| Mutation surface (`@invalidates`, `@must-call-*`) | Med-High | call-graph analysis in frontend | 2b |
| Bidirectional docs (`crush scaffold`) | Med | LLM integration (ai.query cap) | 2c |

**Phase 2a** (contained additions, no architectural change):
executable invariants, `@wip`, `@temporary`, probabilistic errors.

**Phase 2b** (new analysis passes, moderate compiler work):
`@decision`, mutation surface tracking.

**Phase 2c** (external LLM dependency):
bidirectional scaffold.

---

## What Phase 2a Looks Like in Crush Source

A module written with all Phase 2a annotations:

```crush
@module {
    purpose: "cooperative green-thread scheduler for CVM1"
    exports: [run_scheduled, StepAction]
    exhaustive_types: [StepAction]
}

@wip {
    intent: "add batch dispatch"
    done:   ["parse", "index"]
    todo:   ["emit", "tests"]
}

@temporary {
    reason: "linear schedule scan until priority queue lands"
    expires-when: "PriorityQueue added to stdlib"
    added: "2026-06-17"
}

@invariant "rc-refcell-not-send" {
    description: "Rc<RefCell> is not Send; cooperative scheduling prevents re-entrancy"
    applies_to: [execute_one, run_scheduled]
    consequence: "spawn_parallel must deep-clone values at OS thread boundary"
    check: fn(ctx) { !ctx.is_parallel_context }
}

@errors {
    StepQuota:    likely
    StackUnderflow: possible
    BadJump:      rare
}
fn execute_one(thread) {
    match thread.ip {
        Halt(v)     => { return v }
        Push(val)   => { thread.stack.push(val) }
        Jump(addr)  => { thread.ip = addr }
    }
}
```

Every annotation here is:
- A typed CAST node (queryable, renderable)
- Checked by the compiler (W-WIP-001, W-TMP-001, E-EXH-001)
- Returned by `codebase.*` caps to agents at runtime

The source is its own session brief.

---

## Connection to Phase 1

Phase 1 answered: *"what is this code and what does it do?"*
Phase 2 answers: *"why was it written this way, what is temporary, what is in-flight, and how likely is each failure?"*

Together they make the source file the canonical working memory for any agent
or human picking up the codebase — across sessions, across time, across teams.
