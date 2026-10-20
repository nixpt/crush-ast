# Crush 1.0 — release criteria

> **DRAFT for the captain (nixpt) to edit.** This is a planning doc, not a release.
> Nothing here changes versions, tags, publishing or release workflows.
> Ticket: [CRUSH-275](../../.jagent/planning/tickets/CRUSH-275-define-1.0-criteria.md).
> Surveyed against `origin/main` @ `5437ed64` (2026-10-18, workspace version 0.6.0).
> Status marks: ✅ true today · 🟡 partly true / true but unverified · ❌ not true.
> Every criterion is written so that someone can check it without asking the author.

---

## 0. TL;DR

- **1.0 is a compatibility promise, not a quality award.** After 1.0.0, anything in §1's
  "promised" list can only break in 2.0. So the question is not "is Crush good enough?"
  but "which surfaces are we willing to freeze?".
- **Today: 11 ✅ / 22 🟡 / 34 ❌** across 67 criteria (table in §2.9). The strengths are
  real: load-time version gates on CVM1/CASM/CAST, a gated polyglot capability, an MSRV
  job, a cross-backend agreement suite and a CI-verified language guide. The gaps are a
  normative spec, semver tooling, a backward-compatibility corpus, and the class of
  **silently-wrong** results that is still open across engines.
- **Recommendation: a partial 1.0, by surface** (§3). Freeze *the language core, the
  on-disk formats and the `crush` CLI*, plus a small set of foundation crates. Keep
  runtimes, JIT, AOT, walkers, the SDK's broad surface and COSM at 0.x until their
  milestones close.
- **Path (§4):** 0.7 (vega, in flight) → 0.8 "freeze prep" → 0.9 "beta / feature
  freeze" → 1.0.0-rc.N (≥ 4 weeks of soak, no stable-surface changes) → 1.0.0.

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
  and polyglot imports, `@kind` / COSM object semantics, AI-native opcodes and annotations,
  `semantic_switch`, `spawn` / structured concurrency, `mcp` imports and `external` imports
  (`@http`/`@git`/`@file`), GPU/PTX.
- **Performance.** The execution *tier* a program lands on (CVM1, FastVM, JIT, AOT) is not
  part of the promise. Only the observable result is, and only on the engines listed as
  Tier 1.
- **Walker coverage** of Python/JS/Rust/C/Go/… The walkers are best-effort lowering. What
  a walker supports can grow in a minor release. A walker that rejects code it used to
  accept is a bug, not a breaking change.
- **Diagnostics text** (message wording, codes other than documented ones), the REPL, the
  debugger wire protocol (M3), and internal Rust crates (§3, "0.x tier").
- **Platforms other than the ones in the Tier-1 list.** Today that list is Linux x86_64 only
  (§2.8).

---

## 2. Criteria checklist

Each line: **criterion** — how to verify it — status — evidence.

### 2.1 Language

