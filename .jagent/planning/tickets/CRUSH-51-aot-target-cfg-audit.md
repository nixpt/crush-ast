# CRUSH-51 — AOT/installer target_os + target_arch cfg audit (3 sites)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-51 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | M8 |

## Problem

Three OS-cfg sites disagree on platform coverage (CRUSH-22's finding, expanded):
1. `crush-aot` compiler.rs: `.so`/`.dylib`/`.dll` branching
2. `crush-aotc` codegen.rs: unconditional `Command::new("cc")` (no cc on
   stock Windows)
3. `crush-installer` main.rs:466: its own separate Windows branch
Verify each site + current line numbers at dispatch. Silent disagreement means
"works on the OS the author used."

## Approach

One shared platform-info module (target_os/arch → lib extension, compiler
driver, install layout) consumed by all three; per-site unit tests; Windows
answer decided once (`cl.exe`? require MSYS? document unsupported?) and
recorded via dejavue decision, not implied differently in three places.

## Definition of done

- [ ] Shared module; all three sites consume it; disagreement impossible by
      construction
- [ ] Windows compiler-driver decision recorded
- [ ] Green on the CRUSH-49 matrix (or named xfails)

## Files in scope

- `crates/crush-aot`, `crates/crush-aotc`, `crates/crush-installer`, new shared module

## Gates

None; CRUSH-49 gives it CI teeth.
