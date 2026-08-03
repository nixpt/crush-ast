# CRUSH-48 — exo.* capability module layer (io/fs/process/net/env)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-48 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | M7 |

## Problem

Exosphere-side programs expect an `exo.*` namespace (`exo.io`, `exo.fs`,
`exo.process`, `exo.net`, `exo.env`); crush-lang-sdk has zero `exo.` surface
today (s412 triage). The layer is pass-through mediation over the existing
`io.*`/`fs.*`/etc. caps — same operations, exosphere-canonical names, with
mediation rules (a capsule gets exactly what it was granted; nimbus's
capability-boundary doctrine).

## Approach

Register `exo.*` cap providers in crush-lang-sdk delegating to the existing
impls; mediation table = one place mapping exo-name → underlying cap +
required grant. No new I/O implementations. Tests: grant-present passes
through; grant-absent denied with the exo-name in the error; parity test that
`exo.io.print` ≡ `io.print` behavior.

⚠ Contract note: this is the crush-ast↔exosphere seam — coordinate naming
with the exosphere convergence work (CRUSH-55 / EXO-194) so the namespace
matches what exosphere's frozen in-tree crush already exposes; flag any
mismatch rather than inventing names.

## Definition of done

- [ ] exo.* registered + delegating with mediation table committed
- [ ] Grant/deny + parity tests green
- [ ] Naming verified against exosphere's actual surface (cite files)

## Files in scope

- `crates/crush-lang-sdk` (cap provider), docs/design mediation table

## Gates

Naming check vs exosphere (read-only) before landing.