| ID | Criterion | Verify by | Status | Evidence |
|---|---|---|---|---|
| L1 | A **normative language reference** exists and states which constructs are stable. | The doc exists and has a "stability" column per construct. | ❌ | No spec in `docs/`. `docs/cast/schema-reference.md` covers the CAST JSON schema, not surface syntax. The guide's EBNF (`crush-language-guide/src/crush/syntax.md:414`) describes itself as "the grammar the parser implements, simplified", so the parser is the authority. |
| L2 | The reference's grammar and the **two parsers agree**: `crush-frontend` and `tree-sitter-crush`. | A CI job parses the whole example and guide corpus with both parsers. Zero ERROR nodes, zero disagreements. | ❌ | CRUSH-207 (P1, Backlog): every one of the 43/43 example files parses with ERROR nodes in tree-sitter. tree-sitter rejects `//` comments, reassignment, `-> T` and `@decision`. `grammar.js` was last changed 2026-06-12. The editors and crush-lsp use this grammar. |
| L3 | Every keyword is either usable or explicitly reserved-for-future, and documented. | Diff the lexer keyword table against the reference. | 🟡 | Keywords are at `crush-frontend/src/parser/lexer.rs:507-542`. `capability` is reserved with no statement form (`parser/mod.rs:542` "TODO: Parse capability declarations"). `pub`, `mcp` imports and `external` imports have no guide coverage. `let mut` is rejected. |
| L4 | **No silently-wrong results** in stable-core constructs: open issues/tickets labelled silent-wrong on Tier-1 engines = 0. | Issue query. | ❌ | Open: #253 (`return match` → null), #254 (negative slices), #256, #257, #267, #273 (JIT locals overwritten), #255 (`@bash` vars empty, PR #277), CRUSH-217 (indexing/printing diverge across engines), CRUSH-191 (parser accepts malformed input). |
| L5 | **Imports and modules work.** `import a.b` loads a module, or the reference says imports are not in 1.0. | Conformance test. | ❌ | CRUSH-110 "import is a no-op". The guide says `import a.b` parses but loads nothing (`syntax.md:365`). |
| L6 | **Object model decided.** Either COSM K1–K2 has landed and `struct`/`impl`/`trait`/`@kind` semantics are specified, or structs are frozen at today's semantics and COSM stays experimental. | Design doc status plus tickets. | 🟡 | `docs/design/object-model.md` was approved 2026-10-15. K1.3/K1.4 have landed (CRUSH-zorro-42/43 Done: kind table, VM member ops). zorro-40/41/44 are Backlog, K2 (protocols) and K3 (walkers) have not started, and CRUSH-251 (JS object model) is Backlog. **This is the biggest semantic decision still open before 1.0.** |
| L7 | **Error model is uniform**: `try`/`catch`/`throw` catch runtime errors on every Tier-1 engine. | Differential test. | ❌ | CRUSH-219 "runtime errors uncatchable; AOT has no try". |
| L8 | **Numeric semantics specified**: int width/overflow, int↔float, division and modulo sign, and the bigint decision. | Reference section plus a test per rule. | 🟡 | Div/mod truncate toward zero (`crush-vm/src/vm.rs:5`). The bigint decision is open (CRUSH-zorro-36). |
| L9 | **Lambda/closure syntax** is reachable and has defined capture semantics. | Conformance test. | 🟡 | CRUSH-75 (lambda syntax unreachable in lexer) is open. Closures are documented in the guide. |
| L10 | Localised keyword sets (Nepali/CJK) are either stable and specified, or marked experimental. | Reference. | 🟡 | Aliases are in the lexer. The Nepali ones are shown in the guide; CJK is mentioned only in passing (`syntax.md:107`). |

### 2.2 Formats and backward compatibility

| ID | Criterion | Verify by | Status | Evidence |
|---|---|---|---|---|
| F1 | CVM1 has a **magic + version header** and a loader that rejects unknown versions by name. | Code. | ✅ | `MAGIC = b"CVM1"`, `VERSION`/`MIN_VERSION` (`crush-vm/src/bytecode.rs:49-60`). Range gate → `UnsupportedVersion` (`bytecode.rs:486-494`). Kind-table section with its own format byte (`:518-526`). |
| F2 | CASM JSON carries a version and a **major-version load gate**. | Code. | ✅ | `CASM_VERSION = "1.0"`, `check_version` (`casm/src/lib.rs:646,762`), enforced on parse (`:800`). |
| F3 | CAST carries a version and a load gate. | Code. | 🟡 | `CAST_VERSION = "0.1"` with a major-only gate (`crush-cast/src/pack.rs:31,101-121`). **With major 0 this gate accepts every 0.x.** The binary `.castb` has no magic; the format is picked by file extension (`pack.rs:10-13`). Needs CAST 1.0 plus a magic before CAST is promised. |
| F4 | **The written spec matches the format.** `casm_v1.0.md` covers every opcode the 1.0 runtime emits. | Opcode-table diff in CI. | ❌ | `crates/casm/specs/casm_v1.0.md` is marked "FROZEN" (2026-01-15), but COSM opcodes 0xC0–0xCA and the kind table (`bytecode.rs:7-22,40-43`) are not in it. "Frozen" has already drifted once. |
| F5 | A **backward-compat corpus** exists: artifacts written by each released version are loaded and run by the current runtime in CI. | A `tests/compat/v0.N/*.cvm1` directory plus a test. | ❌ | Two `.cvm1` files sit in `examples/crush/`, and no test references them. A version-2 blob is built by hand in a unit test (`crush-vm/src/tests/kind_table.rs:29-33`). That is good, but it is not a corpus. |
| F6 | **Deterministic output**: the same source and toolchain give byte-identical artifacts. | Build twice and diff. | 🟡 | `crush-aot/tests/codegen_deterministic.rs` covers AOT. CRUSH-188 (non-deterministic function layout) is open for CVM1. |
| F7 | The COSM schema (embedded in CVM1 v3) is versioned and its evolution rule is written down. | Code plus doc. | 🟡 | `COSM_VERSION = "0.1.0"` (`crush-cosm/src/lib.rs:48`), with `deny_unknown_fields` throughout `schema.rs`. That is strict, so **any added field breaks old readers**. Pick an evolution rule before freezing. |
| F8 | The capsule manifest and lockfile carry a **schema version**. | Code. | ❌ | `crush-pkg/src/manifest.rs:24-40` has `capsule.version` (the *package* version) but no manifest-format version. |
| F9 | The versioning rule for every format is documented in one place (what bumps minor and what bumps major). | Doc. | ❌ | The rules are spread across code comments (`bytecode.rs:14-22`, `pack.rs:15-19`). |

