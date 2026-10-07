# crush-web

Browser WebAssembly runtime for Crush.

Compiles source via `crush-frontend` and runs it via `crush-vm`'s portable
(green-thread) interpreter — the same compiler and VM backend used natively
by `crush-run`/`crush-diff`, not a separate reimplementation.

## API

```js
import init, { execute, execute_with, Session, run_blob, check } from "./pkg/crush_web.js";
await init();

execute(source);            // { output, steps, halted } or throws; io.read sees EOF
execute_with(source, { stdin: "10\nh\ns\n0\n", max_steps: 1_000_000 });
                            // { ok, output, error?, steps } — never throws for
                            // compile/runtime errors; output before an error is kept

const s = new Session(source, { max_steps: 1_000_000 });
let r = s.run();            // { status, output, error?, steps }
while (r.status === "need_input") {
  term.write(r.output);
  r = s.provide(await nextLine());   // or s.close() to send EOF
}
term.write(r.output);       // status is "done" or "error"
```

### `io.read` in the browser

A browser has no stdin, so where `io.read` gets its lines depends on the entry
point:

- `execute` / `run_blob`: no input; every read returns `""` (EOF).
- `execute_with`: lines of `stdin`, then `""` once it runs out.
- `Session`: interactive. The program runs until it calls `io.read` with no
  line pending, then the call returns `status: "need_input"`.

**Pause/resume contract.** The pause happens *before* the `io.read`
instruction executes (`crush_vm::PortableVm` returns
`VmYield::HostCall { capability: "io.read" }` with IP, stack and step count
untouched). `provide(line)` queues the line (no trailing newline needed; a
`\n` inside it makes several lines) and continues; that read returns it.
Nothing is re-executed and nothing printed is lost. `close()` sends EOF: the
pending and every later read returns `""`.

- `output` in a `Session` report is only what that call printed; append it.
  `transcript()` returns everything so far.
- `steps` is cumulative, and `max_steps` (default 1,000,000) is a budget for
  the whole session, not per call.
- A compile error is reported by the first `run()` as `status: "error"`; it
  is not thrown. Calls after `done`/`error` return the same status with empty
  output.

## What this is not (yet)

- **No `@lang{}` polyglot blocks.** `EXEC_LANG` spawns a subprocess, which
  doesn't exist in a browser sandbox. Calling one returns a normal
  capability-gated `VmError`, the same shape you'd get running natively with
  no `--polyglot` grant — not a silent no-op or a fake success.
- **No FastVM, no AI optimizer, no native plugin loading.** `crush-vm` is
  pulled in with `default-features = false`: those backends depend on `ort`
  (ONNX Runtime) and `libloading` (dynamic linking), neither of which has a
  wasm32 story. See `crush-vm/Cargo.toml`'s `native-plugins` feature.
- **No REPL state across programs.** `execute()`/`execute_with()` compile and
  run fresh each call; a `Session` keeps one program's state across its
  `io.read` pauses, not across programs.

## Building

```bash
wasm-pack build --target web
```

## Testing

```bash
cargo test                                   # native: execute_with + Session
WASM_BINDGEN=wasm-bindgen scripts/browser-test.sh
```

`scripts/browser-test.sh` builds for `wasm32-unknown-unknown`, runs
`wasm-bindgen --target web` (the CLI must match the version in `Cargo.lock`),
stages `www/index.html` with `examples/crush/blackjack_interactive.crush`, and
plays one round in headless Chromium (python3 + playwright) through `Session`
and through `execute_with`, failing unless both transcripts match. Open the
staged `index.html` without `?auto` to play by hand.

## Provenance

Replaces `exosphere-apps/crates/apps/crush-web-runtime`, which depended on
the legacy exosphere `nanovm`/`crush-lang` stack (not this repo's
crush-frontend/crush-vm), never once compiled for `wasm32-unknown-unknown`
(blocked by `mio` — pulled in via `nanovm`'s `tokio` "full" feature — before
even reaching `nanovm`'s other native-only deps: `mlua`, `quick-js`, `ort`),
and whose `execute()` was a stub that never actually called into `nanovm` or
`crush-lang` — real compile-and-run behavior here.

## License

Licensed under either of [MIT](../../LICENSE-MIT) or [Apache License 2.0](../../LICENSE-APACHE) at your option.
