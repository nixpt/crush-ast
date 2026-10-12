# CRUSH-253 — Python walker → DOM ops ("python-dom")

| Field | Value |
|-------|-------|
| **ID** | CRUSH-253 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | M6 |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-252 (real dom provider), CRUSH-251 (method calls / receivers) |
| **Estimated effort** | M |

## Problem

surfer-browser's UI still advertises a "Python DOM Demo" capsule (`surfer/src/capsules/manager.rs:60-70`),
which opens a blank tab: its `ui.html`/`logic.py` never existed. The only real Python-DOM code is
exosphere `crates/platform/runtimes/python/src/dom.rs` (1077 LOC, **PyO3/CPython only**, orphaned since
bliss-core retired; exosphere is being moved away from). crush-ast's Python walker
(`crates/crush-lang-python`, rustpython-parser 0.4, 49 tests) has no DOM surface. (Note: crush-ast's
`crates/crush-python` is unrelated — CAST bindings *for* Python.)

## Success criteria

- [ ] A Python `document`/element API — port the **API surface** of exosphere `dom.rs` (`document.query_selector`,
      `create_element`, `append_child`, `set_attribute`, `text_content`, `add_event_listener`) — lowered by
      the Python walker to `dom_native.*`/`dom.op` caps. No CPython, no PyO3.
- [ ] Unsupported Python features reported by the existing `unsupported_features` analyzer.
- [ ] Example `examples/python/dom_counter.py` running against `TestDomProvider`; same script runs in surfer
      once `SURF-PYDOM-1` lands.

## Files to modify

- `crates/crush-lang-python/src/{lower*.rs,analyzer.rs}`, new `dom` prelude module
- `examples/python/`

## Cross-refs

surfer-browser `SURF-PYDOM-1`, `SURF-SCRIPT-1`; CRUSH-252.
