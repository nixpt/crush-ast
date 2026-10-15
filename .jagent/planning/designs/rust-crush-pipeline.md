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
  with `rustc` for small fixtures; that corpus is VM-only, with no Rust AOT
  parity suite yet. Unsupported imports and top-level items now fail loudly
  (CRUSH-197 / CRUSH-266).
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

### Phase 1 — Shared walker-backed SDK compile API (CRUSH-xps-3)

Add a feature-gated `compile_source(language_or_extension, source)` API over the
existing `WalkerAdapter` registry used by `crush-aotc` and bozo today. Keep
`compile_rust_source` as a thin convenience wrapper if useful. The API lowers
through the selected frontend and the same Crush compiler/runtime pipeline,
then returns the ordinary VM program that callers execute with `Runtime::run`
and their explicit `HostCaps`.

Resolve the dependency direction before implementation. In particular,
`crush-lang-rust/src/sdk.rs` is currently test-only and the Rust crate has an
SDK dev-dependency; do not promote that helper unchanged or create a Cargo
dependency cycle. Move end-to-end tests to the integration boundary or use a
small bridge crate if the graph requires it. Keep individual walker crates
optional so embedders can select the languages they need.

**Exit:** the SDK selects a frontend by language or extension through one
registry-backed API, then runs the result through the common compiler/runtime
and host-capability model. Rust convenience APIs do not create a separate
dispatch path.

### Phase 2 — Rust exact-or-loud conformance framework (CRUSH-xps-5)

Build an exact-or-loud Rust parity harness before exposing `.rs` through the
normal runner. VM behavior must match `rustc`; AOT must match or fail loudly.
The harness needs exact comparisons, a known-loud allow-list that fails if a
fixture becomes exact, a known-silent list tied to fixing tickets, an
unlisted-silent-failure guard, and freshness checks that re-run `rustc`.
Add the AOT twin at `crates/crush-aot/tests/rust_parity.rs`.

The first known-silent entries must point to their fixing tickets:

- block shadowing → CRUSH-xps-6;
- Rust `Vec`/`String`/struct value semantics (`clone`, assignment, argument
  passing) → CRUSH-xps-7;
- integer widths, `as` casts, wrapping, and overflow → CRUSH-xps-8;
- `{:?}` formatting → CRUSH-xps-9.

Every accepted fixture must be exact. Unsupported or known-divergent constructs
must fail loudly until their fixing ticket lands. CRUSH-xps-4 depends on all
four fixing tickets as well as this harness, so the default runner cannot expose
these known-silent cases early.

The first silent cases to capture and fix are:

| Area | Rust source | `rustc` result | Current Crush result |
|---|---|---|---|
| Block shadowing (xps-6) | `let x = 1; { let x = 2; println!("{}", x); } println!("{}", x);` | `2` / `1` | `2` / `2` |
| `Vec::clone` (xps-7) | `let a = vec![1,2,3]; let mut b = a.clone(); b.push(4); println!("{} {}", a.len(), b.len());` | `3 4` | `4 4` |
| Integer cast width (xps-8) | `let big: i64 = 3_000_000_000; println!("{}", big as i32);` | `-1294967296` | `3000000000` |

Also put `wrapping_add` in the known-loud set until the operation is exact.

### Phase 3 — Shared-registry CLI dispatch (CRUSH-xps-4)

Teach `crush run <path>` to use the same extension-to-`WalkerAdapter` registry
as the SDK and existing walker tools. Preserve `.crush`; enable `.rs` first,
gated on Phase 2's exact-or-loud result. `.py`, `.js`, and `.ts` can then be
enabled by their frontend features and parity gates without another dispatch
implementation. Coordinate the `.rs` slice with CRUSH-zorro-27 / PR #201 so
the work is consolidated rather than duplicated.

The next frontend candidates are `.py` (Python parity work #184 / #203) and
`.js` / `.ts` (the JS parity work in PR #198); each stays gated on its own
exact-or-loud results. No language gets a fallback to its host interpreter.
Walked `.py` module-level code runs as `main`; any later `sys.exit(n)` support
must map `n` to the runner's process status. `@python` blocks inside `.crush`
remain real CPython subprocesses behind `--polyglot` and its permission gate.

Never shell out to `rustc`, Cargo, Python, Node, or another language runtime as
a fallback for unsupported syntax. A source extension always selects the
registered walker path or fails closed with an actionable diagnostic.

**Exit:** SDK, `crush run`, `crush-aotc`, and bozo use one extension-to-frontend
mapping; runner tests prove frontend selection, normal Crush runtime behavior,
capability grants, quotas, output preservation, and nonzero failure status.

### Phase 4 — Diagnostics and carefully expanded subset (follow-up)

Carry source spans from Rust lowering into useful diagnostics. Expand accepted
syntax only with exact parity fixtures and VM exact-or-loud / AOT exact-or-loud
coverage. Do not claim arbitrary Rust compatibility.

### Deferred decision — Broader Rust crate access

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
| CRUSH-xps-3 | SDK registry-backed compile API | CRUSH-xps-2, CRUSH-197 | One frontend registry API with optional language adapters |
| CRUSH-xps-5 | Rust exact-or-loud gate | CRUSH-xps-3, CRUSH-197 | Silent mis-lowerings fail loudly; VM/AOT parity harness exists |
| CRUSH-xps-6 | Rust block shadowing | CRUSH-xps-3, CRUSH-xps-5 | Preserve nested binding semantics or reject unsupported shadowing |
| CRUSH-xps-7 | Rust collection/string value semantics | CRUSH-xps-3, CRUSH-xps-5 | `clone`, assignment, and argument passing are exact or loud |
| CRUSH-xps-8 | Rust integer widths and casts | CRUSH-xps-3, CRUSH-xps-5 | Width, cast, wrapping, and overflow behavior is exact or loud |
| CRUSH-xps-9 | Rust debug formatting | CRUSH-xps-3, CRUSH-xps-5 | `{:?}` is exact or rejected |
| CRUSH-xps-4 | Shared-registry CLI dispatch | CRUSH-xps-3, CRUSH-xps-5, CRUSH-xps-6, CRUSH-xps-7, CRUSH-xps-8, CRUSH-xps-9 | `.rs` is enabled only after all parity gates; other walkers reuse dispatch |

CRUSH-xps-1 is the in-flight manual plugin MVP, not a prerequisite for the
source-language SDK phases. CRUSH-179 gates any later Cargo-managed build
proposal, not the manual host-selected wrapper model.
