# CRUSH-68 — Typed polyglot marshaling both ways (JSON env inject + sentinel return)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-68 |
| **Priority** | P1 — silent wrong-type / crash class for non-scalar polyglot inputs |
| **Status** | Done (PR pending) |
| **Phase** | Correctness / polyglot |
| **Assignee** | nixp |
| **Dependencies** | CRUSH-18 ✅, CRUSH-20 ✅ (sentinel scan shared); Python rewrite already in `crush-lang-sdk` |
| **Estimated effort** | M |

## Problem

`@python` guest code (under `polyglot-python`) does `json.loads(os.environ[name])`
and returns via `CRUSH_RESULT_SENTINEL` + `json.dumps`. That protocol is sound.

But `EXEC_LANG` still injects with `Value::as_text()` / `value_to_text` (Display),
not `serde_json::to_string`. Ints/bools accidentally round-trip because their
Display form is valid JSON. **Strings, arrays, and maps do not** —
`json.loads("hello")` fails; `[1, 2]` Display may parse but Maps/`List[...]`
forms will not.

CRUSH-20 explicitly deferred wiring typed inject into production EXEC_LANG.
Same silent-wrong-answer / loud-crash class as CRUSH-65.

JS/bash have **no** guest rewrite at all — only Python.

## Reproduction

```crush
fn main() {
    let msg = "hello";
    @python {
        result = msg + "!"
    }
    print(result);
}
```

Expected: prints `hello!`.  
Actual (before fix): Python `json.loads` fails on env value `hello` (not a JSON string).

Ints still "work", which is why this shipped unnoticed.

## Success criteria

- [x] EXEC_LANG inject uses `serde_json::to_string(&Value)` (shared helper; both scheduler + portable)
- [x] Serialize failure is loud (not Display fallback)
- [x] `@python` string / array / map inputs round-trip end-to-end
- [x] `@javascript` gets the same rewrite shape (JSON env → locals; sentinel dump of bound output)
- [x] Existing int/float Python polyglot tests still green
- [x] Bash: document string-only / no typed rewrite in v1 (non-goal below)

## Non-goals

- Shared heap / live mutation across Crush↔guest
- FastVM Arena `Ref` marshaling
- Bash typed inject (no `jq` dependency in v1)
- PyPI/npm deps (CRUSH-66)
- Changing `Value`'s Serialize lossiness for Handle/Bytes/NaN

## Technical approach

1. Add `scheduler::value_to_polyglot_env(val) -> Result<String, VmError>` → `serde_json::to_string`.
2. Replace `as_text` / `value_to_text` at both EXEC_LANG env-build sites.
3. Add `crush_lang_js::analyzer::free_variables` (swc, mirror Python's FreeVars).
4. Feature `polyglot-javascript` on crush-lang-sdk (optional dep crush-lang-js); `rewrite_javascript_marshaling` + wire in `prepare_stmts`.
5. Tests in `compile.rs` for Python string/array/map; one JS e2e if `node` on PATH.

## Resolution

Shipped on `agent/nixp/CRUSH-68`:

1. `scheduler::value_to_polyglot_env` — `serde_json::to_string`; both EXEC_LANG
   inject sites (scheduler + portable) use it.
2. `crush_lang_js::freevars` + `rewrite_javascript_marshaling` behind
   `polyglot-javascript` (default-on with `polyglot-python`).
3. E2E: Python string/array/map + JS string round-trips.

Bash remains string-only (no rewrite) — documented non-goal.
