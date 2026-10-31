# CRUSH-zorro-38 — Python walker silently drops call keyword arguments

| Field | Value |
|-------|-------|
| **ID** | CRUSH-zorro-38 |
| **Priority** | P0 |
| **Status** | Done — PR #209 merged 2026-10-15 |
| **Phase** | Python parity |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | S (loud) / M (supported builtins) |
| **Filed by** | vega — 2026-10-15, found during the object-model survey (#206), reproduced on `origin/main` `c864367` |

## Problem

`crates/crush-lang-python/src/lower_expr.rs` `lower_call(…, _keywords, …)` **ignores every keyword argument**,
so the call runs as if the keywords weren't there:

```python
print('a', end='')
print('b')
print('c', 'd', sep='-')
```

CPython prints `ab` / `c-d`. Crush prints `a` / `b` / `c d`: silently wrong. The same happens for any call with
keywords (`sorted(xs, key=f, reverse=True)` once `sorted` exists, user functions called with `f(x=1)`, …).

## Success criteria

- [ ] **No call silently ignores a keyword argument.** Every keyword is either supported or a walk-time error naming
      the function and the keyword ("keyword argument `end=` to `print` is not supported yet").
- [ ] `print`'s `sep=` and `end=` are supported exactly (CPython output), as the most common case. `file=` and
      `flush=` are loud, or `flush=` is accepted as a no-op if output is unbuffered (document which).
- [ ] User-defined functions called with keyword arguments either bind them to parameters by name (with defaults),
      or are loud until parameter binding by name exists.
- [ ] cpython_corpus programs for `print(…, end=…)`, `print(…, sep=…)` and a keyword call to a user function;
      python_parity AOT exact-or-loud.

## Technical approach

- Make the `_keywords` parameter used: match the callee. For a known builtin, consult a small table of supported
  keywords; for anything else, `bail!` with the keyword's name.
- `print`: lower `sep`/`end` into the existing `py.print` capability (an extra argument or a sibling capability),
  mirrored in the AOT runtimes like the other `py.*`/`js.*` capabilities.
- Keyword binding for user functions is a natural follow-up (default values are already loud: CRUSH-zorro-2
  follow-ups).

## Non-goals

- `**kwargs` / `*args` forwarding (keep loud).

## Survey 2026-10-31 (status)

PR #209 (`agent/vega/CRUSH-zorro-38-python-kwargs`) merged 2026-10-15. The ticket file was not updated by that PR. Previous status: `Backlog`.
