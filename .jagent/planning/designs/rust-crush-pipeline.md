# Rust ↔ Crush pipeline: phased plan

**Status:** Proposed for review
**Related work:** CRUSH-197, CRUSH-266, CRUSH-179, CRUSH-xps-1
**Owner:** Crush language/runtime and SDK maintainers

## Goal

Make the two source directions easy to use through the supported toolchain:

1. run the supported Rust subset through Crush's compiler and VM;
2. compile and run `.crush` programs from Rust with the SDK;
3. expose selected Rust crate functions to Crush through explicit, host-approved
   plugin wrappers.

These are three related paths with different contracts. Rust source translation
does not make Crush a Cargo runner, and plugin wrappers do not import Rust's
unstable ABI or sandbox native code.

## Current baseline

- `crush-lang-rust` parses Rust with `syn` and lowers a documented subset to
  CAST. Its corpus tests exercise CAST → CASM → CVM1 and compare printed output
  with `rustc` for small fixtures. Unsupported imports and top-level items now
  fail loudly (CRUSH-197 / CRUSH-266).
- `crush-run` compiles Crush source. There is no production SDK or CLI entry
  point that accepts Rust source as a normal run target.
- Rust hosts can compile Crush with
  `crush_lang_sdk::compile::compile_crush_source` and execute it with
  `Runtime::run`.
- The manual `cdylib` wrapper path is being added by CRUSH-xps-1 / PR #196:
  the host selects the library and export allowlist; package code cannot grant
  either. This is the first Rust-crate access mechanism.
- CRUSH-179 tracks the standalone `crush-vm` ambient `os.cargo` and FFI gate.
  It remains a security dependency for any future package-managed native build.

## Phases

### Phase 0 — Establish safe boundaries (in progress)

**Work:** CRUSH-197 / CRUSH-266 and CRUSH-xps-1.
**Exit:** unsupported Rust is rejected rather than silently dropped; Crush can
call only host-selected wrapper exports; Rust can host Crush through the SDK;
native code is described as trusted host code.

No automatic Cargo invocation or guest-selected library path is included.

### Phase 1 — Public Rust-source SDK pipeline (CRUSH-xps-3)

Add a feature-gated `compile_rust_source` API beside
`compile_crush_source`. It should lower through the existing Rust frontend and
the same Crush compiler/runtime pipeline, then return the ordinary VM program
that callers execute with `Runtime::run` and their explicit `HostCaps`.

Resolve the dependency direction before implementation. In particular,
`crush-lang-rust/src/sdk.rs` is currently test-only and the Rust crate has an
SDK dev-dependency; do not promote that helper unchanged or create a Cargo
dependency cycle. Move end-to-end tests to the integration boundary or use a
small bridge crate if the graph requires it. Keep Rust support optional for
embedders that do not need the frontend.

**Exit:** Rust hosts compile `.crush` and supported `.rs` source with explicit
APIs, then run either result through one `Runtime` and host-capability model.
Errors retain the source language and useful file/line context.

### Phase 2 — Unified source-file runner (CRUSH-xps-4)

Teach `crush run <path>` to select the frontend by source extension. Preserve
the existing `.crush` path. Route `.rs` through the public Rust compile API;
unknown or disabled extensions fail with an actionable diagnostic. Do not
silently shell out to `rustc` or Cargo.

**Exit:** CLI tests prove `.crush` and supported `.rs` files run through the
intended frontends, return comparable output/status behavior, and report
unsupported Rust constructs without executing a subprocess.

### Phase 3 — Rust-subset conformance and diagnostics (CRUSH-xps-5)

Use the existing rustc-backed corpus as the contract. Expand it by feature
families only when the Crush representation preserves the Rust behavior being
claimed. Add source spans to Rust lowering diagnostics and document semantic
differences explicitly (including formatting and type/coercion behavior).

**Exit:** every accepted fixture has a parity assertion; unsupported syntax
has a location-bearing error; no phase claims arbitrary Rust compatibility.

### Phase 4 — Broader Rust crate access (deferred decision)

Continue with the manually built `crush-ffi` wrapper and host allowlist from
CRUSH-xps-1. Revisit Cargo-managed plugin builds only after CRUSH-179 is closed
and a separate design approves reproducibility, dependency resolution,
artifact provenance, host consent, and trust policy. This phase is not part of
the current implementation authorization.

## Non-goals

- Running arbitrary `.rs` files, Cargo workspaces, build scripts, or proc
  macros inside the Crush VM.
- Importing arbitrary Rust crates using Rust `use` syntax.
- Expanding Crush's standard library by linking all of Rust `std`.
- Loading native code selected or granted by a Crush package.
- Treating a native plugin as sandboxed or safe for untrusted code.

## Ticket sequence and gates

| Ticket | Phase | Depends on | Outcome |
|---|---|---|---|
| CRUSH-xps-2 | Design / this PR | CRUSH-266 | Ratified phase plan and boundaries |
| CRUSH-xps-3 | SDK Rust-source API | CRUSH-xps-2, CRUSH-197 | Public compile API returning normal VM programs |
| CRUSH-xps-4 | CLI source dispatch | CRUSH-xps-3 | `crush run` supports `.crush` and `.rs` |
| CRUSH-xps-5 | Conformance and diagnostics | CRUSH-xps-3 | Location-aware diagnostics and parity contract |

CRUSH-xps-1 is the in-flight manual plugin MVP, not a prerequisite for the
source-language SDK phases. CRUSH-179 gates any later Cargo-managed build
proposal, not the manual host-selected wrapper model.
