# CRUSH-255 — publish crush-vm 0.4.1 so embedders share one `HostCap` trait

| Field | Value |
|-------|-------|
| **ID** | CRUSH-255 |
| **Priority** | P2 — unblocks exo-light inside surfer-browser |
| **Status** | Backlog |
| **Phase** | M0 — release hygiene |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | S |

## Problem

crates.io has crush-vm **0.4.0**; the workspace is at **0.4.1** (`Cargo.toml:58`), which surfer-browser
consumes by path. exo-light (crates.io consumer, currently `0.3.6`, moving to 0.4 in exo-light EXOLIGHT-12)
and surfer must resolve the *same* crush-vm crate for surfer's browser/DOM `HostCap`s to be accepted by
`ExoLightRuntime::with_cvm_host_caps` — a path copy and a registry copy are distinct crates even at the same
version.

## Success criteria

- [ ] crush-vm 0.4.1 (and the crates it needs: casm, crush-errors, …) published, with `HostCaps::register_shared`
      (`crates/crush-vm/src/host.rs:140`) in the released API.
- [ ] CHANGELOG notes the `HostCap`/`HostCaps` API surface embedders rely on (`register`, `register_shared`,
      `names`, cheap `Clone`) as stable for 0.4.x.
- [ ] Downstream note for path consumers: use `[patch.crates-io] crush-vm = { path = ... }` so registry
      dependents (exo-light) unify with the path copy.

## Cross-refs

exo-light EXOLIGHT-12; surfer-browser SURF-EXOLIGHT-1.
