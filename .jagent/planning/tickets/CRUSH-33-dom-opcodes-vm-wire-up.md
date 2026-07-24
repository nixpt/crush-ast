# CRUSH-33: DOM opcodes VM-side wire-up

| Field | Value |
|-------|-------|
| **Hash** | `df5f59f` (skeleton commit only; full ticket closure after Commit 2 + Commit 3 land) |
| **Status** | In Progress (skeleton landed `df5f59f`; Commit 2 5-tier wiring + Commit 3 differential pending) |
| **Assignee** | unassigned |
| **Why this exists** | CRUSH-32 established the schema for opcode-class wiring across the 5 execution tiers (scheduler, portable_vm, fastvm, AOT-Rust, AOT-C) by landing 10 AI opcodes at slots 0x90-0x99. CRUSH-33 extends that schema to Document Object Model (DOM) opcodes — the next M5 ticket in the build-order diagram. Without this, no .crush program can manipulate DOM nodes; without the per-tier wiring identical to AI's, the differential harness will detect VM-vs-AOT divergence on any DOM-touching program. |
| **Scope** | Land 10 DOM opcodes through three commits. Commit 1 (this ticket landing) is the SKELETON: bytecode slot reservation (0x9A-0xA3), `dom_native.rs` module surface (KINDS constant + macro + register() stub + cap struct stubs), lib.rs `pub mod dom_native`, basic unit tests. Commit 2 wires the skeleton into the 5 tiers (scheduler + portable_vm cap-call, fastvm `HostRequest::DomX` variants + resolve_host_request dispatch extension, AOT-Rust + AOT-C inline stub emit per opcode). Commit 3 extends `differential.rs` with the DOM equality + KINDS surface-stability tests. |
| **Done condition** | All 3 commits landed on `agent/buffy/M2-JIT-PHASES-2-4`; `KINDS.len() == 10`; all 5 tiers agree at observable-behavior level on a tiny DOM-touching program; differential harness shows zero divergences on the DOM surface; ticket marked Done with `Closed by:` row referencing the implementation SHA. |
| **Out of scope** | Real DOM backends (jsdom, headless-chrome, browser-driver) — these are placeholders that produce self-documenting `{ok, kind, echo}` stubs until real backends land in a later milestone, mirroring how `ai_native.*` shapes are stable but the LLM impls are deferred. Bytecode slot reservation beyond 0xA3 (that range is reserved for future MATH/VEC extensions already at 0xA0-0xA8). Polyglot `@javascript` block integration (DOM is a host-level concept, not a polyglot one — see CRUSH-20 for the polyglot surface). |
| **Test plan** | Per the CRUSH-32 schema. Commit 1: `dom_native_kinds_constant_is_size_ten_and_sorted_unique` (size + no-dup), 10x `spec_names_match_kinds_constant` (one per macro cap), `register_inserts_all_10_handlers`, `stub_map_includes_kind_ok_echo` (matches ai_native's shape exactly). Commit 2: identical cargo check / cargo test pattern on the touched crates. Commit 3: parametric differential fixture (loops KINDS, asserts observable-behavior A-vs-B agreement). |
| **Build-order position** | Next-after-CRUSH-32 in M5. Direct dependents after this lands: CRUSH-34 (spans opcodes — already filed), CRUSH-35+ (M6). |

## Architecture verdict (locked)

### A. DOM opcode surface (v1)

| Kind | Rationale | Byte slot |
|------|-----------|-----------|
| `query` | CSS selector → matched nodes | 0x9A |
| `get` | one element, by id/path | 0x9B |
| `set` | modify element attribute/property | 0x9C |
| `create` | new element | 0x9D |
| `remove` | delete element | 0x9E |
| `child` | tree navigation, get children | 0x9F |
| `parent` | tree navigation, get parent | — see B |
| `attr` | attribute get/set, distinct from `set` | — see B |
| `text` | text content get/set | — see B |
| `event` | event listener attach | — see B |

10 opcodes — exactly matches the AI count (0x90-0x99), keeps the surface quantities symmetric for the differential harness count assertions.

### B. Byte slot assignment

Contiguous `0x9A` through `0xA3` (10 slots).

Rationale:
- AI exhausted at 0x99 — DOM picks up at 0x9A (next contiguous).
- 0x9A-0xA3 stops BEFORE `MATH_POW` at 0xA0 — wait, that collides. The existing bytecode tables in `crush-vm/src/bytecode.rs` show `MATH_POW = 0xA0`, `MATH_SQRT = 0xA1`, etc. So 0xA0-0xA8 is MATH/VEC territory.

**REVISED assignment (correct for the existing slot table):**

- 0x9A-0x9F: DOM_QUERY, DOM_GET, DOM_SET, DOM_CREATE, DOM_REMOVE, DOM_CHILD (6 slots)
- 0xB5-0xB8: DOM_PARENT, DOM_ATTR, DOM_TEXT, DOM_EVENT (4 slots — `STR_TO_UPPER` is at 0xB2 through `STR_TRIM` at 0xB4, leaving 0xB5-... free)

Wait — `STR_TO_UPPER = 0xB2, 0xB3, 0xB4` (3 slots). The next free range is 0xB5+.

Actually, looking again at the slot allocator logic, the existing table reserves ranges of 0x10 (16 slots) per category for forward-compatibility. The byte-slot rule for new categories seems to be: pick the next contiguous range that doesn't collide.

**FINAL slot assignment:**
- DOM_QUERY    = 0x9A
- DOM_GET      = 0x9B
- DOM_SET      = 0x9C
- DOM_CREATE   = 0x9D
- DOM_REMOVE   = 0x9E
- DOM_CHILD    = 0x9F
- DOM_PARENT   = 0xB5
- DOM_ATTR     = 0xB6
- DOM_TEXT     = 0xB7
- DOM_EVENT    = 0xB8

These ranges don't collide with MATH (0xA0-0xA8), VEC (0xA6-0xA8 — overlap concern: 0xA6 = VEC_ADD conflicts with nothing since DOM leaves that block free), STR_ (0xB0-0xB4). The split into two ranges (0x9A-0x9F and 0xB5-0xB8) tracks the natural surface split between "DOM node CRUD" (read/write single node, tree navigation backward) and "DOM tree text/event" (parent navigation, attribute API, text API, event API).

This split will be reflected in the `dom_native_kind_for_opcode(opcode: u8) -> Option<&'static str>` switch in `bytecode.rs`.

### C. Module structure

- New file: `crates/crush-lang-sdk/src/dom_native.rs`
- Mirror `ai_native.rs` exactly: KINDS const, `register()` fn, `dom_native_cap!` macro, 10 macro-generated `DomNative*Cap` structs, pub + test mod.
- `crates/crush-lang-sdk/src/lib.rs` — add `pub mod dom_native;` alongside `pub mod ai_native;`.

### D. Host gate names

`dom_native.<kind>` — mirrors `ai_native.<kind>` precedent (consistency with the registry surface). The dot-namespaced gate names keep DOM-side and AI-side caps discoverable through the same prefix `*_native`.

### E. First-commit scope (SKELETON)

Land now in Commit 1:
- This ticket file (`Backlog → In Progress` on land).
- `crates/crush-vm/src/bytecode.rs`: 10 new slot constants (`DOM_QUERY..DOM_EVENT`), the `dom_native_kind_for_opcode()` switch function, `operand_kind(opcode)` extensions for the 10 new ops.
- `crates/crush-lang-sdk/src/dom_native.rs`: full surface — KINDS const, `register()`, `dom_native_cap!` macro, 10 macro-generated stub Cap structs, `#[cfg(test)] mod tests` with the 6 unit tests listed in Test plan above.
- `crates/crush-lang-sdk/src/lib.rs`: add `pub mod dom_native;`.
- All-caps: NO VM wiring in scheduler.rs, NO fastvm HostRequest variants, NO portable_vm cap-call, NO AOT inline stub emit — those land in Commit 2. Commit 1 is purely "surface + registration + tests", and the registration test (`register_inserts_all_10_handlers`) needs the AI tests to remain GREEN.

Rationale for this skeleton-first split:
- Same precedent as CRUSH-32 itself: landing the surface first lets the harness COUNT the surface (KINDS == 10, all cap names match) without forcing every implementer to wire bytecode.rs + scheduler.rs + portable_vm.rs + fastvm/types.rs + codegen.rs + codegen_c.rs in one commit (which would be a 7-file commit with a single load-bearing failure surface).
- The skeleton alone is testable: `cargo check -p crush-lang-sdk --tests` proves the KINDS const + macro + register() work. The full 5-tier wiring is then mechanical (mirror CRUSH-32's commits `1bd01dc` for the impl, `b19a397` for the followup).
- This decomposition also gives a clean review-per-commit boundary: reviewers can ack the surface + tests independently of the wiring.

### F. (Commit 2) resolve_host_request extension

`pub fn resolve_host_request(req: &HostRequest, host_caps: Option<&HostCaps>) -> Option<RuntimeValue>` gains 10 arms:
```rust
HostRequest::DomQuery    { .. } => ("dom_native.query",     "query"),
HostRequest::DomGet      { .. } => ("dom_native.get",       "get"),
HostRequest::DomSet      { .. } => ("dom_native.set",       "set"),
HostRequest::DomCreate   { .. } => ("dom_native.create",    "create"),
HostRequest::DomRemove   { .. } => ("dom_native.remove",    "remove"),
HostRequest::DomChild    { .. } => ("dom_native.child",     "child"),
HostRequest::DomParent   { .. } => ("dom_native.parent",    "parent"),
HostRequest::DomAttr     { .. } => ("dom_native.attr",      "attr"),
HostRequest::DomText     { .. } => ("dom_native.text",      "text"),
HostRequest::DomEvent    { .. } => ("dom_native.event",     "event"),
```

`HostRequest` itself grows 10 new variants in `crates/crush-vm/src/fastvm/types.rs`, mirroring the `AiX` variants.

### G. (Commit 2) Stub Map / Object shape

Same `{ok: true, kind, echo: [<args>]}` family as AI. DOM-specific enrichment (e.g., a placeholder `node` field carrying an opaque DOM-handle) is OUT OF SCOPE for the stub phase — real backends will need richer shapes, deferred per the same "stub phase / real backend phase" separation CRUSH-32 established.

### H. Risk look-ahead

Hardest opcodes to wire when real backends arrive (post-stub phase):
1. `dom_event` (#10, hardest) — requires callback registration lifecycle; cross-thread coordination with the green-thread scheduler. Recommend shipping last, and ONLY after `dom_remove` settles the GC question (a removed element with active listeners needs handle cleanup).
2. `dom_create` + `dom_set` (#5 + #3, medium) — shadow DOM mutation in real backends needs parent-child integrity invariants.
3. `dom_query` (#1, easiest) — CSS selector stub already returns empty array; real backend just dispatches to `document.querySelectorAll` or jsdom equivalent.

Suggested order for the eventual real backend phase: `dom_query → dom_get → dom_attr → dom_text → dom_set → dom_create → dom_remove → dom_child → dom_parent → dom_event`.

## Open questions (none blocking Commit 1)

- Will the Crush frontend wire `dom_<kind>(args)` syntax-level builtin calls before or after the VM-side wiring lands? (Same UX blocker as `ai_<kind>(...)` syntax — see CRUSH-32 impl commit message and the deferred parametric test in `differential.rs`.) Mirror the deferred-comment pattern.
- Should `dom_native.*` caps register automatically on `HostCapsBuilder::new()` (similar to `ai_native(bool)` toggle) or only on explicit `dom_native::register(&mut caps)` call? Recommend: start with explicit register, add `HostCapsBuilder::dom_native(bool)` toggle in Commit 2 mirroring the ai_native one.
- Does `dom_remove` need to invalidate `HostRequest::DomGet`-returned handles? Real backend only; out of scope.

## Will-not-fix (deferred forever)

- DOM handle interop with the SCHED-20 polyglot `@javascript` block (which already has its own DOM in jsdom if real backend ever wires it). DOM ops go through `dom_native.*` caps exclusively; `@javascript` is a separate surface. Cross-surface invariants are TBD in a later milestone.

## Reviewed forward flags (Commit 2 attention)

Lifted from the CRUSH-33 Commit 1 code review (`df5f59f`, verdict
LAND-AS-IS). None blocking, but worth addressing before Commit 2 lands:

1. **Two sources of truth for KINDS ↔ byte-slot mapping.** `KINDS`
   lives in `dom_native.rs` in surface-natural order;
   `dom_native_kind_for_opcode` lives in `bytecode.rs` as a hand-curated
   switch. Reordering KINDS in any future commit silently desyncs the
   slot mapping. **Recommend**: collapse into a single
   `const DOM_OPS: &[(u8, &str); 10]` table and convert both helpers to
   indexed lookups, before Commit 2's per-tier arms re-lock the
   surface.

2. **OperandKind::Dom shape parity with OperandKind::Ai (if present).**
   If `Ai` carries `kind: u8`, the mirror holds; if `Ai` uses a string
   or enum-tag representation, `Dom` now diverges silently. **Confirm**
   the existing `OperandKind::Ai` representation in `bytecode.rs`
   before Commit 2 wires the encoder/decoder.

(Full 6 findings noted in review; remaining 4 are doc-comment polish +
an `echo` stub enrichment suggestion, deferred to Commit 2.)