### 2.3 Public Rust API

| ID | Criterion | Verify by | Status | Evidence |
|---|---|---|---|---|
| A1 | The stable-core crate list is decided and written down (§3). | This doc, once accepted. | ❌ | — |
| A2 | `cargo-semver-checks` (or `cargo-public-api` snapshots) runs in CI for every stable-core crate. | A CI job. | ❌ | No API-diff tooling in `.github/workflows/` or `xtask/`. |
| A3 | Stable-core crates have **no third-party types in public signatures** unless that dependency is itself ≥1.0 and accepted, and no glob re-exports of other crates. | `cargo public-api` review. | ❌ | `crush-cast/src/lib.rs:9` glob-re-exports `caison` 0.1, and `crush-caison/src/lib.rs:18` does `pub use caison::*`. `crush-lang-js` exposes boa/swc types and `crush-lang-rust` exposes `syn`/`proc_macro2`. Many crates return `anyhow::Result`. |
| A4 | The **layering in the operating rules holds**: `crush-cast → casm → crush-errors`, and nothing in the IR layer depends on the VM. | `cargo tree -p crush-cast`. | ❌ | `crush-cast` → `crush-caison` (`crush-cast/Cargo.toml:27`) → `crush-vm` (`crush-caison/Cargo.toml:15`). The IR crate pulls in the whole VM. |
| A5 | Public enums and structs that may grow are `#[non_exhaustive]` (errors, opcode/instruction enums, AST nodes, config structs). | grep plus review. | ❌ | Only 2 uses workspace-wide (`crush-vm/src/lang_exec.rs:47,83`). |
| A6 | Stable-core crates build docs warning-free (`#![warn(missing_docs)]`) and docs.rs renders them. | CI `cargo doc -D warnings`. | ❌ | No `missing_docs` lints, no `[package.metadata.docs.rs]` and no `[workspace.lints]` anywhere. |
| A7 | Feature flags on stable crates are additive and documented, and `--no-default-features` and `--all-features` both build. | CI. | ✅ | The `feature-gates` job runs both (`ci.yml:551-571`). It does not yet document the flags. |
| A8 | An **MSRV policy** exists: which MSRV, how it moves (for example "N-2 stable, a bump is a minor release"), checked in CI. | Doc plus CI. | 🟡 | `rust-version = "1.95.0"` (`Cargo.toml:64`) is checked by the `MSRV (1.95.0)` job (`ci.yml:40-58`, CRUSH-231). The *policy* for moving it is not written down. |
| A9 | Every published stable crate is on crates.io and has no path dependency outside the repo. | `cargo publish --dry-run`. | 🟡 | 24 crates are on crates.io at 0.6.0. `crush-vm` path-depends on `../../../buckets` (which publishes as `crush-buckets` 0.1.0). The comment saying `caison` is "NOT YET on crates.io" (`Cargo.toml:71-73`) is stale: caison 0.1.0 is live. |

### 2.4 Runtime and security (the "capability-sandboxed" north star)

