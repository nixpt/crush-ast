# Crush 1.0 — release criteria

> **DRAFT for the captain (nixpt) to edit.** This is a planning doc, not a release.
> Nothing here changes versions, tags, publishing or release workflows.
> Ticket: [CRUSH-275](../../.jagent/planning/tickets/CRUSH-275-define-1.0-criteria.md).
> Surveyed against `origin/main` @ `5437ed64` (2026-10-18, workspace version 0.6.0);
> **re-scored against `faeb86e8` (2026-10-31, workspace 0.8.0)** — only the status/evidence of
> criteria whose state changed.
> **2026-11-13 (workspace 0.11.2): the captain ruled every freeze-blocking question** (§6). §1, §3,
> §4 and §5 now record those rulings; rows whose criterion a ruling settles say so.
> Status marks: ✅ true today · 🟡 partly true / true but unverified · ❌ not true.
> Every criterion is written so that someone can check it without asking the author.

---

## 0. TL;DR

- **1.0 is a compatibility promise, not a quality award.** After 1.0.0, anything in §1's
  "promised" list can only break in 2.0. So the question is not "is Crush good enough?"
  but "which surfaces are we willing to freeze?".
- **Today (2026-11-13): 22 ✅ / 23 🟡 / 22 ❌** across 67 criteria (table in §2.9; was 18 / 26 / 23 before the rulings, 15 / 26 / 26 on 2026-10-31 and 11 / 22 / 34 on 2026-10-18). The rulings (§6) settle L6, A8, S10 and T6. R1 (no accidental 1.0) is fixed by #348. The strengths are
  real: load-time version gates on CVM1/CASM/CAST, a gated polyglot capability, an MSRV
  job, a cross-backend agreement suite and a CI-verified language guide. The gaps are a
  normative spec, semver tooling, a backward-compatibility corpus, and the class of
  **silently-wrong** results that is still open across engines.
- **Decided (2026-11-13): a partial 1.0, by area** (§3). Stable core: the language core,
  the CAST/CASM formats, the Tier-1 engines and the `crush` CLI basics. Experimental in
  1.0: polyglot, walkers, AI ops, GPU, `spawn`, `impl`/`trait`/`@kind`.
- **Path (§4, renumbered 2026-11-13):** 0.12 "freeze prep" → 0.13 "feature freeze" →
  1.0.0-rc.N (≥ 4 weeks of soak, no stable-surface changes) → 1.0.0. 0.8–0.11 shipped as
  ordinary feature releases, so the old 0.8/0.9 gates moved to 0.12/0.13.

---

## 1. What 1.0 promises

### 1.1 Promised (breaking these after 1.0.0 needs 2.0.0)

| # | Surface | The promise |
|---|---|---|
| P1 | **Crush source language — the "stable core" subset** listed in the language reference (§2.1) | A program that compiles and runs on 1.x compiles and gives the same observable result on every later 1.y. |
| P2 | **CVM1 bytecode** (`.cvm1`), **CASM** (JSON), **CAST** (`.cast.json` / `.castb`) | A 1.x runtime loads any artifact a 1.0+ compiler wrote, and either runs it or refuses it **by name** (version error). It never silently mis-runs it. |
| P3 | **Capability model** | Code reaches outside the sandbox only through a capability that the program declared *and* the host granted. A missing grant fails loudly. It never becomes a silent no-op or a `null`. (Exact guarantees and non-guarantees are in §2.4.) |
| P4 | **`crush` CLI** — documented subcommands, flags, exit codes and `--message-format=json` | Scripts and CI that use them keep working. |
| P5 | **Capsule manifest** (`crush.toml`) and **lockfile**, if `crush-pkg` is in scope | A capsule that builds on 1.0 builds on 1.y. |
| P6 | **Stable-core Rust crates** (§3) | Normal Cargo semver, enforced by `cargo-semver-checks` in CI. |
| P7 | **Stable stdlib capabilities** (the `io.*`, `fs.*`, `env.*`, … set marked stable) | Name, arity, argument and result shapes, and error behaviour. |

### 1.2 Not covered by 1.0 (say so loudly, in the README and the reference)

- **Experimental language features**, gated by an annotation or flag: polyglot `@lang` blocks
  and polyglot imports (sandboxed by default, §6), `impl` / `trait` / `@kind` / COSM object
  semantics (`struct` itself is stable), AI-native opcodes and annotations,
  `semantic_switch`, `spawn` / structured concurrency, GPU/PTX, and the localised keyword
  sets (Nepali/CJK). `capability`, `let mut`, `pub`, `mcp` imports and `external` imports
  are **reserved words with no form** in 1.0.
- **Performance.** The execution *tier* a program lands on (CVM1, FastVM, JIT, AOT) is not
  part of the promise. Only the observable result is, and only on the **Tier-1 engines:
  the scheduler VM, PortableVm and FastVM**. AOT (Rust/C) is Tier-2 with documented gaps,
  but enforces the same capability checks. The JIT is experimental and off by default.
- **Determinism and fuel budgets** (S10), the **LSP** (T4) and the **debugger** (T6).
- **Walker coverage** of Python/JS/Rust/C/Go/… The walkers are best-effort lowering. What
  a walker supports can grow in a minor release. A walker that rejects code it used to
  accept is a bug, not a breaking change.
- **Diagnostics text** (message wording, codes other than documented ones), the REPL, the
  debugger wire protocol (M3), and internal Rust crates (§3, "0.x tier").
- **Platforms other than the ones in the Tier-1 list: Linux x86_64 and aarch64** (§6). macOS and others are Tier-2, best effort.

---

## 2. Criteria checklist

Each line: **criterion** — how to verify it — status — evidence.

### 2.1 Language

