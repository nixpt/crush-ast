# CRUSH-251 — VM primitives for a JS object model: prototype lookup, closures, `this`, `new`

| Field | Value |
|-------|-------|
| **ID** | CRUSH-251 |
| **Priority** | P1 — blocks real-page JS on crush (bozo frontend) |
| **Status** | Backlog |
| **Phase** | M6 |
| **Assignee** | unassigned |
| **Dependencies** | none (consumed by bozo BOZO-M7-1/M7-2) |
| **Estimated effort** | L |

## Problem

Today JS is lowered by name, not by semantics: methods become string calls with no receiver
(`lower_swc.rs:1252-1263`), `new X()` calls a function named `"new X"` (`:779-794`), lambdas are
lifted to named functions **with no captured environment** (`crush-frontend/src/compiler.rs:2047+`),
and `await` is "MVP: just compile the expression" (`compiler.rs:1987-1991`). CASM has
`GetField`/`SetField` (`crates/casm/src/lib.rs:165-166`) but no prototype chain, no closure cells,
no receiver-carrying call. Without these, no frontend can run ordinary page JS (every framework uses
closures, `this` and prototypes).

## Success criteria

- [ ] CASM/VM ops (or a documented runtime-library equivalent) for:
  - property get/set with **prototype-chain lookup** (own → proto → … → null), `in`, `delete`;
  - **closures**: captured-variable cells (upvalues) shared between closures and the defining frame;
  - **receiver calls**: `call_method(obj, key, args)` binding `this`; `construct(F, args)` (`new`) creating
    an object whose proto is `F.prototype`;
  - first-class function objects carrying `prototype`, `length`, `name`.
- [ ] Implemented in CVM1 (portable) and FastVM first; JIT/AOT may initially `Unsupported` → fall back
      (same pattern as CRUSH-72).
- [ ] Differential tests across the implemented tiers (crush-diff harness).
- [ ] A design note: value representation of JS objects vs existing `Value::Map`, GC/ownership of
      cyclic object graphs (arena vs refcount+cycle collector), and how far this reaches into AOT.

## Technical approach

- Prefer a small number of general ops (`get_prop`, `set_prop`, `call_method`, `construct`,
  `make_closure`, `load_cell`, `store_cell`) over JS-specific sugar; other walkers (Python
  attribute lookup, Ruby-ish dispatch) can reuse them.
- Cycles: JS object graphs are cyclic; `Rc<RefCell<..>>` maps leak. Decide arena + periodic mark
  (per-VM) — see crush-jit's conservative-GC plan.
- Keep it engine-agnostic: no DOM, no event loop here (event loop is CRUSH-254).

## Files to modify

- `crates/casm/src/lib.rs` — op definitions
- `crates/crush-vm/src/{bytecode.rs,portable_vm.rs,fastvm/*}` — execution
- `crates/crush-frontend/src/compiler.rs` — emit for CAST closures/methods (generic, not JS-only)
- `docs/design/object-model.md` — new

## Cross-refs

bozo `BOZO-M7-1`, `BOZO-M7-2`; surfer-browser `SURF-ENGINE-2`.

## Sequencing note (2026-10-15, owner decisions)

**Closures / function values are the first slice, ahead of the rest of this ticket.** Besides bozo's JS object
model, they now block: Python `lambda` and nested `def` closures (deferred from the Python e2e parity
push, #184 / CRUSH-zorro-2), JS arrow-function closures, and **#175 `parallel_map`**, whose closure form waits
for this ticket (decision recorded in `.dejavue/decisions.md`). The owner noted that higher-order functions
are still needed. Prototypes / `this` / `new` can follow as later slices.
