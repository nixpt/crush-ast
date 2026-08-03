# CRUSH-47 — Embedded RustPython lane: runtime="rustpython", no host Python

| Field | Value |
|-------|-------|
| **ID** | CRUSH-47 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | M7 |

## Problem

`@python` blocks need a host Python (or CRUSH-20's bucket-provisioned one).
Hermetic CI and sandboxed-polyglot use want a zero-host-dependency lane.
Current state (s412 triage): `rustpython-parser` is used for PARSING only
(crush-lang-python/Cargo.toml:20-21); there is no embedded RustPython VM
runtime option. Design intent: docs/design/crushvm-rustpython.md
("crushpy-* profiles").

## Approach

`runtime = "rustpython"` option in the python lane: embed the RustPython VM
(feature-gated — it's a heavy dep; same build-isolation bar as CRUSH-45),
route `@python` execution through it when selected, host surface mediated by
the same capability gates as EXEC_LANG (CRUSH-2's polyglot_gate applies).
Document the compatibility seam honestly (RustPython ≠ CPython; stdlib gaps)
— opt-in divergence, never silent. Lane router: explicit per-program/manifest
selection among host-python / buckets-sandboxed (CRUSH-20) / rustpython.

## Definition of done

- [ ] `@python` fixture runs with runtime="rustpython" on a machine with no
      python3 (CI job proves it)
- [ ] polyglot_gate + quotas enforced in the lane (tests)
- [ ] Compat seam documented; default builds free of the dep

## Files in scope

- `crates/crush-lang-python`, `crush-vm` EXEC_LANG router, Cargo features

## Gates

None hard. Coordinate with CRUSH-20's lane selection.