| ID | Criterion | Verify by | Status | Evidence |
|---|---|---|---|---|
| S1 | A **security model doc** (`SECURITY.md` + reference chapter) states what is guaranteed, on which engines, and what is not. | Doc exists. | ❌ | No `SECURITY.md`. The model is described in code comments (`crush-vm/src/vm.rs:1-6`) and in the guide's `capabilities.md`. |
| S2 | CVM1 enforces **declared ∩ granted** capabilities and has hard quotas (steps, stack, output, call depth). | Tests. | ✅ | `vm.rs:1-6`, `Quotas` (`vm.rs:844-890`). |
| S3 | **Declared capabilities survive compilation.** A `.cvm1` built by `crushc`/`crush build` carries `@capabilities`, and an undeclared cap fails at compile or load. | Test. | ❌ | **#258**: crushc and `crush build` ignore `@capabilities`; the `.cvm1` loses the declaration. |
| S4 | **Polyglot is a capability**: `@lang` without a `polyglot.<lang>` grant is refused. | Test. | ✅ | `crush-lang-sdk/tests/polyglot_capability_test.rs` (7 tests, CRUSH-104 gate). |
| S5 | A *granted* polyglot block is **sandboxed by default**, or the docs say plainly that granting `polyglot.*` means "run arbitrary code as the host user". | Default features plus a live bwrap test in CI. | ❌ | CRUSH-236: `sandboxed-polyglot` is not a default feature (`crush-vm/Cargo.toml:75,85`). CI only `cargo check`s it, and `@lang[pypi:…]` deps are dropped. |
| S6 | Ungranted AI-native ops **fail closed and loudly** on every engine. | Test. | ❌ | **#261** / CRUSH-234: an ungranted `AI_QUERY` silently yields `null` when the host has no `ai_native` gate. |
| S7 | **Wall-clock bounds** cover every blocking capability. | Test. | 🟡 | `EXEC_LANG` is killable. `CAP_CALL` is bounded only if the `HostCap` overrides `call_with_deadline` (`vm.rs:860-870`, CRUSH-19). CRUSH-40 is open. |
| S8 | **Engine parity of the sandbox.** FastVM, JIT and AOT enforce the same capability checks as CVM1, *or* the docs say AOT native code runs with host authority. | Differential test per engine. | 🟡 | `crush-aot/tests/js_runtime_caps.rs` exists, and CRUSH-218 is open (FastVM cap errors). The native-AOT trust boundary is not stated anywhere. |
| S9 | Fuzzing of the parser, CVM1 loader and JIT runs in CI (or nightly), with no open crashers. | CI job. | ❌ | No fuzz job. The JIT fuzz target disagrees with the interpreter on about 55/86 FastOps (CRUSH-59, M10). |
| S10 | A deterministic mode and fuel budgets exist, if they are part of the 1.0 story. | — | ❌ | CRUSH-41 and CRUSH-42 are Backlog (M7). *Captain call: in scope or not?* |

### 2.5 Tooling

| ID | Criterion | Verify by | Status | Evidence |
|---|---|---|---|---|
| T1 | **One documented `crush` CLI** whose surface is the 1.0 promise. The many auxiliary binaries are either hidden or marked unstable. | `crush --help` matches the reference. | ❌ | 15+ binaries ship: `crush-run`, `crush-compile`, `crush-repl` (sdk), `crushc`, `walker`, `crush-vm`, `crush-aotc`, `crush-walk-run`, `crush-aot-runner`, `jit-runner`, `crush-debugger`, `crush-installer`, `*_walker` × 7, `export-ts`/`export-py`. #260 and #258 show that `crushc` and `crush-run` disagree. |
| T2 | CLI errors carry a source location, and output is not lost on a runtime error. | Tests. | ❌ | CRUSH-190 and CRUSH-184 are open. |
| T3 | `crush-pkg`: manifest schema frozen and versioned (see F8), with reproducible capsules. | Tests. | 🟡 | `crush-pkg` 0.6.0 is published and has `merkle.rs`/`signer.rs`. The manifest is unversioned. |
| T4 | **LSP / editor support** exists for the 1.0 grammar. | crush-lsp builds against crush-ast 1.0. | ❌ | `crush-lsp` is 0.1.0 in a separate repo, last touched 2026-08-02, and depends on the drifted tree-sitter grammar (L2). *Captain call: is the LSP a 1.0 deliverable or a fast-follow?* |
| T5 | The installer installs a pinned toolchain on Tier-1 platforms. | Smoke test in CI. | 🟡 | `crush-installer` is published. There is no install smoke test. Release binaries are being added in PR #280. |
| T6 | Debugger: either M3 is done, or the debugger is excluded from 1.0. | — | 🟡 | M3 is about 50% done (`MILESTONES.caison`). Recommend excluding it. |

