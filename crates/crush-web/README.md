# crush-web

Browser WebAssembly runtime for Crush.

Compiles source via `crush-frontend` and runs it via `crush-vm`'s portable
(green-thread) interpreter — the same compiler and VM backend used natively
by `crush-run`/`crush-diff`, not a separate reimplementation.

## API

```js
import init, { execute, execute_with, Session, run_blob, check } from "./pkg/crush_web.js";
await init();

execute(source);            // { output, steps, halted, exit_code? } or throws; io.read sees EOF
execute_with(source, { stdin: "10\nh\ns\n0\n", max_steps: 1_000_000, args: ["a"] });
                            // { ok, output, error?, exit_code?, steps } — never throws for
                            // compile/runtime errors; output before an error is kept

const s = new Session(source, { max_steps: 1_000_000 });
let r = s.run();            // { status, output, error?, exit_code?, steps }
while (r.status === "need_input") {
  term.write(r.output);
  r = s.provide(await nextLine());   // or s.close() to send EOF
}
term.write(r.output);       // status is "done" or "error"
```

### Capabilities

Every entry point registers what `crush-run` gives a program with no grant
flags, all of it pure computation that never leaves the VM:

- the standard library (`math.*`, `str.*`, `system.*`, …), behind the
  default-on `stdlib` cargo feature;
- `sys.args()`, which returns the `args` option (`[]` when absent, and for
  `execute`/`run_blob`);
- `sys.exit(code)`: the run stops and reports `exit_code`. Output printed
  before it is kept. A `Session` ends with `status: "done"`; `execute_with`
  sets `ok: false` unless the code is 0; `execute` returns normally
  (`steps` is then 0);
- `caison.parse`.

