# CRUSH-256 — GC predictor without ONNX Runtime: generated Rust rules (or tract); ort out of crush-vm defaults

| Field | Value |
|-------|-------|
| **ID** | CRUSH-256 |
| **Priority** | P2 — dead weight on every default build; blocks the optimizer on wasm |
| **Status** | In Progress — core landed in PR #160 (2026-10-12: ort dropped from crush-vm and crush-lint); the fitted policy, real `alloc_rate`, and optional tract loader are open |
| **Phase** | M8 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | M |

## Problem

crush-vm's "AI optimizer" (`crates/crush-vm/src/ai_optimizer/mod.rs`) is a GC-timing predictor:
FastVM calls `VmOptimizer::should_gc` every 64 instructions (`fastvm/mod.rs` ~157-165) with
`[current_mem, peak_mem, instructions_since_gc, alloc_rate]` → one `ort` `Session::run` on a `[1, 4]`
float tensor → an `i64` label (`float_input` / `output_label`: a scikit-learn ONNX export).

- **No model ships.** Neither `crates/crush-vm/models/gc_model.onnx` nor crush-lint's `lint_model.onnx`
  is in the repo, so `should_gc` returns `false` and the predictor never affects a run.
- **ort costs on every default build anyway:** `native-plugins` (crush-vm's default, and
  crush-lang-sdk's) pulls `ort` with `download-binaries`: ONNX Runtime fetched at build time, a large
  native library, and no `wasm32-unknown-unknown` story — one reason crush-web / bozo-wasm must build
  crush-vm with `default-features = false` (losing the optimizer).
- **Hot-path cost if a model were present:** a session run + mutex + tensor conversion every 64
  instructions is microseconds for a 4-feature classifier; `inputs` is a fresh `Vec` per call and
  `alloc_rate` is always `0.0` (`fastvm/mod.rs` ~160).
- `native-plugins` couples three unrelated things: ort (optimizer), libloading + crush-ffi (native plugins).
- crush-lint carries an `ort` session field that is never used (`crates/crush-lint/src/lib.rs`).

## Options considered (2026-10-12)

| | generated Rust rule | **tract** (sonos, pure Rust, 0.23.x) | **ort** (ONNX Runtime 2.0.0-rc.x) |
|---|---|---|---|
| Build | nothing | plain cargo | downloads/links native lib |
| wasm32 | ✅ | ✅ | ❌ |
| Per-call cost (4 features) | nanoseconds | low µs | µs + lock + value conversion |
| sklearn (`ai.onnx.ml`) models | n/a (exported coefficients) | partial — export without ZipMap, verify ops | full |
| GPU / big NN models | — | CPU only | ✅ |

## Success criteria

- [ ] **Default path: generated rules.** A trainer step (from the existing telemetry CSV,
      `VmOptimizer::dump_telemetry_csv`) fits a small decision tree / logistic model and emits a Rust
      function (`fn should_gc(cur, peak, since, rate) -> bool`, constants only) checked into
      `ai_optimizer/`; FastVM calls it with stack values (no `Vec`, no lock). Same decision on every
      target, including wasm.
- [ ] `alloc_rate` is a real measurement (allocations since last GC / instructions) or dropped from the
      feature set.
- [ ] **Optional model file:** a feature `gc-model-tract` loading a `.onnx` (exported without ZipMap)
      with tract; works on wasm32. ort is not used by crush-vm.
- [ ] `native-plugins` = libloading + crush-ffi only; ort removed from crush-vm (and from
      crush-lang-sdk's defaults transitively). If ort is kept anywhere, it is behind its own opt-in
      feature (e.g. crush-lint `onnx-gpu`) for real NN inference.
- [ ] crush-lint's unused `ort` session removed (or gated) until its inference loop exists.
- [ ] Benchmarks: FastVM loop with the generated rule vs current (no model) — no regression;
      differential harness unchanged (GC timing must not change program output).

## Files to modify

- `crates/crush-vm/Cargo.toml` — drop `ort`; `native-plugins = ["dep:libloading", "dep:crush-ffi"]`;
  optional `gc-model-tract = ["dep:tract-onnx"]`
- `crates/crush-vm/src/ai_optimizer/mod.rs` — generated rule; optional tract loader
- `crates/crush-vm/src/fastvm/mod.rs` — call site (no allocation; real `alloc_rate`)
- `crates/crush-lint/{Cargo.toml,src/lib.rs}` — unused ort session
- `scripts/` or `tools/` — telemetry → fitted rule → Rust generator

## Cross-refs

crush-web / bozo-wasm (`default-features = false` on crush-vm); surfer-browser `SURF-BUILD-1`
(heavy defaults pulled through crush-lang-sdk).

## Survey 2026-10-31 (status)

PR #160 says it "implements the core of CRUSH-256" and lists the fitted policy / `alloc_rate` / tract loader as follow-ups. Previous status: `Backlog`.
