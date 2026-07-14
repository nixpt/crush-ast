# crush-web

Browser WebAssembly runtime for Crush.

Compiles source via `crush-frontend` and runs it via `crush-vm`'s portable
(green-thread) interpreter — the same compiler and VM backend used natively
by `crush-run`/`crush-diff`, not a separate reimplementation.

## What this is not (yet)

- **No `@lang{}` polyglot blocks.** `EXEC_LANG` spawns a subprocess, which
  doesn't exist in a browser sandbox. Calling one returns a normal
  capability-gated `VmError`, the same shape you'd get running natively with
  no `--polyglot` grant — not a silent no-op or a fake success.
- **No FastVM, no AI optimizer, no native plugin loading.** `crush-vm` is
  pulled in with `default-features = false`: those backends depend on `ort`
  (ONNX Runtime) and `libloading` (dynamic linking), neither of which has a
  wasm32 story. See `crush-vm/Cargo.toml`'s `native-plugins` feature.
- **No persistent runtime state across calls.** Each `execute()` call
  compiles and runs fresh; there's no session/REPL state yet.

## Building

```bash
wasm-pack build --target web
```

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
