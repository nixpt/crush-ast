# `ui.*` — the Crush GUI capability API (CRUSH-319)

Status: phase 1 (desktop + headless backends). Crate: `crates/crush-gui`.

A Crush program draws a page and **pulls** events in a loop. Nothing calls
back into the VM: `ui.next_event()` blocks like `io.read()` does, so there is
no re-entry, no closures stored in the host, and the same program runs on any
backend.

```crush
ui.title("Counter")
ui.render(page(count))
while true {
    let ev = ui.next_event()          // a map: {kind, id, value, key}
    if ev.kind == "close" { break }
    if ev.id == "inc" { count = count + 1 }
    ui.render(page(count))
}
```

## The surface

All nine capabilities are in one namespace, granted together by `--ui`.

| call | returns | notes |
|---|---|---|
| `ui.render(markup)` | — | Replace the whole page with `markup` (HTML). Arniko's stylesheet is loaded; its component classes (`arniko-btn`, `arniko-card`, …) work in your own markup. |
| `ui.next_event()` | event map | Blocks until the next event. After the window is closed it returns `close` every time. |
| `ui.title(s)` | — | Window title. Read once, when the window opens (on the first `ui.render`), so call it first. |
| `ui.escape(s)` | string | HTML-escape `s`. Use it on any text you interpolate into markup. |
| `ui.button(id, label[, variant])` | markup | `variant`: `default` `destructive` `outline` `secondary` `ghost` `accent`. |
| `ui.input(id[, value[, placeholder]])` | markup | A text input. |
| `ui.card(title, body_html)` | markup | `body_html` is markup (not escaped). |
| `ui.alert(message[, variant])` | markup | `info` `success` `warning` `error`. |
| `ui.badge(text[, variant])` | markup | `default` `success` `warning` `error` `info` `purple`. |

The helpers are the mapping of `arniko-crush`'s host caps (the same arniko
components and variant names) with an `id` added to the interactive ones.
Markup is plain HTML; the helpers are conveniences, not a requirement. An
element is **addressable** if it has an `id` attribute.

### Events

Every event is a map with the same four string fields, so `ev.kind` and
`ev.id` never need an existence check; fields that do not apply are `""`.

| `kind` | when | `id` | `value` | `key` |
|---|---|---|---|---|
| `click` | a click on, or inside, an element with an `id` (the nearest enclosing id) | that id | `""` | `""` |
| `input` | the text of an `<input id=…>` changed | the input | its new text | `""` |
| `submit` | Enter pressed in an `<input id=…>` | the input | its text (the rendered `value` if nothing was typed) | `""` |
| `key` | a non-printing key: `Escape`, `Tab`, `ArrowUp`, … (not `Enter`, which is `submit`; printable keys are not reported) | `""` | `""` | the key name |
| `close` | the window was closed | `""` | `""` | `""` |

Reserved for later, not emitted in phase 1: `resize` (`value` = `"WxH"`),
`change` for checkboxes/selects, printable `key` events.

### Things a program must know

* `ui.render` **replaces the DOM**. An `<input>` loses focus and caret when
  it is replaced, so do not re-render on every `input` event: record
  `ev.value` and redraw on `click`/`submit`.
* On `close` a program should `break`. It is not killed: it can save state.
  `render`/`title` after close are silently dropped.
* A program that ends normally leaves its last page on screen until the
  window is closed; `sys.exit(n)` and errors end the process at once.
* The step quota defaults to 500M instructions for `crush-gui` (a UI loop is
  long-lived); `--max-steps` changes it. Time spent blocked in
  `ui.next_event()` costs no steps.

## The grant

`--ui` unlocks every `ui.*` capability. Without it:

```
$ crush-gui run app.crush
[capabilities] app.crush needs capabilities this run does not grant; nothing was run:
  --ui: ui.next_event, ui.render
```

It is the same preflight as `--fs`/`--net`/`--tcp`: the compiled program's
reachable `CAP_CALL`s are checked against what is registered
(`crush_lang_sdk::effects::missing_grants`), before any instruction runs.
No other flag grants `ui.*`, and `--ui` grants nothing else. The other grants
(`--fs`, `--net`, …) work in `crush-gui run` as in `crush-run`. Plain
`crush-run` has no `ui.*`; it refuses such a program and points at `crush gui`.

Effects declared on the caps: `ui/render` (render, title) and `ui/event`
(next_event); the markup helpers are pure.

## Backends

```rust
pub trait UiBackend: Send + Sync + 'static {
    fn render(&self, markup: &str) -> Result<(), String>;
    fn set_title(&self, title: &str) -> Result<(), String>;
    fn next_event(&self) -> UiEvent;      // blocks; `close` forever once gone
}
```

`UiBackend` is the **program's** side: the `ui.*` caps call it from the
program's thread. The shipped backends share one implementation of it,
`Link`, made by `link()`: a channel pair whose other end, `SurfaceEnd`
(`Receiver<Request>` + `Sender<UiEvent>`), belongs to whoever owns the
surface. A backend is "something that drains requests and feeds events".

**Threading.** The Crush VM cannot be re-entered, and `Value` is `!Send`, so
the program runs on its own thread and only plain strings cross:

```
program thread                     main thread (winit)
ui.render ── Request::Render ──▶   pump → Signal<String> → wake loop
                                   reactor flush → set_inner_html → rewire
ui.next_event ◀── UiEvent ───────  DOM click/input/key handlers (arniko EventRouter)
```

winit requires the main thread, so the CLI runs the program on a spawned
thread and `desktop::run_window` on main. Closing the window ends the loop,
sends `close`, and `crush-gui` joins the program thread.

* **desktop** (`desktop.rs`) — arniko `launch_reactive`: a bliss document,
  vello renderer, winit window. Each render is a `set_inner_html` followed by
  re-registering click/input/Enter handlers on the new nodes (a click lands on
  the innermost node, so every node under an `id`'d element gets one).
* **headless** (`headless.rs`) — the same document and the same handlers with
  no window. `HeadlessApp` runs a program, parses every page into a bliss DOM,
  and acts on it by element id (`click`, `type_into`, `press_enter`, `close`),
  settling after each action (the link announces when the program is blocked
  in `next_event`, so there are no sleeps). Used by `tests/headless.rs`.
* **terminal** (phase 2, tui-easy / ratatui 0.30) — implements the same
  `SurfaceEnd` loop: parse the markup subset (headings, paragraphs, lists,
  buttons, inputs, `id`s) into a widget tree, draw it, turn Tab/Enter/typing
  into `click`/`input`/`submit`/`key`. Same API, same events; markup that uses
  CSS/layout beyond the subset degrades to text. The interactive helpers carry
  their `id`s so focus order is document order.
* **browser** (later, crush-web) — `Session` already steps a program
  cooperatively. `ui.next_event` becomes a yield point (like `io.read`): the
  page applies the render, the host delivers the next DOM event by resuming
  the session. `ui.render` maps to `innerHTML` and the events are the same
  four-field maps.

## Out of scope for phase 1

Multiple windows, focus control, timers/animation frames (`time.sleep` blocks
the program, not the UI), images, menus/dialogs, dynamic title after open,
clipboard, drag and drop, accessibility tree, a keyed/diffed render (every
render is a full replace).
