# CRUSH-53 — crush-installer: Pi-class (aarch64) default target + smoke test

| Field | Value |
|-------|-------|
| **ID** | CRUSH-53 |
| **Priority** | P3 |
| **Status** | Backlog |
| **Phase** | M8 |

## Problem

ROADMAP M8: Pi-class (`aarch64-unknown-linux-gnu`) should be a first-class
install target for embedded use. Verify crush-installer's current target list
at dispatch (crate exists, pre-dates the roadmap; no Pi work in git log per
s412 triage). ROADMAP's "gnueabihf" spelling is a 32-bit-ism — the ticket
standardizes on aarch64 (64-bit Pi 3+/4/5); note the correction.

## Approach

Add aarch64 to the installer target list (consuming CRUSH-51's shared platform
module); produce install artifacts from the CRUSH-50 lane; smoke test =
install + `crush-run` a hello fixture on real aarch64 (qemu-user acceptable,
labeled; real-Pi run manual, documented).

## Definition of done

- [ ] aarch64 installable; smoke green (qemu labeled if so)
- [ ] Target list documented; gnueabihf correction noted

## Files in scope

- `crates/crush-installer`

## Gates

CRUSH-50, CRUSH-51.
