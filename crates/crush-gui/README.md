# crush-gui

Crush programs that draw a window. A grant-gated `ui.*` capability API, a
desktop backend on [arniko](https://crates.io/crates/arniko), and a headless
backend for tests. Ticket CRUSH-319; design in
[`docs/design/ui-api.md`](../../docs/design/ui-api.md).

```crush
ui.title("Counter");
let count = 0;
ui.render(page(count));
while true {
    let ev = ui.next_event();         // blocks; {kind, id, value, key}
    if ev.kind == "close" { break; }
    if ev.id == "inc" { count = count + 1; }
    ui.render(page(count));
}
```

```bash
cargo build -p crush-gui
crush-gui run examples/gui/counter.crush --ui     # needs a desktop session
crush gui run examples/gui/todo.crush --ui        # via the umbrella CLI
crush-gui run examples/gui/counter.crush          # refused: "--ui: ui.next_event, ui.render"
```

* `--ui` grants every `ui.*` capability and nothing else; the other grants of
  `crush-run` (`--fs`, `--net`, …, not `--gpu`/`--graphics`/`--db`) work the same.
* Not part of `crush-lang-sdk`'s default build: arniko brings Stylo, vello and winit.
* `crush gui` execs the `crush-gui` next to `crush`; `crush-installer` installs it when it was built.

## Layout

| module | what |
|---|---|
| `caps` | the nine `ui.*` capabilities and the `--ui` grant |
| `backend` | `UiBackend` (the program's side), `link()` / `SurfaceEnd` (a backend's side) |
| `desktop` | `run_window`: arniko `launch_reactive`, main thread |
| `headless` | `HeadlessApp`: a bliss DOM with no window; click/type/press by element id |
| `page` | DOM events to `UiEvent`s; shared by both backends |
| `run` | compile, preflight against the grants, run |

## Testing

`cargo test -p crush-gui` needs no display. `tests/headless.rs` runs each
example in `examples/gui/`, injects events by element id into a real bliss
DOM, and asserts on the text of the page the program renders next.

**What the tests do not cover:** the actual window. `desktop::run_window` is
compiled and structured for a machine with a display, but it was written on a
box without one and has not been run on a screen: winit/vello startup, how the
page looks, and delivery of real mouse and keyboard events to the DOM
(the headless tests dispatch the DOM events directly) are unverified.
