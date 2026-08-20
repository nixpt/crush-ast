# CRUSH-112 — dotted `array.*`/`str.*`/`math.*` builtins compile to unregistered capabilities

| Field | Value |
|-------|-------|
| **ID** | CRUSH-112 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | M1 |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-111 (shared registry); see CRUSH-113 for the `stdlib` feature |
| **Estimated effort** | M |

## Problem

Dotted builtin calls parse as `CapabilityCall` and are lowered to
`cap_call "<name>"`, but several of those names are not in the portable
registry (`crush-vm/src/caps.rs` only has `str.concat`/`str.len`/
`str.contains`/`str.split`/`str.replace`/`str.join` plus `append`/`push`/
`arr_set`/`arr_get`/`arr_slice`/`make_range`).

- `array.push` / `array.pop` are not registered **anywhere** in the standalone binary.
- `str.starts_with` / `str.ends_with` / `str.to_upper` / `str.to_lower` / `str.trim` and `math.pow` / `math.sqrt` / `math.abs` / `math.round` / `math.floor` / `math.ceil` exist only behind the `stdlib` feature (CRUSH-113).

The compiler's opcode fast-paths for these (`array_push`, `array_pop`,
`str_starts_with`, `math_pow`, …) live in `compile_call`, which is unreachable
from `.crush` source because dotted names never produce a `Call` expression —
they always become `CapabilityCall`, which is lowered straight to `cap_call`.

## Impact

`array.push`/`array.pop` (the natural stack ops for array-backed state) and a
whole family of string/math builtins compile cleanly and only fail **at
runtime** with "unknown capability". This is the exact trap the `awesome-crush`
interpreters hit while building array-backed stacks.

## Reproduction

```bash
printf 'fn main() { let a = [1,2,3]; let x = array.pop(a); io.print(x); return 0; }\n' > /tmp/p.crush
crushc /tmp/p.crush -o /tmp/p.cvm1 && crush-run run /tmp/p.cvm1 --cap io.print --cap array.pop
# [runtime] unknown capability: array.pop

printf 'fn main() { io.print(str.starts_with("abc", "a")); return 0; }\n' > /tmp/s.crush
crushc /tmp/s.crush -o /tmp/s.cvm1 && crush-run run /tmp/s.cvm1 --cap io.print --cap str.starts_with
# [runtime] unknown capability: str.starts_with

printf 'fn main() { io.print(math.sqrt(9)); return 0; }\n' > /tmp/m.crush
crushc /tmp/m.crush -o /tmp/m.cvm1 && crush-run run /tmp/m.cvm1 --cap io.print --cap math.sqrt
# [runtime] unknown capability: math.sqrt
```

## Success criteria

- [ ] `array.push`/`array.pop` work from source (registered as caps, or lowered to the existing `array_push`/`array_pop` opcodes).
- [ ] `str.starts_with`/`str.ends_with`/`str.to_upper`/`str.to_lower`/`str.trim` and `math.*` work from source in the default build — or fail at **compile** time, not runtime, if they genuinely require a feature.
- [ ] Any dotted name that isn't a registered capability errors at compile time instead of surfacing a runtime "unknown capability".

## Technical approach

- Register `array.push`/`array.pop` in the portable registry, **or** route dotted `array.*`/`str.*`/`math.*` names through `compile_call`'s existing opcode fast-paths instead of `cap_call`.
- Align with CRUSH-111/CRUSH-65: a single shared builtin registry so names can't silently fall through to an unregistered `cap_call`.
- Add a compile-time (or manifest-load) validation that every emitted `cap_call` name is in the host registry, so misrouted names fail loudly at build time.

## Files to modify

- `crates/crush-vm/src/caps.rs` — portable registry completeness.
- `crates/crush-frontend/src/parser/mod.rs` and/or `compiler.rs` — route dotted builtins to opcodes.
- `crates/crush-lang-sdk/src/` — stdlib host-cap wiring (cross-ref CRUSH-113).

## Non-goals

- New string/math semantics beyond what the opcodes already implement.
- The `--stdlib` feature-default question itself (CRUSH-113).