Nothing that reaches outside the VM (`fs`, `time`, `env`, `process`, `net`)
is registered unless the page grants it (the `grants` option); calling an
ungranted one is refused before the program runs. A page can grant `time` (the
browser's clock), `env` (empty, as under `env -i`) and `fs`: the `fs.*` and
`text.*` file caps over an empty in-memory directory of the run's own, with
the same results and error messages as `crush-run --fs` in an empty
`--fs-root`. `process`, `net` and `polyglot.bash` are never grantable.

The standard library roughly doubles the module (about 0.5 MB to 0.95 MB
gzipped after `wasm-bindgen`, mostly `regex`). A page that doesn't need it can
build without it:

```bash
wasm-pack build --target web -- --no-default-features --features panic-hook
```

### `io.read` in the browser

A browser has no stdin, so where `io.read` gets its lines depends on the entry
point (`io.read_line` behaves the same except that end of input is `null`
instead of `""`, so a blank line and EOF differ; CRUSH-239):

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

### Polyglot blocks (JavaScript, Python)

```js
import init, { execute_with, set_lang_executor } from "./pkg/crush_web.js";
await init();

// Runs a granted block where the page decides — here a sandboxed function in a
// dedicated Worker, never with the page's own authority. Synchronous: return
// everything the block printed (including the result-sentinel line).
set_lang_executor((lang, code, env) => runInSandbox(lang, code, env));

execute_with(source, { grants: ["polyglot.javascript"] });
```

- **Two keys, both required:** the `grants` option (`polyglot.javascript`,
  `polyglot.python`; `polyglot.bash` and anything else is refused) and an
  executor. A grant without an executor, or an executor without a grant, is
  refused by the same `polyglot.<lang>` gate native uses.
- **`env`** maps the block's input variables to JSON strings — what native
  passes as environment variables. The marshaling prologue reads them as
  `process.env[name]`, so the sandbox must provide a `process.env` object.
- **Typed values in and out** need the `polyglot-javascript` cargo feature
  (free-variable analysis + guest rewrite via `boa_parser`; about 1.2 MB more raw
  `.wasm`, 0.4 MB gzipped). It does not link swc, which only lowers whole `.js`
  files (the SDK's `polyglot-javascript-swc`). Without it a granted block still
  runs, but sees no Crush variables and its result is its printed text.
- Python works through the same hook (e.g. Pyodide in a Worker). Its typed
  marshaling needs the `polyglot-python` cargo feature (rustpython's parser,
  pure Rust; about 1.8 MB more raw `.wasm`, 0.66 MB gzipped, on top of
  `polyglot-javascript`). Without it a variable *assigned* inside `@python` and used after
  it with no `->` declaring it (`@python { result = base ** 3 }` then
  `print(result)`) is a compile error here (`Undefined variable: result`),
  while native — which has the marshaling — prints `125`.

### Blocks that run asynchronously (`deferLang`)

The executor above is called synchronously, inside the VM's step, so a block
can't wait for anything: no promises, `await` or timers. A `Session` created
with `deferLang: true` hands each granted block to the page instead. The run
pauses, the page runs the block however long that takes, and then continues
the run:

```js
const s = new Session(source, { grants: ["polyglot.javascript"], deferLang: true });
let r = s.run();
while (r.status === "need_lang" || r.status === "need_input") {
  if (r.status === "need_lang") {
    const { lang, code, env } = r.lang;            // env: inputs as JSON text, by name
    try { r = s.provide_lang(await runAsync(lang, code, env)); }  // everything it printed
    catch (e) { r = s.fail_lang(String(e.message)); }              // what it would print on stderr
  } else {
    r = s.provide(await nextLine());
  }
}
```

`provide_lang` gives the block's output exactly as an executor returns it,
with the result-sentinel line its marshaling prints, so outputs, `->` values
and errors are the same as with `set_lang_executor`. The grant is checked
before the pause, and an ungranted block is refused before anything runs.
`execute_with` can't pause, so it reports an error for a deferred block.

### Lowered polyglot blocks (no interpreter)

```js
execute_with(source, { polyglot: "lower" });
```

With `polyglot: "lower"` (and the `polyglot-python` / `polyglot-javascript`
feature for the block's language), an `@python` / `@javascript` block isn't
run by an executor at all: its body is lowered through the language's walker
into the program, and runs as Crush. It spawns nothing, so it needs no grant
and no `set_lang_executor`.

- **It is not CPython or Node.** The block runs as the walker lowers it. That
  is exact for what the walker supports, and a compile error for everything
  else (`line 3: this @python block can't be lowered inline: class definitions
  not yet supported …`) — never a fallback to the executor, never a silently
  different result. A block that needs the real language (classes, most of the
  standard library, packages) has to run through the executor instead.
- **Inputs and outputs** work as with an interpreter: the Crush variables a
  block reads come in, the `->` outputs go back, each copied as JSON would
  copy it (a block can't change a Crush array it only reads; a function can't
  be an output).
- **The default is `"exec"`.** Native `crush-run` always runs blocks in an
  interpreter.
- **Output and failure work as with an interpreter**
  ([#312](https://github.com/nixpt/crush-ast/issues/312)). A block's output is
  trimmed, so `print("one")`, `@python { print("two") }`, `print("three")` prints
  `one` / `twothree` in both modes. A block that raises fails the program with the
  same error (`@python block raised a runtime error: (at .crush line N) ValueError:
  x`), even inside a Crush `try`, and what it printed before raising is dropped.

## What this is not (yet)

- **`@lang{}` blocks only through a host executor.** There's no subprocess in
  a browser. A block runs when the page grants it *and* supplies a runner —
  see [Polyglot blocks](#polyglot-blocks-javascript-python). Otherwise it's
  the same capability-gated `VmError` as native with no `--polyglot` grant.
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

Or with cargo and `wasm-bindgen-cli` (at the version in `Cargo.lock`), which is
how the size figures above were measured:

```bash
cargo build --release --target wasm32-unknown-unknown --features polyglot-javascript
wasm-bindgen --target web --remove-name-section --out-dir pkg \
    target/wasm32-unknown-unknown/release/crush_web.wasm
```

This crate is its own workspace root, so its `[profile.release]` (size-tuned:
`opt-level = "s"`, LTO, one codegen unit) applies to both. The profile strips
only debug info; `--remove-name-section` drops the ~1.3 MB `name` section. A full
symbol strip would also remove `target_features`, and wasm-bindgen would then
fall back from externref to its older glue. With `polyglot-javascript` this gives
a 5.0 MB `.wasm`, 1.5 MB gzipped.

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
