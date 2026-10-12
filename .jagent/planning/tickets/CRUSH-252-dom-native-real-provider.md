# CRUSH-252 — `dom_native.*`: real DOM provider via a shared `DomBridge` contract

| Field | Value |
|-------|-------|
| **ID** | CRUSH-252 |
| **Priority** | P1 — CRUSH-33 opcodes are wired but only echo |
| **Status** | Backlog |
| **Phase** | M5 |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-33 (done) |
| **Estimated effort** | M |

## Problem

CRUSH-33 landed 10 DOM opcodes (`query/get/set/create/remove/child` 0x9A-0x9F,
`parent/attr/text/event` 0xB5-0xB8; `crates/crush-vm/src/bytecode.rs:116+`) through all 5 tiers,
but `crates/crush-lang-sdk/src/dom_native.rs` caps return `{ok, kind, echo}` stubs and the "real
backend" was left out of scope. Meanwhile two other DOM contracts exist: bozo's `DomBridge`
(`op(cmd, &[String]) -> String`, 35 ops, `bozo/bozo/src/dom.rs:54-60`), which mirrors surfer's
real bliss-dom bridge (`surfer/src/browser/js.rs` `op_dom_inner`), and exosphere crush-python's
`DomMutation` batches. Three contracts, one real DOM.

## Success criteria

- [ ] A `DomBridge` trait lives in **crush-lang-sdk** (`crush_lang_sdk::dom`), the lowest crate both bozo and
      surfer already depend on (bozo → crush-ast, so it cannot live in bozo without a cycle). Op names,
      argument order and string node-id handles = the existing bozo/surfer contract.
- [ ] `dom_native.<kind>` caps delegate to an injectable `Arc<dyn DomBridge>` registered on `HostCaps`;
      with none registered they fail closed (`ok:false, error:"no dom provider"`) instead of echoing.
- [ ] Mapping table `dom_native.<kind>` → bridge ops documented (CRUSH-33's 10 are typed sugar over the 35).
- [ ] `TestDomProvider` (in-memory tree) + tests for each of the 10 kinds across tiers; the differential
      harness keeps passing.
- [ ] `dom.op` (generic string cap used by bozo) is registered from the same provider.

## Technical approach

- Move the trait from bozo into crush-lang-sdk; bozo re-exports it (bozo `BOZO-M7-4`); surfer
  implements it (surfer `SURF-DOMBRIDGE-2`).
- Event registration (`DOM_EVENT`) needs a callback path back into the VM — define it as
  "register handler id; host enqueues a job" and leave dispatch to the event-loop primitive (CRUSH-254).

## Files to modify

- `crates/crush-lang-sdk/src/{dom.rs (new),dom_native.rs,lib.rs}`
- `crates/crush-lang-sdk/src/differential.rs` — provider-backed cases

## Cross-refs

CRUSH-33, CRUSH-254; bozo `BOZO-M7-4`; surfer-browser `SURF-DOMBRIDGE-2`.