### 2.6 Documentation

| ID | Criterion | Verify by | Status | Evidence |
|---|---|---|---|---|
| D1 | The language guide is **pinned to the release** and every code block is checked in CI. | The guide's CI. | ✅ | `crush-language-guide/CRUSH_AST_COMMIT` pins `3ae0b4a`. `scripts/verify-examples` (121 files) and `scripts/check-examples.py` (every fenced block) run in the `verify` job. |
| D2 | A failed guide verification **blocks deployment**. | Workflow. | ❌ | In `gh-pages.yml`, `deploy` has no `needs: verify`. |
| D3 | The guide marks every experimental feature (matching §1.2). | Review. | 🟡 | The guide uses "*not yet implemented*" and runfail markers (polyglot 4, structs 3, capabilities 3, concurrency 2, …). It has no stable/experimental labels. |
| D4 | crushlang.org is accurate for the release and its samples run. | WEB-1 closed. | ❌ | The last content change was 2026-07-14. WEB-1 (content accuracy audit) is Backlog. No toolchain is pinned. |
| D5 | Migration guide 0.x → 1.0, plus a written deprecation policy (§4.4). | Doc. | ❌ | — |
| D6 | Internal status docs are current. | Review. | ❌ | `docs/design/readiness-matrix.md` is dated 2026-07-14. Several ticket statuses are stale: CRUSH-104 is "In review" although its gate has shipped, and zorro-40/41 are "Backlog" although `crush-cosm` exists. |

### 2.7 Quality gates (CI)

