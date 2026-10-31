# CRUSH-319 — A GUI for Crush: grant-gated `ui.*` API + desktop backend on arniko (phase 1)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-319 |
| **Priority** | P2 |
| **Status** | In review (PR open) |
| **Phase** | M3 |
| **Assignee** | nova |
| **Dependencies** | none (arniko 0.3.1 from crates.io) |
| **Estimated effort** | L |

## Problem

A Crush program can print and read lines but cannot show anything on a screen.
Decisions (repo owner, 2026-10-31): one `ui.*` API with several backends; the
program renders and **pulls events** with a blocking `ui.next_event()` (no
host-to-VM callbacks, the `io.read` pattern); desktop first (arniko), a
terminal backend on tui-easy and the browser playground later; lives in a new
`crates/crush-gui`, not in `crush-lang-sdk`'s default build (arniko pulls
Stylo/vello/winit); grant-gated by `--ui`.

## Success criteria

- [x] `docs/design/ui-api.md`: the surface, event shapes, grant, backend trait, how terminal/browser map on.
- [x] `crates/crush-gui`: `ui.*` caps, the `UiBackend` seam, a desktop backend (arniko reactive + bliss window), a headless backend.
- [x] `crush-gui run FILE --ui [grant flags]`; `crush gui …` delegates; `crush-installer` installs it when built.
- [x] `examples/gui/counter.crush`, `examples/gui/todo.crush`, driven only through `ui.*`.
- [x] Headless tests run both examples, inject events by id, assert the rendered DOM text; refused without `--ui`; other grants do not grant `ui.*`.
- [ ] Desktop window run on a machine with a display (this box has none).

## Outcome

See the PR. Phase 2: terminal backend (tui-easy, ratatui 0.30). Later: browser
(crush-web `Session`, `ui.next_event` as a yield point like `io.read`).
