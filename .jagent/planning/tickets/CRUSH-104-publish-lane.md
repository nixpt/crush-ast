# CRUSH-104 — Publish lane: version.workspace sweep + walker-core publish + walker rename

| Field | Value |
|-------|-------|
| **ID** | CRUSH-104 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | Publish |

## Problem

Per archived root TASKS (.jagent/planning/TASKS-root-archive-2026H1.md) Publish lane (verify counts at dispatch): only 9/35
crates use `version.workspace = true`; 6 crates (walker-core, cli/"walker",
go_walker, zig_walker, dart_walker, wasm_walker) hardcode a stale 0.1.0 vs
the workspace's 0.3.0. `walker-core` isn't on crates.io at all — blocking 10
dependent crates (crush-aot + 8 crush-lang-* + crush-aotc) from publishing.
And `crates/cli`'s package name `walker` is squatted on crates.io → needs the
`crush-walker` rename first.

## Approach

1. `version.workspace = true` sweep (mechanical; verify each crate builds).
2. Rename `walker` → `crush-walker` (package name; grep consumers + docs).
3. Publish `walker-core`, then the unblocked dependents in dependency order.
4. Do this AFTER CRUSH-36's trait unification settles which walker crates
   even survive (publishing a crate that M6 then retires is wasted API
   surface — the gate is real).

## Definition of done

- [ ] All 35 crates on workspace version; no 0.1.0 stragglers
- [ ] crush-walker rename landed; walker-core + dependents published in order
- [ ] Publish order + dry-run (`cargo publish --dry-run`) evidence quoted

## Files in scope

- Cargo.tomls across the workspace, crates.io publishing

## Gates

CRUSH-36 (crate-set stability). Consolidation from CRUSH-98 (wasm crates) also
lands first.
