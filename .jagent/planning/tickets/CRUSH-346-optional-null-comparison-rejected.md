# CRUSH-346 — `x != null` is a type error when `x` came from a nullable function

| Field | Value |
|-------|-------|
| **ID** | CRUSH-346 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | M2 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | S |

## Problem

A user function that can `return null` is inferred as `optional<any>`, and the
semantic checker then rejects the only natural way to test it:

```
[type] type error: Cannot compare types optional<any> and null
```

Meanwhile `conv.parse_int(...)` / `conv.to_float(...)` results (also nullable)
compare against `null` fine, so the restriction is specific to inferred
user-function returns. The workaround is to stop returning `null` from helpers
and return 0/1-element arrays or `{ok, value}` maps — which every library then
has to do, and which reads worse.

Found while building `facet` (facet.crush's `__facet_find_long` /
`__facet_find_short` lookups now return `[]` or `[entry]` purely to dodge
this).

## Reproduction

```bash
cat > /tmp/opt.crush <<'EOF'
fn find(entries, name) {
    for e in entries {
        if e.long == name { return e }
    }
    return null
}
fn main() {
    let x = find(["a"], "a")
    if x != null { print("found") } else { print("missing") }
}
EOF
crush-run run /tmp/opt.crush
# [compile] Cannot compare types optional<any> and null
```

Control: `let n = conv.parse_int("x", 10); if n == null { … }` compiles.

## Success criteria

- [ ] `== null` / `!= null` are allowed (and narrow) on values of type
      `optional<T>` wherever a host-cap result of the same shape is allowed.
- [ ] `if x != null { x.field }` type-checks without a cast.
- [ ] Regression tests cover inferred-optional returns, including from
      methods with multiple `return` sites (one null, one value).

## Technical approach

- Find the comparison rule in `crates/crush-frontend/src/semantics.rs` that
  rejects `optional<any>` vs `null` (grep `Cannot compare types`); permit
  `Optional(_) == Null` / `!=` and narrow the bound value in the true branch —
  mirroring whatever path lets `conv.parse_int`'s result through.
- Confirm whether declared/annotated types exist for return values; if so, an
  explicit `-> ?T` may already sidestep this and only inference is broken.

## Files to modify

- `crates/crush-frontend/src/semantics.rs` — comparison + narrowing rule.
- `crates/crush-frontend/tests/` — inferred-optional fixtures.

## Non-goals

- Full sum types / option unwrapping operators (`?.`, `??`).
- Changing what `type_of(null)` reports.
