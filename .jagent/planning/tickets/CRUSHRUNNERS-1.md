# CRUSHRUNNERS-1: crush-pkg runner subsystem gaps — ContainerRunner stub, ScriptRuntime 4-cap, `Unknown → CrushRunner` silent fallback

**Status**: open (registration only — no implementation commits on this branch)
**Branch**: `agent/buffy/CRUSHRUNNERS-1` — created off `2f2b2f5` (= `agent/buffy/network`'s tip; the surface area referenced in this ticket lives here, not on plain `main` where `TICKETS/` doesn't exist yet).
**Priority**: 🟡 P1 — runner-subsystem gaps aren't correctness blockers (each gap fails loudly or silently in narrow paths) but they are clear surface debt that future contributors will trip on.
**Goal**: catalogue the 3 actual runner-subsystem gaps so a future contributor (e.g., implementing `Container` capsules, adding a 5th language runtime, or wiring `--strict` into the `run` path) has the locus + migration path laid out instead of rediscovering it. **Cross-link** to CRUSHPKG-1 (commit `2f2b2f5`, PR #6) which registered the byte-exact NDJSON contract + the `scan_entry_file_references` URL-fragment fix in `builder.rs`.

## Identity

- Crate: `crush-ast/crates/crush-pkg`.
- Three gaps in the runner subsystem (one per gap, intentionally distinct):
  1. **ContainerRunner stub-bail** — `src/runners.rs:122-128` (impl body, `bail!`).
  2. **ScriptRuntime 4-variant cap** — `src/manifest.rs` (`pub enum ScriptRuntime { #[default] Bun, Node, Deno, Python }`).
  3. **`PayloadFormat::Unknown → Box::new(CrushRunner)` silent fallback** — `src/runners.rs:175` (`match format { ... }` tail arm in `get_runner_for_payload`).
- All three are *not* bugs (each has a working code path today); they are surface debt where the current shape constrains future contributors.

## Root cause per gap

### Gap 1 — `ContainerRunner` stub-bail

- **Site**: `src/runners.rs:122-128`
- **State**: implementation refuse-up-front. `CapsuleType::Container` is wired into both `get_runner` (`CapsuleType::Container => Box::new(ContainerRunner)`) and `get_runner_for_payload` (`PayloadFormat::Container => Box::new(ContainerRunner)`), but the underlying `CapsuleRunner::run` bails:
  ```rust
  impl CapsuleRunner for ContainerRunner {
      fn run(&self, manifest: &Manifest, _payload_path: &Path, _args: &[String]) -> anyhow::Result<ExecutionResult> {
          anyhow::bail!(
              "Container capsules are not yet supported in crush-pkg (capsule: {})",
              manifest.capsule.name
          )
      }
  }
  ```
- **Why it's a gap**: any `crush-pkg run` on a `Container` capsule surfaces the early-bail rather than picking a model. The user sees "not yet supported" but has no actionable path forward.
- **Open question** (design, not code): "Container" can mean (a) `podman run` / `docker run` invocation, (b) WASM-as-container via `wasmtime`, (c) CrushVM-internal sandbox (capabilities + quotas, `crush_vm::Quotas`). No decision is on disk yet.
- **Minimal fix** *(out of scope here — its own ticket)*: pick one of the three models AND extend `CapsuleType::Container` to carry a sub-variant (e.g., `Container(PodmanBackend)` / `Container(WasmBackend)`); OR delete `CapsuleType::Container` entirely until a model is chosen.

### Gap 2 — `ScriptRuntime` enum capped at 4

- **Site**: `src/manifest.rs` — the `enum ScriptRuntime { #[default] Bun, Node, Deno, Python }` 4-variant shape, with `get_runtime_command` in `src/runners.rs:67-74` mapping each variant to its binary + args (`bun run`, `node` no-args, `deno run --allow-read --allow-write`, `python3`).
- **Why it's a gap**: the cap is *intentional* but undocumented. Adding a 5th runtime (Ruby, Go script, Julia, Perl, R, etc.) is mechanical — three changes (enum variant + `get_runtime_command` arm + read site in `manifest::scaffold_package`) — but a contributor has to rediscover this constraint from the test surface.
- **Minimal fix** *(out of scope here — on-demand)*: when a real language need is established, add the variant + binary mapping. Until then, the cap is acceptable. Suggested hardening: an inline comment at the `enum ScriptRuntime` declaration documenting the "intent-aware cap" so future contributors know to add on demand rather than substituting another runner.

### Gap 3 — `PayloadFormat::Unknown → Box::new(CrushRunner)` silent fallback

- **Site**: `src/runners.rs:175` — the terminal arm of `get_runner_for_payload`'s `match format { ... }`:
  ```rust
  PayloadFormat::Container => Box::new(ContainerRunner),
  PayloadFormat::Unknown => Box::new(CrushRunner),
  ```
- **State**: silent fallback. A payload whose extension AND magic-byte probe both fail to classify (e.g. a novel `.foo` blob) is dispatched to `CrushRunner` → `crush_lang_sdk::compile::compile_crush_source` → `crush_vm::run_with_caps` — which fails at the parser with a syntax error, not "unknown format". The user sees a confusing parser error.
- **Why it's a gap**: masks missing language families. A future walker (`crush-lang-zig`, `crush-lang-rust-script`, etc.) that hasn't been wired up yet would silently somersault through Crush interpretation, surfacing a misleading syntax error to the user instead of an actionable "unknown format" warning.
- **Minimal fix** *(out of scope here — its own ticket)*: in `--strict` mode (the wire is already in place via `MessageFormat::Strict` for the LINT surface — see CRUSHPKG-1's `crush-pkg lint` subcommand behavior), emit a `CommandFailure::Run` bail with the unknown payload's path + extension + magic-byte snippet. Non-strict (default) mode keeps the silent fallback for back-compat. **Note**: `MessageFormat::Strict` is wired on the lint path *today*; the run path needs the same `global = true` flag + a dispatch hook. That wiring is its own ticket — see `src/main.rs:91-110` for the existing JSON/strict dispatch on the build/lint surface, to be mirrored on the run surface.

## Cross-link to CRUSHPKG-1 URL-fragment specifics

This ticket does NOT modify `builder.rs`. The byte-exact NDJSON contract (commit `2f2b2f5`, registered in PR #6 via CRUSHPKG-1) lands in `crates/crush-pkg/src/builder.rs`:

- `scan_entry_file_references` (function at `builder.rs:299`, body lines ~285-325) — whitespace-or-BOL `#` gate prevents string-literal URL fragments (`"docs.md#install"`) from being silently truncated as line comments.
- Doc-rationale block at `builder.rs:998-1007` documents the 4 lockdown tests: (a) URL fragments survive, (b) true line-comment markers strip, (c) `key#suffix` identifiers split rather than comment, (d) `#`-at-file-start triggers comment mode.

CRUSHRUNNERS-1 is the runner-subsystem companion — it surfaces the *gap catalogue* without touching the byte-exact contract. Both tickets are sequenced for the same wave: CRUSHPKG-1 locks the surface, CRUSHRUNNERS-1 names what to fill next.

## Done condition

This ticket is finished when ALL of:

1. A subsequent commit picks one Container model (Gap 1) — OR deletes `CapsuleType::Container` from `manifest.rs` until a model is chosen. (Either path closes the surface.)
2. Each missing `ScriptRuntime` needed by a real language gets added (Gap 2) — OR an inline comment at the enum declaration documents the "intent-aware cap" so future contributors know to add on demand rather than substitute.
3. Gap 3 fallback emits a `CommandFailure::Run` bail under `--strict` (mirroring the lint path's existing `CommandFailure::Lint`); non-strict behavior preserved for back-compat. Wiring `MessageFormat::Strict` into the run dispatch in `src/main.rs::fn dispatch(Commands::Run { .. })` is its own scoped change.
4. Each fix lands as its own PR with a corresponding `TASKS.md` Done-log entry mirroring the CRUSHPKG-1 entry format.

## Out of scope

- Implementing a Container runtime (Docker / WASM / CrushVM-internal). Design + impl are separate tickets once a model is chosen.
- Adding any 5th `ScriptRuntime` language. Pending language request.
- Refactoring `PayloadFormat::from_path` to be more lenient. The current strict-by-extension-then-magic-byte-sniff is intentional; only the silent Crush fallback to a user-visible error is intended here.
- Touching `builder.rs` byte-exact NDJSON contract. That's CRUSHPKG-1's domain.

## Risk / migration

- **Gap 3**: silent → strict-mode bail. Users who relied on Crush-as-default for unknown-format payloads (likely a developer's `.scratch.md`-style ad-hoc scenario) will see a different error message under `--strict`. The default (non-strict) path stays silent. Document in CRUSH-AST-1 release notes.
- **Gap 1**: already `bail!` so removing it (vs adding an impl) is back-compat-safe.
- **Gap 2**: zero blast radius if treated as "intentional cap" rather than a bug.

## Suggested sequencing

1. **Gap 3 (strict-mode-bail on Unknown format)** — smallest, most user-visible, mirrors the existing `MessageFormat::Strict` lint-surface pattern.
2. **Gap 2 (ScriptRuntime additions)** — on demand as real language needs emerge. Add a comment in the meantime.
3. **Gap 1 (Container model)** — push back `CapsuleType::Container` or pick + impl; largest design decision, requires an upstream model choice.

## References

- Source files (all on commit `2f2b2f5` of `agent/buffy/network`):
  - `crates/crush-pkg/src/runners.rs:122-128` (ContainerRunner stub-bail)
  - `crates/crush-pkg/src/runners.rs:175` (Unknown→CrushRunner fallback)
  - `crates/crush-pkg/src/runners.rs:67-74` (`get_runtime_command` for ScriptRuntime)
  - `crates/crush-pkg/src/manifest.rs` (ScriptRuntime enum + CapsuleType::Container wiring)
  - `crates/crush-pkg/src/builder.rs:299` + `:998-1007` (URL-fragment fix cross-link)
- Companion ticket **CRUSHPKG-1** (PR #6 on this repo, branch `agent/buffy/CRUSHPKG-1` off `2f2b2f5`): byte-exact NDJSON registration in `STATE.md` + `TASKS.md`.
- Format reference written earlier on this branch: `TICKETS/CRUSHSDK-1.md` (crush-lang-sdk `--all-features` deref-fix ticket sketch, same template shape).
- Format reference existing on a sister branch: `TICKETS/CRUSHVM-2-EXEC-LANG-POP-NAMED.md` (crush-vm EXEC_LANG pop-on-name followup).