| ID | Criterion | Verify by | Status | Evidence |
|---|---|---|---|---|
| Q1 | CI is green on `main` and required on PRs. | `gh run list`. | ✅ | The last 4 `main` runs pass. One failure on 2026-10-16 (#259). |
| Q2 | MSRV is checked. | CI. | ✅ | `ci.yml:40-58`. |
| Q3 | Cross-backend agreement and example parity run on every PR. | CI. | ✅ | `backend_agreement` + `examples_parity` (`ci.yml:256-258`). AOT tests are sharded. |
| Q4 | AOT C runtime under GC stress. | CI. | ✅ | `ci.yml:262`. |
| Q5 | The **conformance corpus** runs in CI and passes 100% of its non-skipped cases. | CI. | ❌ | CRUSH-211: `conformance` gives 18 pass / 19 fail / 12 skip and is not in CI. |
| Q6 | Clippy runs with `-D warnings` (or a curated deny list) on the stable core. | CI. | 🟡 | The job blocks, but only on `-D clippy::never_loop` (`ci.yml:486-490`). |
| Q7 | **Multi-OS** (Linux, macOS, Windows if promised) and multi-arch (x86_64, aarch64). | CI matrix. | ❌ | All jobs are `ubuntu-latest`. CRUSH-49 and CRUSH-22 are open. |
| Q8 | `cargo-deny` / `cargo-audit` (licences, advisories). | CI. | ❌ | None. |
| Q9 | wasm32 builds for the browser-facing crates. | CI. | ✅ | `ci.yml:377-378` (walkers), `crush-web` headless Chromium (`ci.yml:579-613`). |
| Q10 | The guide's examples run against the release candidate's commit. | Guide CI pinned to the rc tag. | 🟡 | The mechanism exists (D1). It is not yet tied to tags. |

### 2.8 Release engineering

| ID | Criterion | Verify by | Status | Evidence |
|---|---|---|---|---|
| R1 | **No accidental 1.0.** While major = 0, a `feat!:` / `BREAKING CHANGE` commit bumps *minor*. 1.0.0 is minted only on purpose. | Read `bump-version.sh`. | ❌ | `scripts/bump-version.sh:7,72-73`: breaking → **major**. One `feat!:` on main followed by a release run would mint **1.0.0**. Releases are `workflow_dispatch`-only (`release.yml:16-19`), which lowers the risk but does not remove it. *Smallest, most urgent fix in this doc.* |
| R2 | Versioning can **differ per crate** (needed for a partial 1.0). | Tooling. | ❌ | The workspace is lock-step (`version.workspace`, `Cargo.toml:59`), and `bump-version.sh` and `publish-crates.py` assume it. |
| R3 | A pre-release (`1.0.0-rc.N`) can be cut and published. | Dry run. | 🟡 | `publish.yml` triggers on `v*` tags (`publish.yml:30`) and has trusted publishing. Pre-release suffixes are untested in `bump-version.sh`. |
| R4 | Every crate in the release is publishable by CI. No manual first publish. | `publish-crates.py` plan. | 🟡 | 24 crates are published. About 20 more would each need a manual first `cargo publish`. |
| R5 | The CHANGELOG has a **"Breaking"** section per release, separate from the mechanical entries. | Review. | 🟡 | `CHANGELOG.md` follows Keep a Changelog "loosely". Entries are auto-appended. |
| R6 | Release binaries for Tier-1 platforms are attached to the GitHub release. | Release page. | 🟡 | PR #280 (artifact upload) is open. |
| R7 | The open-P1 count is triaged: each P1 is fixed, or scoped out of 1.0 with a note. | Board. | ❌ | **72 open P1 ticket files** (of 191 open), plus 15 open GitHub issues. |

### 2.9 Scoreboard

| Group | ✅ | 🟡 | ❌ | Total |
|---|---|---|---|---|
| 2.1 Language | 0 | 5 | 5 | 10 |
| 2.2 Formats / compat | 2 | 3 | 4 | 9 |
| 2.3 Public Rust API | 1 | 2 | 6 | 9 |
| 2.4 Runtime / security | 2 | 2 | 6 | 10 |
| 2.5 Tooling | 0 | 3 | 3 | 6 |
| 2.6 Docs | 1 | 1 | 4 | 6 |
| 2.7 Quality gates | 5 | 2 | 3 | 10 |
| 2.8 Release engineering | 0 | 4 | 3 | 7 |
| **Total** | **11** | **22** | **34** | **67** |
---

## 3. Scope decision: full 1.0 vs partial 1.0

### Option A — full 1.0 (every published crate goes to 1.0.0)

- **Pros:** one number to talk about, and nothing to explain.
- **Cons:** it freezes `crush-vm` (≈600 public items across 56 `pub mod`s, much of it
  internals like `rust_methods_core` and `js_ops`), `crush-lang-sdk` (≈300 items,
  11 features, re-exports VM types at `crush-lang-sdk/src/lib.rs:67-75`), every walker
  (which expose `syn`/`swc`/`boa` types), JIT and AOT, all mid-milestone (M2, M3, M5, M6, M10).
  Either we stop changing them, or we cut 2.0 within months. Both undercut what a 1.0
  is supposed to mean.
- **Earliest realistic date:** after M2, M5, M6, M9 and COSM K3. Many months.

### Option B — partial 1.0, by surface (recommended)

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

| Release | Theme | Exit criteria (from §2) |
|---|---|---|
| **0.7.0** | vega, in flight | Unchanged by this doc. |
| **0.8.0 — freeze prep** | Tooling and decisions | R1 (no accidental major), A2 semver-checks wired (report-only), F5 compat corpus seeded with 0.6/0.7/0.8 artifacts, F9 format-versioning doc, S1 `SECURITY.md` draft, L1 reference skeleton with stability column, Q5 conformance in CI (allowed-failures list), the scope decision (§3) and the COSM decision (L6). |
| **0.9.0 — beta / feature freeze** | Close the stable surface | L2, L4 (zero silent-wrong on Tier-1), L5, L7, F3, F4, A3, A4, S3, S5 (or the S5 docs), S6, T1, T2, D3, R2, R7 triaged. **No new syntax after 0.9.0**, except behind an experimental gate. |
| **1.0.0-rc.1 … rc.N** | Soak | All ❌ in the stable scope closed or explicitly scoped out. Then only bug fixes. **Any change to a promised surface restarts the clock with a new rc.** Minimum 4 weeks from the last rc to 1.0.0. The guide (D1/D2) and website (D4) are pinned to the rc tag. A downstream canary (openko/fabric, arniko, surfer) builds against each rc. |
| **1.0.0** | Promise starts | Tag. Changelog "Breaking since 0.x" section. Migration guide (D5). |

### 4.2 Milestones and tickets: close, or scope out of 1.0

| Item | Proposed 1.0 treatment |
|---|---|
| M2 JIT completion / M10 (CRUSH-59/60/61) | **Out.** The JIT is an experimental tier and is off by default for stable programs until differential parity is proven. #273 must still be fixed or the JIT disabled, because silently-wrong output on an opt-in tier is still silently wrong. |
| M3 debugger | **Out.** |
| M4 / M9 cross-project convergence | **Out**, except the stdlib capability set that 1.0 marks stable (P7). |
| M5 AI-native layer (CRUSH-30) | Annotations that are already CAST nodes: **experimental**. S6 (fail-closed) is **in**. |
| M6 walker parity (CRUSH-21/35-39), CRUSH-251 | **Out** (walkers are best-effort, §1.2). |
| M7 runtime hardening (CRUSH-40/41/42) | CRUSH-40 (timeouts) **in** if S7 is promised. 41/42 are a captain call (S10). |
| M8 platforms (CRUSH-22/49) | **Linux x86_64 Tier-1 at 1.0.** Others are Tier-2 with no promise. |
| COSM K1.2/K1.5/K2 (zorro-40/41/44, K2) | Decide at 0.8 (L6). Either finish K2 before the 0.9 freeze, or ship `@kind` as experimental. |
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

### 4.4 Deprecation and migration policy (proposal)

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
| **Semantics** | Changing the observable result of a stable-core program on a Tier-1 engine (outputs, errors raised, capability calls made). | New stdlib functions. New optional parameters. | Fixing a result that contradicted the reference. **Policy call:** fixing a silently-wrong result is a *patch* even if someone depended on the bug; it gets a changelog note. |
| **Bytecode / formats** | A runtime can no longer load an artifact written by an earlier 1.x compiler. A format major bump. | New opcodes or sections that old runtimes refuse **by name** (the CVM1 v3 pattern, `bytecode.rs:14-22`). A format minor bump. | — |
| **Capabilities** | Removing a stable capability. Changing its args or results. **Widening** what a grant allows. | New capabilities. New optional fields. | Tightening a check to match the documented model. |
| **Stdlib** | Removal, rename, or a signature/result-shape change. | Additions. | Fixes. |
| **CLI** | Removing or renaming a documented subcommand or flag. Changing exit codes or the `--message-format=json` schema. | New subcommands, flags, or JSON fields. | Message-text changes. |
| **Rust API (stable crates)** | Anything `cargo-semver-checks` flags as major. | Additive changes. | Fixes. |
| **MSRV** | — | A bump, per policy (A8). | — |

---

## 5. Open questions for the captain

1. **Which shape: A, B or C?** (Recommended: B, starting as C.) Who are the external
   embedders we owe Rust stability to, if any?
2. **COSM before or after 1.0?** If structs and `@kind` are going to get COSM semantics,
   1.0 should wait for K2, or ship `struct` as stable with today's semantics and `@kind`
   as experimental. Changing struct semantics after 1.0 would be a 2.0.
3. **Is polyglot part of the 1.0 promise?** It is the headline feature, but #255/#266/#278/#279
   and CRUSH-236 (granted means full host authority) suggest "experimental" for 1.0.
4. **Which engines are Tier-1** for the "same observable result" promise? Proposal:
   CVM1 scheduler + PortableVM + FastVM. AOT C/Rust are Tier-2. JIT is experimental.
5. **Fixing silently-wrong bugs after 1.0: patch or major?** (Proposed: patch, with a
   changelog note.)
6. **Platforms:** is Linux x86_64-only acceptable for 1.0? Or is macOS/aarch64 a must?
7. **Is the LSP / editor story a 1.0 deliverable** (T4), or a fast-follow?
8. **Does 1.0 include M7's determinism and fuel** (S10)? That matters if exosphere or
   openko want reproducible capsule execution as part of the contract.
9. **Naming:** do we keep the 15+ binaries, or consolidate into one `crush` CLI with
   subcommands before freezing (T1)?
10. **Timeline appetite:** the train in §4.1 is about 3 minor releases plus an rc soak.
    Is that the right pace, or should 0.8 and 0.9 merge?

---

*Sources: `.jagent/planning/MILESTONES.caison`, `ROADMAP.md`, ticket files under
`.jagent/planning/tickets/`, `gh issue list -R nixpt/crush-ast` (15 open, 2026-10-19),
`.github/workflows/*.yml`, code as cited, `crush-language-guide` @ `dfe88f3`,
`crush-website` @ `b1ff932`, crates.io API (2026-10-19).*
