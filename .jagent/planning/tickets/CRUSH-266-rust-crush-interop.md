# CRUSH-266 — Rust ↔ Crush interoperability and Rust crate access

| Field | Value |
|-------|-------|
| **ID** | CRUSH-266 |
| **Priority** | P1 |
| **Status** | In progress |
| **Phase** | M6 / interop |
| **Assignee** | nixp (user-directed) |
| **Dependencies** | CRUSH-179 must gate native plugin loading before exposing Rust crates |
| **Estimated effort** | L |
| **Filed by** | Codex, user-directed Rust interop audit — 2026-10-15 |

## Goal

Make the boundary between Rust and Crush usable in both directions, and provide a
practical path for Crush programs to call Rust libraries where Crush's own
standard library is too small. Treat `crush-lang-rust`'s source walker,
`crush-lang-sdk`'s Rust embedding API, and the native plugin ABI as distinct
surfaces with explicit contracts.

## Current findings

- Rust source enters Crush through `crush-lang-rust` (`syn` → CAST), and can be
  run through the walker/AOT tools. The supported subset is documented in
  `crates/crush-lang-rust/src/lib.rs`; the CRUSH-197 output corpus covers nine
  small programs. The ordinary `crush-run` source path is Crush-only.
- The audit found that the Rust walker lowered non-function top-level items to
  a null expression statement, silently ignoring `use` declarations. It now
  rejects unsupported top-level items and qualified crate paths with an
  explicit message that crate APIs require a Crush capability/plugin. Rust
  crate imports still do not resolve to Crush functions.
- Rust hosts can already compile and execute Crush through `crush-lang-sdk`
  (`compile::compile_crush_source`, `Runtime::run`); `crush-aot` can also emit a
  Rust-compiled shared library. `VmResult` now exposes the entry function's
  explicit return value, and the SDK has a runnable example that observes both
  output and return value.
- `crush-ffi` defines a C ABI and `crush-plugin-example` is a Rust `cdylib`.
  A Rust host can load a host-selected library with `load_plugin_exports` and
  grant a named export allowlist. The wrapper ABI currently accepts scalar
  null/bool/int/float/string arguments and results; complex values are rejected
  until ownership is specified. Ordinary Rust ABI symbols, generic APIs, and
  unrestricted `use` syntax are not direct interpreter imports.
- `crush-vm run` currently registers guest-selected `os.cargo` and
  `__crush_ffi__` capabilities. CRUSH-179 tracks this ambient process/dlopen
  access and is a hard gate for a packaged Rust-crate workflow. Cargo builds
  must remain host-controlled; a Crush program must never choose arbitrary
  Cargo flags or a shared-library path.

## Success criteria

- [x] Rust source accepted by the walker executes through the normal Crush
      compile/runtime path, with parity fixtures checked against `rustc`.
- [ ] Every unsupported Rust top-level item or expression fails with a useful
      diagnostic; no import, declaration, or expression is silently discarded.
- [x] A documented Rust-host example compiles a Crush program and observes its
      output and return value through `crush-lang-sdk`.
- [x] A Rust crate can be exposed to Crush through a narrow, named
      `crush-ffi` capability; the host chooses and loads the trusted plugin,
      and the guest only receives explicitly granted export names.
- [x] A small end-to-end example demonstrates a Rust library function called
      from Crush. The example states the supported value types and the native
      code trust boundary.
- [x] Rust crate resolution/building is not performed from guest-controlled
      `os.cargo` calls. Any future `crush-pkg` Cargo integration is separately
      gated, reproducible, and requires host approval for native code.

## Work order

1. Fix the Rust walker's silent item loss and establish the supported-subset
   parity boundary; expand the executable corpus from concrete audit findings.
2. Document and demonstrate Rust-host embedding using the existing SDK.
3. Land CRUSH-179's host-side gate for `os.cargo` and dynamic plugin loading.
4. Add the smallest Rust `cdylib` crate wrapper workflow that exposes selected
   library functions as capabilities. Keep automatic Cargo dependency
   resolution/building out of the first slice unless a separate ticket approves
   its package and trust model.

## Non-goals

- Importing arbitrary Rust source or crate APIs directly into the interpreter
  without a stable boundary.
- Reimplementing all of `std` in Crush.
- Treating a Rust crate's Rust ABI or unsafe internals as sandboxed code.
- Reworking the Rust AOT performance gap (CRUSH-259).