| ID | Criterion | Verify by | Status | Evidence |
|---|---|---|---|---|
| L1 | A **normative language reference** exists and states which constructs are stable. | The doc exists and has a "stability" column per construct. | ❌ | No spec in `docs/`. `docs/cast/schema-reference.md` covers the CAST JSON schema, not surface syntax. The guide's EBNF (`crush-language-guide/src/crush/syntax.md:414`) describes itself as "the grammar the parser implements, simplified", so the parser is the authority. |
| L2 | The reference's grammar and the **two parsers agree**: `crush-frontend` and `tree-sitter-crush`. | A CI job parses the whole example and guide corpus with both parsers. Zero ERROR nodes, zero disagreements. | 🟡 | **Partly done (2026-10-31).** CRUSH-207 (#342): the tree-sitter grammar now parses 49 of 51 `examples/crush/*.crush` without ERROR nodes (was 1/51), and `crates/tree-sitter-crush/tests/grammar_examples.rs` checks it in CI (2 files in `KNOWN_BAD`, with reasons). #347 adds slices and hex/binary literals. Still missing: a *differential* check against crush-frontend, and the guide's corpus. |
| L3 | Every keyword is either usable or explicitly reserved-for-future, and documented. | Diff the lexer keyword table against the reference. | 🟡 | Keywords are at `crush-frontend/src/parser/lexer.rs:507-542`. `capability` is reserved with no statement form (`parser/mod.rs:542` "TODO: Parse capability declarations"). `pub`, `mcp` imports and `external` imports have no guide coverage. `let mut` is rejected. **Ruled (§6):** all of them are reserved with no form in 1.0, and documented as reserved. |
| L4 | **No silently-wrong results** in stable-core constructs: open issues/tickets labelled silent-wrong on Tier-1 engines = 0. | Issue query. | 🟡 | Fixed and closed: #253, #254, #256, #257 (#306–#310), #267 (#319), #273 (#316). #258 (`@capabilities` ignored) and #261 (ungranted AI op → `null`) are fixed and closed (#380, #379). Open and near this line: #434 (crush-aotc compiles an unknown op to a comment) and #435 (a struct method named `pop`/`push` reroutes every `.pop()`/`.push()`). Walker silent-wrongs (#329–#331, C) are outside the stable core. FastVM is Tier-1 since the rulings, so its out-of-range reads returning `null` (#445) count here; #442 (negative-index writes dropped) has its fix in review (#448), and #425 (bitwise parity) in #449. |
| L5 | **Imports and modules work.** `import a.b` loads a module, or the reference says imports are not in 1.0. | Conformance test. | 🟡 | CRUSH-110 (in review): `import a.b [as x] [{ f }]` loads `a/b.crush` at compile time (`docs/design/import-system.md`, tests `crush-lang-sdk/tests/imports_cli.rs`). The language guide (`crush-language-guide`, `syntax.md`) still says imports load nothing. |
| L6 | **Object model decided.** Either COSM K1–K2 has landed and `struct`/`impl`/`trait`/`@kind` semantics are specified, or structs are frozen at today's semantics and COSM stays experimental. | Design doc status plus tickets. | ✅ | **Decided 2026-11-13 (§6):** `struct` is frozen at today's semantics; `impl`/`trait`/`@kind` ship experimental; COSM K2 continues after 1.0. State of the work: `docs/design/object-model.md` was approved 2026-10-15; K1.2–K1.4 have landed (CRUSH-zorro-40/41/42/43 Done, #211, #214); zorro-44, K2 (protocols) and K3 (walkers) have not started, and CRUSH-251 (JS object model) is Backlog. |
| L7 | **Error model is uniform**: `try`/`catch`/`throw` catch runtime errors on every Tier-1 engine. | Differential test. | ❌ | CRUSH-219 "runtime errors uncatchable; AOT has no try". #418 (2026-11-12): a cross-function catch on FastVM drops the callee's locals, and the JIT refuses a `try` it can't classify instead of mis-catching. Follow-ups are #427. |
| L8 | **Numeric semantics specified**: int width/overflow, int↔float, division and modulo sign, and the bigint decision. | Reference section plus a test per rule. | 🟡 | Div/mod truncate toward zero (`crush-vm/src/vm.rs:5`). **Ruled (§6):** i64 with checked overflow (ArithmeticOverflow) for 1.0; bigint later as a separate, opt-in type (CRUSH-zorro-36). Bitwise operators follow the same rules (#425, §6). |
| L9 | **Lambda/closure syntax** is reachable and has defined capture semantics. | Conformance test. | ✅ | Lambdas and closures parse, capture and run: `|a, b| { … }`, `|x, y| => x * y`, `|x| => x + n` (verified on main 2026-10-31; CRUSH-251 steps 1–5). The guide's closures chapter is CI-verified. CRUSH-75's lambda half is done; its remaining lexer fallback (`&` lexes as an identifier) is CRUSH-343. |
| L10 | Localised keyword sets (Nepali/CJK) are either stable and specified, or marked experimental. | Reference. | 🟡 | **Ruled experimental (§6).** Aliases are in the lexer. The Nepali ones are shown in the guide; CJK is mentioned only in passing (`syntax.md:107`). Open: label them experimental in the guide and the reference. |

### 2.2 Formats and backward compatibility

| ID | Criterion | Verify by | Status | Evidence |
|---|---|---|---|---|
| F1 | CVM1 has a **magic + version header** and a loader that rejects unknown versions by name. | Code. | ✅ | `MAGIC = b"CVM1"`, `VERSION`/`MIN_VERSION` (`crush-vm/src/bytecode.rs:49-60`). Range gate → `UnsupportedVersion` (`bytecode.rs:486-494`). Kind-table section with its own format byte (`:518-526`). |
| F2 | CASM JSON carries a version and a **major-version load gate**. | Code. | ✅ | `CASM_VERSION = "1.0"`, `check_version` (`casm/src/lib.rs:646,762`), enforced on parse (`:800`). |
| F3 | CAST carries a version and a load gate. | Code. | 🟡 | `CAST_VERSION = "0.1"` with a major-only gate (`crush-cast/src/pack.rs:31,101-121`). **With major 0 this gate accepts every 0.x.** The binary `.castb` has no magic; the format is picked by file extension (`pack.rs:10-13`). Needs CAST 1.0 plus a magic before CAST is promised. **Ruled (§6):** CAST and `.castb` go to 1.0 at the 0.13 feature freeze, and `.castb` gets a magic + version header. |
| F4 | **The written spec matches the format.** `casm_v1.0.md` covers every opcode the 1.0 runtime emits. | Opcode-table diff in CI. | ❌ | `crates/casm/specs/casm_v1.0.md` is marked "FROZEN" (2026-01-15), but COSM opcodes 0xC0–0xCA and the kind table (`bytecode.rs:7-22,40-43`) are not in it. "Frozen" has already drifted once. **Ruled (§6):** the spec absorbs the COSM opcodes 0xC0–0xCA. |
| F5 | A **backward-compat corpus** exists: artifacts written by each released version are loaded and run by the current runtime in CI. | A `tests/compat/v0.N/*.cvm1` directory plus a test. | ❌ | Two `.cvm1` files sit in `examples/crush/`, and no test references them. A version-2 blob is built by hand in a unit test (`crush-vm/src/tests/kind_table.rs:29-33`). That is good, but it is not a corpus. |
| F6 | **Deterministic output**: the same source and toolchain give byte-identical artifacts. | Build twice and diff. | ✅ | CRUSH-188 done (#324): CVM1 function layout is sorted, `crushc --emit casm` is byte-identical across runs (`compile::tests::casm_to_vm_lays_out_functions_in_name_order`). AOT: `crush-aot/tests/codegen_deterministic.rs`. |
| F7 | The COSM schema (embedded in CVM1 v3) is versioned and its evolution rule is written down. | Code plus doc. | 🟡 | `COSM_VERSION = "0.1.0"` (`crush-cosm/src/lib.rs:48`), with `#[serde(deny_unknown_fields)]` on the schema types (`crush-cosm/src/schema.rs:88,100,143,292`, `span.rs:12`). That is strict, so **any added field breaks old readers**. **Rule chosen (§6):** additive, ignore unknown: a new optional field is a minor bump that old readers skip; removing or changing a field is major. Open: switch the code to it. |
| F8 | The capsule manifest and lockfile carry a **schema version**. | Code. | ❌ | `crush-pkg/src/manifest.rs:24-40` has `capsule.version` (the *package* version) but no manifest-format version. **Ruled (§6):** add a format-version field before the freeze. |
| F9 | The versioning rule for every format is documented in one place (what bumps minor and what bumps major). | Doc. | ❌ | The rules are spread across code comments (`bytecode.rs:14-22`, `pack.rs:15-19`). **Ruled (§6):** one written rule per format, additive = minor, breaking = major. Open: the doc. |

### 2.3 Public Rust API

| ID | Criterion | Verify by | Status | Evidence |
|---|---|---|---|---|
| A1 | The stable-core crate list is decided and written down (§3). | This doc, once accepted. | ❌ | The scope is decided (partial 1.0 by area, §6); the crate-by-crate list in §3 Option B is not yet ratified. |
| A2 | `cargo-semver-checks` (or `cargo-public-api` snapshots) runs in CI for every stable-core crate. | A CI job. | ❌ | No API-diff tooling in `.github/workflows/` or `xtask/`. |
| A3 | Stable-core crates have **no third-party types in public signatures** unless that dependency is itself ≥1.0 and accepted, and no glob re-exports of other crates. | `cargo public-api` review. | ❌ | `crush-caison/src/lib.rs:18` does `pub use caison::*`; `crush-cast` exposes it as `pub mod caison` (named re-exports, `crush-cast/src/caison.rs:9`) plus a deprecated `cson` alias that glob-re-exports it (`crush-cast/src/lib.rs:5-10`, "removed in 0.5", still there). `crush-lang-js` exposes boa/swc types and `crush-lang-rust` exposes `syn`/`proc_macro2`. Many crates return `anyhow::Result`. **Ruled (§6):** stable crates expose their own Error types (no `anyhow::Result`) and no glob re-export of caison. |
| A4 | The **layering in the operating rules holds**: `crush-cast → casm → crush-errors`, and nothing in the IR layer depends on the VM. | `cargo tree -p crush-cast`. | ❌ | `crush-cast` → `crush-caison` (`crush-cast/Cargo.toml:27`) → `crush-vm` (`crush-caison/Cargo.toml:15`). The IR crate pulls in the whole VM. **Ruled (§6):** break this edge. |
| A5 | Public enums and structs that may grow are `#[non_exhaustive]` (errors, opcode/instruction enums, AST nodes, config structs). | grep plus review. | ❌ | Only 2 uses workspace-wide (`crush-vm/src/lang_exec.rs:47,83`). **Ruled (§6):** mandatory on growable public enums. |
| A6 | Stable-core crates build docs warning-free (`#![warn(missing_docs)]`) and docs.rs renders them. | CI `cargo doc -D warnings`. | ❌ | No `missing_docs` lints, no `[package.metadata.docs.rs]` and no `[workspace.lints]` anywhere. |
| A7 | Feature flags on stable crates are additive and documented, and `--no-default-features` and `--all-features` both build. | CI. | ✅ | The `feature-gates` job runs both (`ci.yml:551-571`). It does not yet document the flags. |
| A8 | An **MSRV policy** exists: which MSRV, how it moves (for example "N-2 stable, a bump is a minor release"), checked in CI. | Doc plus CI. | ✅ | **Policy (§6, ruled 2026-11-13): N-2 stable; a bump is a minor release.** `rust-version = "1.95.0"` (`Cargo.toml:64`) is checked by the `MSRV (1.95.0)` job (`ci.yml:40-58`, CRUSH-231). Clippy is pinned separately to 1.100.0 (#438). |
| A9 | Every published stable crate is on crates.io and has no path dependency outside the repo. | `cargo publish --dry-run`. | 🟡 | 27 crates are on crates.io at **0.8.0** (2026-10-30); 15 workspace crates have never been published (10 walkers, crush-aot, crush-net, crush-tui, crush-vm-capi, crush-walker). `crush-vm` still path-depends on `../../../buckets` (published as `crush-buckets` 0.1.0, 56 commits behind its master — nixpt/buckets#7). |
### 2.4 Runtime and security (the "capability-sandboxed" north star)

| ID | Criterion | Verify by | Status | Evidence |
|---|---|---|---|---|
| S1 | A **security model doc** (`SECURITY.md` + reference chapter) states what is guaranteed, on which engines, and what is not. | Doc exists. | ❌ | No `SECURITY.md`. The model is described in code comments (`crush-vm/src/vm.rs:1-6`) and in the guide's `capabilities.md`. |
| S2 | CVM1 enforces **declared ∩ granted** capabilities and has hard quotas (steps, stack, output, call depth). | Tests. | ✅ | `vm.rs:1-6`, `Quotas` (`vm.rs:844-890`). |
| S3 | **Declared capabilities survive compilation.** A `.cvm1` built by `crushc`/`crush build` carries `@capabilities`, and an undeclared cap fails at compile or load. | Test. | ✅ | #258 fixed by #380 (2026-10-31): `crushc` and `crush build` enforce `@capabilities` and keep the declaration in the artifact (`crush-lang-sdk/tests/crushc_capabilities.rs`). |
| S4 | **Polyglot is a capability**: `@lang` without a `polyglot.<lang>` grant is refused. | Test. | ✅ | `crush-lang-sdk/tests/polyglot_capability_test.rs` (7 tests, CRUSH-104 gate). |
| S5 | A *granted* polyglot block is **sandboxed by default**, or the docs say plainly that granting `polyglot.*` means "run arbitrary code as the host user". | Default features plus a live bwrap test in CI. | ❌ | CRUSH-236: `sandboxed-polyglot` is not a default feature (`crush-vm/Cargo.toml:75,85`). CI only `cargo check`s it, and `@lang[pypi:…]` deps are dropped. **Ruled (§6):** sandboxed by default; running unsandboxed needs an explicit flag; a dropped pinned dep becomes a loud error; zandbox (nixpt/zandbox) is to be evaluated as the backend (today: buckets). |
| S6 | Ungranted AI-native ops **fail closed and loudly** on every engine. | Test. | ✅ | #261 / CRUSH-234 fixed by #379 (2026-10-31): the scheduler VM and PortableVm fail closed on an ungranted AI op, and AOT refuses AI ops. #432 (#423, 2026-11-12) does the same for DOM ops on AOT, makes crush-aotc refuse `ai_*`/`dom_*`, and applies the embedder's allowlist in FastVM's `resolve_host_request` (`crush-aot/tests/unsupported_ops.rs`, `crush-vm/tests/dom_fail_closed.rs`). Open: #433 (FastVM reports an allowlist refusal like a failed handler; still fails closed). |
| S7 | **Wall-clock bounds** cover every blocking capability. | Test. | 🟡 | `EXEC_LANG` is killable. `CAP_CALL` is bounded only if the `HostCap` overrides `call_with_deadline` (`vm.rs:860-870`, CRUSH-19). CRUSH-40 is open. `parallel_map` items now share the caller's deadline on the scheduler and PortableVm (#422, fixed by #430). **Ruled (§6): in the 1.0 promise on the Tier-1 engines**: `HostCap::call_with_deadline` becomes mandatory and CRUSH-40 is in scope (overlaps #401: FastVM enforces no step or wall-time quota). |
| S8 | **Engine parity of the sandbox.** FastVM, JIT and AOT enforce the same capability checks as CVM1, *or* the docs say AOT native code runs with host authority. | Differential test per engine. | 🟡 | `crush-aot/tests/js_runtime_caps.rs` exists, and CRUSH-218 is open (FastVM cap errors). #432 (#423) closed the DOM/AI gaps on AOT, aotc and FastVM. Open: #436 (the JIT may run `str.ord` when it isn't granted). The native-AOT trust boundary is not stated anywhere. **Ruled (§6):** AOT enforces the same capability checks as CVM1 (Tier-2, but not a sandbox escape). |
| S9 | Fuzzing of the parser, CVM1 loader and JIT runs in CI (or nightly), with no open crashers. | CI job. | ❌ | No fuzz job. The JIT fuzz target disagrees with the interpreter on about 55/86 FastOps (CRUSH-59, M10). |
| S10 | A deterministic mode and fuel budgets exist, if they are part of the 1.0 story. | — | ✅ | **Not part of the 1.0 story (§6):** experimental, later. CRUSH-41 and CRUSH-42 stay Backlog (M7). |

### 2.5 Tooling

| ID | Criterion | Verify by | Status | Evidence |
|---|---|---|---|---|
| T1 | **One documented `crush` CLI** whose surface is the 1.0 promise. The many auxiliary binaries are either hidden or marked unstable. | `crush --help` matches the reference. | 🟡 | `crush` now fronts `run`, `build`, `repl`, `doctor`, `debug` (#341) and `gui` (#344). The auxiliary binaries (`crush-run`, `crushc`, `crush-compile`, `crush-repl`, `crush-diff`, …) still ship and are not marked unstable. **Ruled (§6):** stable entry points fold into `crush <sub>`; the remaining aux binaries are marked unstable. |
| T2 | CLI errors carry a source location, and output is not lost on a runtime error. | Tests. | 🟡 | CRUSH-184 done (#325): `crush-walk-run` keeps output before a runtime error and exits non-zero. #338: `crushc` and `crush-run` emit identical JSON records, and semantic errors carry line/col. Runtime errors still have no position (no pc→source map); CRUSH-190 is open. |
| T3 | `crush-pkg`: manifest schema frozen and versioned (see F8), with reproducible capsules. | Tests. | 🟡 | `crush-pkg` 0.6.0 is published and has `merkle.rs`/`signer.rs`. The manifest is unversioned. |
| T4 | **LSP / editor support** exists for the 1.0 grammar. | crush-lsp builds against crush-ast 1.0. | 🟡 | `crush-lsp` 0.1.0 is **published** (crates.io, 2026-10-31) on crush-frontend 0.8.0 with real diagnostics (crush-lsp#1). *Correction: it uses crush-frontend, not the tree-sitter grammar.* Diagnostic ranges are still 0:0 (no spans from the parse path). **Ruled (§6): experimental, not a 1.0 deliverable.** |
| T5 | The installer installs a pinned toolchain on Tier-1 platforms. | Smoke test in CI. | 🟡 | `crush-installer` 0.8.0 is published and installs `crush` and `crush-debug` (#341). Prebuilt Linux binaries are attached to every tag's pre-release from 0.9.0 (#346), with `cargo binstall` metadata. No install smoke test in CI. |
| T6 | Debugger: either M3 is done, or the debugger is excluded from 1.0. | — | ✅ | **Excluded from the 1.0 promise (§6): experimental.** `crush debug FILE` / `crush-debug` ship (#341): grant-gated stepping, breakpoints by source line, watch/print by name, NDJSON events. `spawn`/`await` programs are untried (it drives PortableVm, not the scheduler). |
### 2.6 Documentation

| ID | Criterion | Verify by | Status | Evidence |
|---|---|---|---|---|
| D1 | The language guide is **pinned to the release** and every code block is checked in CI. | The guide's CI. | ✅ | `crush-language-guide/CRUSH_AST_COMMIT` pins `c1cc21fb` (the 0.8.0 release); `verify-examples`, `check-examples.py`, the browser check and the notebooks job (blocking again, kernel 0.2.0) all run in its CI. |
| D2 | A failed guide verification **blocks deployment**. | Workflow. | ❌ | In `gh-pages.yml`, `deploy` has no `needs: verify`. |
| D3 | The guide marks every experimental feature (matching §1.2). | Review. | 🟡 | The guide uses "*not yet implemented*" and runfail markers (polyglot 4, structs 3, capabilities 3, concurrency 2, …). It has no stable/experimental labels. |
| D4 | crushlang.org is accurate for the release and its samples run. | WEB-1 closed. | 🟡 | crushlang.org was updated 2026-10-31 (crush-website#10): runtime from crush-ast `c1cc21fb`, grants, sandboxed `@javascript`/`@python`; a 20-case headless-Chromium check passed against the live site. WEB-1 (content audit) is still Backlog. |
| D5 | Migration guide 0.x → 1.0, plus a written deprecation policy (§4.4). | Doc. | ❌ | — |
| D6 | Internal status docs are current. | Review. | 🟡 | The planning survey (#339, 2026-10-31) re-checked 86 ticket statuses against merged PRs and updated BACKLOG-INDEX, ROADMAP and MILESTONES. 147 Backlog tickets were not individually re-verified. 2026-11-13: S3, S6 and Q6 rows re-checked against the merged fixes, the §2.9 scoreboard recounted from the rows, and the #411–#421 review follow-ups (#425–#436) indexed in BACKLOG-INDEX. |
### 2.7 Quality gates (CI)

| ID | Criterion | Verify by | Status | Evidence |
|---|---|---|---|---|
| Q1 | CI is green on `main` and required on PRs. | `gh run list`. | ✅ | The last 4 `main` runs pass. One failure on 2026-10-16 (#259). |
| Q2 | MSRV is checked. | CI. | ✅ | `ci.yml:40-58`. |
| Q3 | Cross-backend agreement and example parity run on every PR. | CI. | ✅ | `backend_agreement` + `examples_parity` (`ci.yml:256-258`). AOT tests are sharded. |
| Q4 | AOT C runtime under GC stress. | CI. | ✅ | `ci.yml:262`. |
| Q5 | The **conformance corpus** runs in CI and passes 100% of its non-skipped cases. | CI. | ❌ | CRUSH-211: `conformance` gives 18 pass / 19 fail / 12 skip and is not in CI. |
| Q6 | Clippy runs with `-D warnings` (or a curated deny list) on the stable core. | CI. | ✅ | The job blocks on `-D warnings` over `--workspace --all-targets` (#424); crush-gui is linted in its own job, crush-python is left out of it (needs libpython). |
| Q7 | **Multi-OS** (Linux, macOS, Windows if promised) and multi-arch (x86_64, aarch64). | CI matrix. | ❌ | All jobs are `ubuntu-latest`. CRUSH-49 and CRUSH-22 are open. **Ruled (§6):** Tier-1 is Linux x86_64 + aarch64, so CI needs an aarch64 Linux job; macOS and others are Tier-2. |
| Q8 | `cargo-deny` / `cargo-audit` (licences, advisories). | CI. | ❌ | None. |
| Q9 | wasm32 builds for the browser-facing crates. | CI. | ✅ | `ci.yml:377-378` (walkers), `crush-web` headless Chromium (`ci.yml:579-613`). |
| Q10 | The guide's examples run against the release candidate's commit. | Guide CI pinned to the rc tag. | 🟡 | The mechanism exists (D1). It is not yet tied to tags. |

### 2.8 Release engineering

| ID | Criterion | Verify by | Status | Evidence |
|---|---|---|---|---|
| R1 | **No accidental 1.0.** While major = 0, a `feat!:` / `BREAKING CHANGE` commit bumps *minor*. 1.0.0 is minted only on purpose. | Read `bump-version.sh`. | ✅ | **Done (#348, merged 2026-10-31):** on 0.x a breaking change (`feat!:` / `BREAKING CHANGE`) bumps minor; 1.0.0 needs `BUMPVER_ALLOW_MAJOR=1` (release.yml `allow_major`). squadron's `bump-version` template still needs the same change, or `bump-version --install` reverts it. |
| R2 | Versioning can **differ per crate** (needed for a partial 1.0). | Tooling. | ❌ | The workspace is lock-step (`version.workspace`, `Cargo.toml:59`), and `bump-version.sh` and `publish-crates.py` assume it. |
| R3 | A pre-release (`1.0.0-rc.N`) can be cut and published. | Dry run. | 🟡 | From 0.9.0 every tag gets a GitHub **pre-release** (`--prerelease --latest=false`, #346). `1.0.0-rc.N` version suffixes are still untested in `bump-version.sh` and `publish-crates.py`. |
| R4 | Every crate in the release is publishable by CI. No manual first publish. | `publish-crates.py` plan. | 🟡 | 27 crates publish from CI (Trusted Publishing; #320 lists every crate that needs a config instead of stopping at the first). 15 never-published crates each need a manual first `cargo publish`. |
| R5 | The CHANGELOG has a **"Breaking"** section per release, separate from the mechanical entries. | Review. | 🟡 | `CHANGELOG.md` follows Keep a Changelog "loosely". Entries are auto-appended. |
| R6 | Release binaries for Tier-1 platforms are attached to the GitHub release. | Release page. | ✅ | Mechanism merged (#346): `release-artifacts` builds Linux x86_64 binaries (`crush`, `crushc`, `crush-run`, `crush-compile`, `crush-repl`, `crush-debug`, `crush-pkg`, `crush-gui`), the crush-web pkg and `SHA256SUMS`, and attaches them to the tag's pre-release. First real run: 0.9.0. Linux only (see Q7). |
| R7 | The open-P1 count is triaged: each P1 is fixed, or scoped out of 1.0 with a note. | Board. | ❌ | **63 open P1 ticket files** on main (2026-10-31; was 72). Not yet triaged against §3's scope. |
### 2.9 Scoreboard

| Group | ✅ | 🟡 | ❌ | Total |
|---|---|---|---|---|
| 2.1 Language | 2 | 6 | 2 | 10 |
| 2.2 Formats / compat | 3 | 2 | 4 | 9 |
| 2.3 Public Rust API | 2 | 1 | 6 | 9 |
| 2.4 Runtime / security | 5 | 2 | 3 | 10 |
| 2.5 Tooling | 1 | 5 | 0 | 6 |
| 2.6 Docs | 1 | 3 | 2 | 6 |
| 2.7 Quality gates | 6 | 1 | 3 | 10 |
| 2.8 Release engineering | 2 | 3 | 2 | 7 |
| **Total** | **22** | **23** | **22** | **67** |
---

## 3. Scope decision: full 1.0 vs partial 1.0

> **Decided 2026-11-13 (§6): partial 1.0 by area** — Option B below. Stable
> core: the language core, the CAST/CASM formats, the Tier-1 engines and the `crush` CLI
> basics. Experimental: polyglot, walkers, AI ops, GPU, `spawn`. The crate-by-crate list
> and the embedder surface (facade crate vs `crush_lang_sdk::stable`) are still to ratify.

### Option A — full 1.0 (every published crate goes to 1.0.0)

- **Pros:** one number to talk about, and nothing to explain.
- **Cons:** it freezes `crush-vm` (≈600 public items across 56 `pub mod`s, much of it
  internals like `rust_methods_core` and `js_ops`), `crush-lang-sdk` (≈300 items,
  11 features, re-exports VM types at `crush-lang-sdk/src/lib.rs:67-75`), every walker
  (which expose `syn`/`swc`/`boa` types), JIT and AOT, all mid-milestone (M2, M3, M5, M6, M10).
  Either we stop changing them, or we cut 2.0 within months. Both undercut what a 1.0
  is supposed to mean.
- **Earliest realistic date:** after M2, M5, M6, M9 and COSM K3. Many months.

### Option B — partial 1.0, by surface (chosen)

1.0 freezes **the language core, the formats, the capability contract and the `crush` CLI**
(P1–P4, P7), plus a small set of foundation crates. Everything else keeps its own 0.x
version and its own pace. This is the same split many language projects use: the language
and the artifacts are stable, and the compiler internals are not.

**Stable core (1.x semver, checked by `cargo-semver-checks`):**

| Crate | Why it is in the core | Gate before it can be 1.0 |
|---|---|---|
| `crush-errors` | Leaf, 49 items, no internal deps. Owns `VersionMismatch` (the format gates' error type). | A5 (`non_exhaustive` on error enums) |
| `crush-diagnostics` | Leaf, 14 items. The diagnostic wire shape tools consume. | Doc the JSON shape |
| `casm` | **The bytecode contract.** 11 dependents. | F4 (spec matches opcodes), F5 (compat corpus). Its `Program.cosm` field exposes `crush_cosm::CosmModule` publicly (`casm/src/lib.rs:668,682`), so **either `crush-cosm` comes along or that field becomes opaque.** |
| `crush-cosm` | Comes in only with `casm`. 102 items, no deps. | COSM K2 landed and F7 evolution rule chosen. *If that is too far away, make `casm`'s COSM fields private and keep cosm 0.x.* |
| `crush-cast` | The IR, with 20 dependents (the most-used crate). | A4 (drop the `crush-caison` → `crush-vm` edge), A3 (no `pub use caison::*`), F3 (CAST 1.0 + `.castb` magic) |
| `crush-ffi` | C ABI contract, 16 items. | ABI doc + header test |
| `tree-sitter-crush` | Editors depend on it. 2 items. | L2 (CRUSH-207 closed) |
| `crush-pkg` (manifest types only) *(optional)* | Capsules are user-visible artifacts. | F8, T3. *Or keep it 0.x and promise only the `crush.toml` format (P5).* |

**Stays 0.x (experimental tier):** `crush-vm`, `crush-frontend`, `crush-lang-sdk` (broad
surface), `crush-jit`, `crush-aot`/`crush-aotc`, every `crush-lang-*` walker,
`crush-walker-core`, `crush-index`, `crush-debugger`, `crush-net`, `crush-ptx`,
`crush-caison`/`crush-cson`, `crush-lint`, `crush-macros`, `crush-tui`, `crush-vm-capi`,
`crush-web`, `crush-vm-py`, `crush-python`, `crush-installer`, `crush-plugin-example`.

**Host embedding at 1.0.** The SDK is how hosts like openko/fabric, arniko and
mycelium-mobile embed Crush. It is too large to freeze whole. Two ways to give embedders a
1.0 surface:

1. Add a curated `crush` facade crate at 1.0 (`compile`, `run`, `HostCap`, `Quotas`,
   `Value`, `Error`) that *owns* its types, rather than re-exporting `crush-vm`'s.
2. Or add a `crush_lang_sdk::stable` module checked by semver-checks, with the rest of the
   SDK behind an `unstable` feature.

Either way, **a stable crate must not re-export a type from a 0.x crate.** If it does, a
0.x bump becomes a hidden break.

**Cost of B:** it needs R2 (per-crate versions in `bump-version.sh` and
`publish-crates.py`) and a "this crate is 0.x" banner in each experimental crate's README.

### Option C — "language 1.0, crates all 0.x"

Promise only P1–P4 (language, formats, capabilities, CLI). Every Rust crate stays 0.x.
This is the cheapest option and needs no release-tooling change. It is honest if embedders
are all in-house today. Its weakness: an external embedder gets no Rust stability at all.

**Recommendation: B, starting as C.** Ship 1.0.0 with the language, format, capability
and CLI promises, and the leaf crates (`crush-errors`, `crush-diagnostics`, `crush-ffi`,
`tree-sitter-crush`) at 1.0. Promote `casm`+`crush-cosm` and `crush-cast` to 1.x as
their gates close (in a 1.y minor; promoting a crate to stable is not a breaking change).
The facade crate can follow in 1.1.

---

## 4. The path

### 4.1 Release train

*Renumbered 2026-11-13 (§6). 0.8–0.11 shipped as ordinary feature releases; the old 0.8
gates moved to 0.12 and the old 0.9 gates to 0.13.*

| Release | Theme | Exit criteria (from §2) |
|---|---|---|
| **0.12.0 — freeze prep** | Tooling and the rulings land | R1 (done, #348), A2 semver-checks wired (report-only), F5 compat corpus seeded with released artifacts, F9 format-versioning doc, S1 `SECURITY.md` draft, L1 reference skeleton with stability column, Q5 conformance in CI (allowed-failures list). The §6 rulings land in code where they change behaviour (string units, `CapFailed` fatal, keyword field names, literal callees, COSM additive schema, `crush.toml` format version, JIT off by default). The scope decision (§3) and the COSM decision (L6) are made. |
| **0.13.0 — feature freeze** | Close the stable surface | L2, L4 (zero silent-wrong on Tier-1), L5, L7, F3 (CAST/`.castb` 1.0 with magic), F4, A3, A4, A5, S3, S5, S6, S7, S8, T1, T2, D3, R2, R7 triaged. **No new syntax after 0.13.0**, except behind an experimental gate. |
| **1.0.0-rc.1 … rc.N** | Soak | All ❌ in the stable scope closed or explicitly scoped out. Then only bug fixes. **Any change to a promised surface restarts the clock with a new rc.** Minimum 4 weeks from the last rc to 1.0.0. The guide (D1/D2) and website (D4) are pinned to the rc tag. A downstream canary (openko/fabric, arniko, surfer) builds against each rc. |
| **1.0.0** | Promise starts | Tag. Changelog "Breaking since 0.x" section. Migration guide (D5). |

### 4.2 Milestones and tickets: close, or scope out of 1.0

| Item | Proposed 1.0 treatment |
|---|---|
| M2 JIT completion / M10 (CRUSH-59/60/61) | **Out (ruled).** The JIT is an experimental tier, **off by default (opt-in)**; values past 48 bits deoptimize to the VM instead of truncating. Silently-wrong output on an opt-in tier is still silently wrong, so it must deopt or refuse rather than diverge. |
| M3 debugger | **Out.** |
| M4 / M9 cross-project convergence | **Out**, except the stdlib capability set that 1.0 marks stable (P7). |
| M5 AI-native layer (CRUSH-30) | Annotations that are already CAST nodes: **experimental**. S6 (fail-closed) is **in**. |
| M6 walker parity (CRUSH-21/35-39), CRUSH-251 | **Out** (walkers are best-effort, §1.2). |
| M7 runtime hardening (CRUSH-40/41/42) | CRUSH-40 (timeouts) **in** (S7 is promised). 41/42 **out** (S10, experimental, later). |
| M8 platforms (CRUSH-22/49) | **Linux x86_64 + aarch64 Tier-1 at 1.0** (ruled). macOS and others Tier-2, best effort; Android/RISC-V later. |
| COSM K1.5/K2 (zorro-44, K2) | **Decided (L6):** `struct` frozen; `impl`/`trait`/`@kind` experimental; K2 continues after 1.0. (K1.2 zorro-40/41 are Done, #211/#214.) |
| CRUSH-207 grammar drift | **In** (L2, T4). |
| CRUSH-110 imports | **In**, or remove `import` from the stable core. |
| CRUSH-211 conformance, 188, 190, 184, 191, 217, 219, 236, 234 | **In.** |
| Open issues #253–#258, #260, #261, #266, #267, #273, #278, #279 | **In** if they touch the stable core. The polyglot ones (#255, #266, #278, #279) can be deferred with polyglot marked experimental. |

### 4.3 Release-candidate process

1. Cut `release/1.0` from `main` at rc.1. Only fixes are cherry-picked onto it.
2. Each rc: full CI, plus the compat corpus, plus guide verification at the tag, plus the
   downstream canary builds, plus `cargo-semver-checks` against the previous rc (it must
   be clean).
3. Publish rcs to crates.io as `1.0.0-rc.N`. Cargo will not pick these up for `^0.x` or
   `^1` users unless they ask for them.
4. 1.0.0 = the last rc with the suffix removed and no code change.

### 4.4 Deprecation and migration policy (ratified 2026-11-13)

- A promised item is **deprecated in a minor release** with a compile-time warning (a
  diagnostic code for Crush source, `#[deprecated]` for Rust) and a replacement in place.
- It is **removed only in the next major release**, and no sooner than **2 minor releases
  or 6 months** after deprecation, whichever is longer.
- Every deprecation lands with a CHANGELOG entry and a migration note. Where a mechanical
  fix exists, it ships as a `crush fix` rewrite (CAST → CAST).
- Experimental features may change in any minor release, but with a changelog note.
  Leaving experimental status is itself a minor-release event.

### 4.5 Versioning rules after 1.0: what counts as breaking

| Surface | **Major** (breaking) | **Minor** | **Patch** |
|---|---|---|---|
| **Syntax** | Removing or changing the meaning of stable syntax. Reserving a new keyword that was a legal identifier. | New syntax that was a parse error before. New contextual keywords. | Parser bug fixes that only reject inputs the reference already said are invalid. |
| **Semantics** | Changing the observable result of a stable-core program on a Tier-1 engine (outputs, errors raised, capability calls made). | New stdlib functions. New optional parameters. | Fixing a result that contradicted the reference. **Ratified (§6):** fixing a silently-wrong result is a *patch* even if someone depended on the bug; it gets a changelog note. |
| **Bytecode / formats** | A runtime can no longer load an artifact written by an earlier 1.x compiler. A format major bump. | New opcodes or sections that old runtimes refuse **by name** (the CVM1 v3 pattern, `bytecode.rs:14-22`). A format minor bump. | — |
| **Capabilities** | Removing a stable capability. Changing its args or results. **Widening** what a grant allows. | New capabilities. New optional fields. | Tightening a check to match the documented model. |
| **Stdlib** | Removal, rename, or a signature/result-shape change. | Additions. | Fixes. |
| **CLI** | Removing or renaming a documented subcommand or flag. Changing exit codes or the `--message-format=json` schema. | New subcommands, flags, or JSON fields. | Message-text changes. |
| **Rust API (stable crates)** | Anything `cargo-semver-checks` flags as major. | Additive changes. | Fixes. |
| **MSRV** | — | A bump, per policy (A8: N-2 stable). | — |

---

## 5. Questions for the captain (answered 2026-11-13)

1. **Which shape: A, B or C?** → **B, partial 1.0 by area.** The external-embedder
   question (facade crate vs `crush_lang_sdk::stable`) is still open.
2. **COSM before or after 1.0?** → **After.** `struct` is frozen as it is;
   `impl`/`trait`/`@kind` ship experimental; K2 continues after 1.0.
3. **Is polyglot part of the 1.0 promise?** → **No, experimental**, but **sandboxed by
   default**, with an explicit flag to run unsandboxed and a loud error for dropped
   pinned deps. Evaluate zandbox as the sandbox backend.
4. **Which engines are Tier-1?** → **Scheduler VM + PortableVm + FastVM.** AOT is Tier-2
   (documented gaps, same capability checks); the JIT is experimental and off by default.
5. **Fixing silently-wrong bugs after 1.0?** → **Patch**, with a changelog note.
6. **Platforms?** → **Linux x86_64 + aarch64** Tier-1; macOS and others Tier-2.
7. **Is the LSP a 1.0 deliverable?** → **No, experimental.** So is the debugger.
8. **Determinism and fuel (S10)?** → **Experimental, later.**
9. **Binaries?** → **Fold the stable entry points into `crush <sub>`**; mark the rest
   unstable.
10. **Timeline?** → **Renumbered:** 0.12 freeze prep, 0.13 feature freeze, then 1.0-rc.

## 6. Captain's rulings (2026-11-13)

Recorded by the foreman as the captain ruled, from the open-decision sweep of
2026-11-13. "Criteria" names the rows above that each ruling settles or steers. Items not
listed here are still open (for example COSM method dispatch, crush-aotc delete vs
parity, augmented assignment on mutable values, string-op coercion).

| Ruling | Decision | Criteria |
|---|---|---|
| Release train | 0.12 = freeze prep (the rulings land); 0.13 = feature freeze (all old 0.9 gates); then 1.0-rc | §4.1 |
| 1.0 scope | Partial 1.0 by area. Stable core: language core, CAST/CASM formats, Tier-1 engines, CLI basics. Experimental: polyglot, walkers, AI ops, GPU, `spawn` | §3, A1 |
| Tier-1 engines | Scheduler VM + PortableVm + FastVM. AOT Tier-2 with documented gaps; JIT experimental | §1.2, L4 |
| Object model | `struct` frozen as it is; `impl`/`trait`/`@kind` experimental; COSM K2 after 1.0 | L6 |
| String index units (#368) | All characters: `len`, `index_of`, `s[i]` and slices count Unicode characters; add explicit byte APIs (`byte_len`, `bytes()`) | L4, P1 |
| Capability refusals (#427) | Fatal, not catchable: `CapFailed` behaves like `CapDenied`; never silently swallowed | L7, P3 |
| Negative-index writes (#442) | Python-style normalisation on every engine; writes match reads | L4 |
| Bitwise operators (#425) | Int operands only, else a type error; a shift count outside 0..=63 is ArithmeticOverflow; full i64 on every engine; JS keeps refusing them | L4, L8 |
| Float print (#393) | Keep Rust `Display` (today's behaviour on every engine); document it | L1 |
| Polyglot sandbox | Sandboxed by default; unsandboxed needs an explicit flag; a dropped pinned dep is a loud error; polyglot stays experimental; evaluate zandbox as the backend | S5 |
| Native AOT trust boundary | AOT enforces the same capability checks as CVM1 | S8 |
| JIT default and width | Off by default (opt-in); values past 48 bits deoptimize to the VM instead of truncating | §4.2 M2 |
| Integer width | i64 with checked overflow for 1.0; bigint later as a separate, opt-in type | L8 |
| Keyword field names (#428) | Allowed everywhere (literals, struct fields, member access) | L3, L2 |
| Literal callees (`5(3)`, `"s"(1)`) | Rejected statically, at parse or compile time | L3 |
| Reserved keywords | `capability`, `let mut`, `pub`, `mcp`/`external` imports: reserved, no forms in 1.0, documented as reserved | L3 |
| Localised keywords | Experimental, outside the 1.0 syntax promise | L10 |
| CAST and `.castb` | 1.0 at the 0.13 feature freeze; `.castb` gets a magic + version header | F3 |
| Format versioning | One written rule per format: additive = minor, breaking = major; the casm spec absorbs the COSM opcodes 0xC0–0xCA | F4, F9 |
| COSM schema evolution | Additive, ignore unknown: a new optional field is a minor bump that old readers skip; removing or changing a field is major | F7 |
| Silently-wrong fix after 1.0 | Patch release with a changelog note | §4.5 |
| Deprecation policy | §4.4 ratified as written | §4.4, D5 |
| CLI | Stable entry points fold into `crush <sub>`; other aux binaries marked unstable; LSP and debugger experimental | T1, T4, T6 |
| Platforms | Linux x86_64 + aarch64 Tier-1; macOS and others Tier-2 best effort; Android/RISC-V later | §1.2, Q7 |
| Public API hygiene | Stable crates expose their own Error types (no `anyhow::Result`); no glob re-export of caison; break the `crush-cast → crush-caison → crush-vm` edge; `#[non_exhaustive]` mandatory on growable public enums | A3, A4, A5 |
| MSRV | N-2 stable; a bump is a minor release | A8 |
| `crush.toml` | Add a format-version field before the freeze; runtime pins, lockfile and `crush env` stay later | F8, T3 |
| Wall-clock bounds | In the 1.0 promise on Tier-1 engines: `HostCap::call_with_deadline` mandatory; CRUSH-40 in scope | S7 |
| Determinism, fuel budgets | Experimental, later (CRUSH-41/42) | S10 |

---

*Sources: `.jagent/planning/MILESTONES.caison`, `ROADMAP.md`, ticket files under
`.jagent/planning/tickets/`, `gh issue list -R nixpt/crush-ast` (15 open, 2026-10-19),
`.github/workflows/*.yml`, code as cited, `crush-language-guide` @ `dfe88f3`,
`crush-website` @ `b1ff932`, crates.io API (2026-10-19).*
