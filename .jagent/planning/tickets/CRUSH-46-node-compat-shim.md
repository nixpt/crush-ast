# CRUSH-46 — Node.js compat shim: require('http') subset over CAP_CALL

| Field | Value |
|-------|-------|
| **ID** | CRUSH-46 |
| **Priority** | P3 |
| **Status** | Backlog |
| **Phase** | M7 |

## Problem

JS programs written for Node can't run: no module resolution for Node
builtins. ROADMAP scopes a deliberate SUBSET shim — `require('http')` (and
what falls out cheaply, e.g. parts of `fs`/`path`) mapped onto existing
capability calls, with gaps documented rather than faked.

## Approach

Shim layer in the JS lane resolving a small allowlisted builtin set to
capability-backed impls (`http` → NET caps, `fs` → fs caps — mediated, so the
capability system still gates everything). Unsupported builtin → clear error
naming the module (never a stub that pretends — the CRUSH-84/85 lesson).
`docs/design/node-compat.md` gap table is a first-class deliverable.

## Definition of done

- [ ] A small real Node http example runs via the shim under capability mediation
- [ ] Unsupported builtins error loudly with module name (test)
- [ ] Gap table committed; capability mediation asserted in tests (denied cap → denied request)

## Files in scope

- `crates/crush-lang-js` (or sdk shim module), docs/design

## Gates

None hard; benefits from CRUSH-40 (NET timeout).
