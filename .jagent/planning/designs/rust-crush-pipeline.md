# Rust ↔ Crush pipeline: phased plan

**Status:** Phase plan; see the completion addendum for current review state
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

## Current baseline (2026-10-15 snapshot)

Main is `30288bc`; the Rust SDK review stack is #213 → #220 → #224 → #233
(head `1be8c51`). Features in that stack are unreleased until integrated and
published; the existing SDK 0.6.0 API is not evidence that those additions ship.

- `crush-lang-rust` parses with `syn`. PR #213 adds the public registry-backed
  source API and conservative closure/block correctness fixes; xps-10–12 remain
  In review there.
- PR #220 adds the SDK VM/Rust AOT/C AOT conformance inventory. With the xps-6
  bindings and xps-7 value changes through #233, it has 58 fixtures and three
  known silent entries: integer cast/overflow (xps-8), String Debug (xps-9).
  A green inventory still permits those pinned failures.
- `crush-run` does not yet provide normal `.rs` source dispatch. xps-4 remains
  gated on the Rust source contract and coordinates with CRUSH-zorro-27.
- Rust hosts can compile `.crush` with `compile_crush_source` and execute it
  with `Runtime::run` and explicit host capabilities. The in-review source API
  adds language/extension selection through the walker registry.
- Manual host-approved Rust plugin wrappers remain the crate-access model
  (CRUSH-xps-1 / #196). Package code cannot choose native-code grants.
  CRUSH-179 and a separate design gate future Cargo-managed native builds.
- Main now has COSM K1.4 record runtime support (#222) and the Boa default
  parser/feature layout (#228). Neither enables Rust struct syntax or closes
  Rust numeric, formatting or AOT capability gaps. Integrate those shared
  compiler/SDK changes before continuing runtime implementation.

The detailed [xps-8–13 completion specification](rust-parity-xps-8-13.md)
defines numeric facts, Debug/format evaluation, existing review closure,
backend capability contracts and dispatch matrices. It preserves the six
existing IDs; xps-10–12 are not new Backlog work.

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

The original reproductions below motivated the gate. Shadowing and flat vector
clone now have fixes in review; the numeric case remains open:

| Area | Rust source | `rustc` result | Original Crush result |
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
| CRUSH-xps-8 | Rust integer widths and casts | CRUSH-xps-3, CRUSH-xps-5, CRUSH-xps-6, CRUSH-xps-7 | Width, cast, wrapping, and overflow behavior is exact or loud |
| CRUSH-xps-9 | Rust Debug and macro evaluation | CRUSH-xps-3, CRUSH-xps-5, CRUSH-xps-6, CRUSH-xps-7; numeric descriptors after xps-8 | `{:?}` is exact or rejected |
| CRUSH-xps-10 | Shared walker helper/loop contract | CRUSH-xps-2, CRUSH-197 | In-review implementation; close the helper/scope proof matrix in #213 |
| CRUSH-xps-11 | Scalar closures exact or loud | CRUSH-xps-2, CRUSH-197; coordinate xps-6/7 | In-review implementation; close the parameter/capture proof matrix in #213 |
| CRUSH-xps-12 | Complete bodies / safe value-block boundary | CRUSH-xps-3, CRUSH-xps-5, CRUSH-197 | In-review implementation; close the block/effects proof matrix in #213 |
| CRUSH-xps-13 | Mapped capability parity on AOT | CRUSH-xps-3, CRUSH-xps-5, CRUSH-xps-7; numeric dispatch after xps-8 | Shared kernels and compatible source method contracts |
| CRUSH-xps-4 | Shared-registry CLI dispatch | CRUSH-xps-3, CRUSH-xps-5, CRUSH-xps-6, CRUSH-xps-7, CRUSH-xps-8, CRUSH-xps-9, CRUSH-xps-10, CRUSH-xps-11, CRUSH-xps-12 | `.rs` is enabled only after all parity gates; other walkers reuse dispatch |

CRUSH-xps-1 is the manual plugin MVP, not a prerequisite for the
source-language SDK phases. CRUSH-179 gates any later Cargo-managed build
proposal, not the manual host-selected wrapper model.

## Completion dispatch order

Review/merge existing xps-10–12 evidence with the SDK stack. Implement xps-8,
then xps-9, then xps-13's numeric portion; its string/kernel portion can proceed
after the shared conformance infrastructure. xps-8 must reject/correct incompatible
integer method results even if full AOT capability expansion waits for xps-13.
Full AOT capability support is not an extra prerequisite for the VM-only xps-4
CLI: its documented AOT refusals may remain. Source-level output/cast/format
misbehavior must be exact or refused before that CLI is enabled.
