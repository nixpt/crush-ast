# CRUSH-99 — Cross-language inlining PoC (Python → inlined JS fn inside a C-codegen .so)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-99 |
| **Priority** | P3 |
| **Status** | Backlog |
| **Phase** | M11 |

## Problem

The catalyst thesis ("write in your language, ship at C speed") needs its
first cross-language proof: two distinct walker inputs (a Python function
calling a JS function) compiled into ONE native .so with the call inlined at
the CAST/CASM level — no subprocess, no marshaling at the seam.

## Approach

PoC-honest scope: one Python file + one JS file, both lowered to CAST,
linked into one program (shared function table), AOT-C compiled; the
cross-language call inlined (or at minimum direct-called — measure both, say
which landed). Correctness = differential vs running the pair under
PortableVm. Success metric written before code: correct output + call
overhead below the polyglot-subprocess baseline by an order of magnitude.

## Definition of done

- [ ] The pair compiles to one .so, runs correctly, differential-green
- [ ] Overhead vs EXEC_LANG subprocess path measured + quoted
- [ ] What was inlined vs direct-called stated plainly; follow-ups filed

## Files in scope

- `crates/crush-aotc` multi-input, `crush-frontend` program linking

## Gates

CRUSH-98 (chain proven), CRUSH-103.
