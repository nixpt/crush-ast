//! `crush-pkg/tests/test_selfhost_sdk_matrix.rs` — structural lockstep test
//! for the authoring-surface matrix documented in
//! `TICKETS/CRUSH-SELFHOST-1.md`.
//!
//! Locks the **7 reachable** surface cells (the `good` cells of the matrix)
//! to the actual SDK surface. If any future change to `crush-vm::HostCap`,
//! `crush-lang-sdk::caps::*`, `crush-lang-sdk::Runtime`,
//! `crush-lang-sdk::compile`, or `crush-pkg::runners::CrushRunner` breaks a
//! cell, the relevant assertion fails synchronously and the assertion message
//! names the cell identifier — pointing at the matrix row that drifted.
//!
//! Per `TICKETS/CRUSH-SELFHOST-1.md`, the 7 reachable cells are:
//!
//!   ── Capsule axis (3 reachable cells)
//!     1. The program body: `fn main()` calling any host-table cap
//!     2. The runtime chooses `CapsuleType::Crush` via `language = "crush"`.
//!        Legacy `capsule_type = "Crush"` is auto-migrated by
//!        `Manifest::from_str` to `language = "crush"` (HALF 1,
//!        CRUSH-SELFHOST-1-AMEND-1 RESOLVED), AND the dispatcher in
//!        `language_to_capsule_type` is case-insensitive (HALF 2,
//!        CRUSH-SELFHOST-1-AMEND-1 RESOLVED) so mixed-case fallback still
//!        reaches `CapsuleType::Crush` rather than the `_ =>` `Auto` arm.
//!     3. Host-table pushes via a custom `HostCap` registered into
//!        `crush-pkg::runners::CrushRunner.host_caps: Option<HostCaps>`.
//!
//!   ── REPL+runtime axis (4 reachable cells)
//!     4. Built-in caps (e.g. `io.print`) dispatch through the VM without
//!        any host registration.
//!     5. User-extended host caps: `Box<dyn HostCap>` registered via
//!        `crush_vm::HostCaps::register`, name-addressed, with `spec()`
//!        declaring `(name, argc, returns)`.
//!     6. Runtime API: `crush-lang-sdk::Runtime` (re-exported via
//!        `crush-lang-sdk::lib.rs:53`) loads + executes a `Program`.
//!     7. Compile API: `crush-lang-sdk::compile::compile_crush_source`
//!        (re-exported via `crush-lang-sdk::lib.rs:11`).
//!
//! ## Cap-name shape (locked separately)
//!
//! Each cap-side cell is reached via a **2-segment** dotted cap name —
//! `capsule.cell_1`, `capsule.cell_3`, `repl_runtime.cell_5` — matching
//! the syntax shape that already works in `tests/test_selfhost_demo.rs`
//! (`gui.register_command`). The round-1 fixture used 3-segment names
//! (`matrix.capsule.cell_1_program_body(...)`) which the type-checker
//! rejected as "Cannot call non-function" — the deeper-nesting path was
//! resolved through member access rather than cap dispatch. The 2-segment
//! form is empirically accepted by `capsule.cell_1`, `capsule.cell_3`,
//! `repl_runtime.cell_5`, and the existing `gui.register_command`. If a
//! future parser/type-checker change accepts deeper nesting, re-run this
//! test and consider bumping to 3-segment names.
//!
//! Each assertion embeds its CELL_N slug in the failure message so a
//! reviewer sees which matrix cell drifted. The capturer prints the full
//! log on `--nocapture` for at-a-glance inspection.

use std::sync::{Arc, Mutex};

use crush_lang_sdk::compile::compile_crush_source;
use crush_lang_sdk::{HostCap, HostCapSpec, HostCaps, Runtime};
use crush_pkg::manifest::{language_to_capsule_type, CapsuleType, Manifest};
use crush_pkg::runners::{CapsuleRunner, CrushRunner, ExecutionResult};
use crush_vm::{run_with_caps, Quotas};
use crush_vm::vm::Value;

mod test_paths;

// ─────────────────────────────────────────────────────────────────────────────
// Per-cell capture map
// ─────────────────────────────────────────────────────────────────────────────
//
// One shared log keyed by cell slug. Any HostCap impl that wants to record
// to a specific cell copies the CaptureMap; each `call()` pushes a row
// tagged with the cell's static slug. A future regression surfaces as
// "this assertion failed → look at which cell ID is named".

type ArgsRow = Vec<String>;

#[derive(Clone, Default)]
struct CaptureMap {
    inner: Arc<Mutex<Vec<(&'static str, ArgsRow)>>>,
}

impl CaptureMap {
    fn record(&self, cell: &'static str, args: ArgsRow) {
        self.inner.lock().unwrap().push((cell, args));
    }

    fn log(&self) -> Vec<(&'static str, ArgsRow)> {
        self.inner.lock().unwrap().clone()
    }
}

fn arg_of(v: &Value) -> String {
    match v {
        Value::Null => "null".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => f.to_string(),
        Value::Str(s) => s.clone(),
        Value::Error(e) => format!("error({e})"),
        // Array / Map / Bytes / Handle are not what these matrix-cell
        // caps should ever see from a well-formed `.crush` caller;
        // record a tag so a regression is loud instead of silent.
        _ => "<unprintable>".to_string(),
    }
}

// One HostCap per cap-side cell. The cap NAME encodes the cell ID and
// matches the .crush call syntax (2-segment dotted); the `cell` slug
// is what's logged into the shared CaptureMap.
fn make_cap(
    name: &'static str,
    cell: &'static str,
    argc: usize,
    map: CaptureMap,
) -> impl HostCap {
    struct Cap {
        name: &'static str,
        cell: &'static str,
        argc: usize,
        map: CaptureMap,
    }
    impl HostCap for Cap {
        fn spec(&self) -> HostCapSpec {
            HostCapSpec {
                name: self.name.to_string(),
                argc: Some(self.argc),
                // `returns: true` is required because the crush-frontend
                // compiler treats every unknown call as an expression that
                // yields one value, then appends a `POP` opcode to discard
                // the result when the call sits in statement position. If
                // we returned `Ok(None)`, the `POP` would underflow the
                // VM stack. Pushing `Value::Null` lets the statement-cleanup
                // `POP` find its target. (Same contract as
                // `tests::test_selfhost_demo.rs::Recorder` — the lockstep
                // runs through the same SDK surface.)
                returns: true,
            }
        }
        fn call(&self, args: Vec<Value>) -> Result<Option<Value>, String> {
            let argv: Vec<String> = args.iter().map(arg_of).collect();
            self.map.record(self.cell, argv);
            Ok(Some(Value::Null))
        }
    }
    Cap {
        name,
        cell,
        argc,
        map,
    }
}

// Cap-side cell slugs (2-segment, match the .crush call syntax).
const CAP_CELL_1: &str = "capsule.cell_1";
const CAP_CELL_3: &str = "capsule.cell_3";
const CAP_CELL_5: &str = "repl_runtime.cell_5";

// ─────────────────────────────────────────────────────────────────────────────
// Main fixture: one `main.crush` that drives all cap-side cells (1, 3, 5)
// and the built-in cap (4) in a single program. API-side cells (2, 6, 7)
// are verified at the Rust boundary separately so a drift there
// surfaces independently of the source program.
// ─────────────────────────────────────────────────────────────────────────────

// ---------------------------------------------------------------------------
// Fixture-on-disk under `tests/fixtures/sdk-matrix-locator/` — the per-fixture
// HEADER comments map each file to the cell(s) it covers. `ls + cat` reads
// ALL 7 matrix cells inline at the fixture layer:
//
//   CELL 1, 3, 5 (cap-side) + CELL 4-in-main   main.crush
//   CELL 4 (dedicated isolation)              builtin_only.crush
//   CELL 6 (dedicated isolation)              runtime_api.crush
//   CELL 2 (language dispatch)                capsule.toml
//   CELL 7 (compile API)                      compile_crush_source(<each src>)
//                                             firing on every fixture above
//
// Path convention: runtime = `test_paths::fixture_root("sdk-matrix-locator")`
// (single anchor — see `tests/test_paths.rs`). Compile-time = `include_str!`
// source-file-relative literals (the `..` prefix would escape `tests/`).
// All 3 include_str! literals below share the `sdk-matrix-locator/` prefix
// and MUST change together if the fixture directory is renamed.
// ---------------------------------------------------------------------------
const PROGRAM_SRC: &str = include_str!("fixtures/sdk-matrix-locator/main.crush");
const BUILTIN_ONLY_SRC: &str = include_str!("fixtures/sdk-matrix-locator/builtin_only.crush");
const RUNTIME_SRC: &str = include_str!("fixtures/sdk-matrix-locator/runtime_api.crush");

/// The structural lockstep test — every assertion embeds its cell ID so
/// a regression failure message names the matrix row that drifted.
#[test]
fn sdk_matrix_structural_lockstep() {
    let map = CaptureMap::default();

    // Load fixture on disk via the shared `test_paths::fixture_root`
    // helper (single grep anchor for the path-anchor convention).
    // The runner reads `payload_path`, not in-memory source.
    let fixture_root = test_paths::fixture_root("sdk-matrix-locator");
    let main_path = fixture_root.join("main.crush");
    let manifest = Manifest::from_file(&fixture_root.join("capsule.toml"))
        .expect("CELL_2: fixture capsule.toml must parse under tests/fixtures/sdk-matrix-locator/");

    // ─── CELL 7 ─ compile API surface ────────────────────────────────
    // Citation: `crush-lang-sdk::lib.rs:11` — `pub mod compile;`.
    let program_via_compile = compile_crush_source(PROGRAM_SRC)
        .expect("CELL_7 (compile API): compile_crush_source must succeed");
    assert!(
        !program_via_compile.code.is_empty(),
        "CELL_7 (compile API): Program.code must be non-empty after compile_crush_source"
    );

    // ─── CELL 6 ─ Runtime API surface ────────────────────────────────
    // Citation: `crush-lang-sdk::lib.rs:53` — `pub use runtime::{Runtime, RuntimeError};`.
    // The dedicated fixture (`runtime_api.crush`) is loaded at compile
    // time via `include_str!` (module-level `RUNTIME_SRC` const); below
    // we drive it through `Runtime::new().run(...)` to pin the SDK
    // Runtime path independently of the CrushRunner-driven CELL_7
    // path (which uses `main.crush` + `CrushRunner.run`).
    let runtime_program = compile_crush_source(RUNTIME_SRC)
        .expect("CELL_6 setup: compile_crush_source(runtime_api.crush) must succeed");
    let runtime_result = Runtime::new()
        .run(&runtime_program)
        .expect("CELL_6 (Runtime API): Runtime::run must succeed");
    assert!(
        runtime_result.output.contains("from Runtime API"),
        "CELL_6 (Runtime API): runtime_result.output must contain the print sentinel; \
         got: {:?}",
        runtime_result.output
    );

    // ─── CELL 2 ─ language dispatch surface ──────────────────────────
    // Citation: `crush-pkg::manifest::language_to_capsule_type` — `"crush"` →
    // `CapsuleType::Crush`. `CrushRunner::run` is reached via the same
    // dispatch arm below; the audit-trail assertion here pins the
    // SOURCE field the dispatcher keys on.
    assert_eq!(
        manifest.capsule.language, "crush",
        "CELL_2 (language dispatch): manifest.capsule.language must be \"crush\" \
         (the dispatch signal the runtime keys on); if a future manifest shape \
         moves this, update this cell-2 audit AND the second #[test] below."
    );
    // The dispatch arm is exercised below via CrushRunner directly;
    // no need for a separate `get_runner_for_payload` bind — its only
    // effect here would be a no-op smoke test, and the cell below
    // (CELL_3) drives a real runner.

    // ─── CELL 5 ─ user-extended HostCap registry surface ─────────────
    // Citation: `crush-vm::host.rs:30-50` — `HostCaps::register` /
    // `HostCaps::get`. Pin the registry pattern by registering three
    // custom caps and asserting each gets looked up by name.
    let mut caps = HostCaps::new();
    caps.register(Box::new(make_cap(
        CAP_CELL_1,
        CAP_CELL_1,
        1,
        map.clone(),
    )));
    caps.register(Box::new(make_cap(
        CAP_CELL_3,
        CAP_CELL_3,
        2,
        map.clone(),
    )));
    caps.register(Box::new(make_cap(
        CAP_CELL_5,
        CAP_CELL_5,
        2,
        map.clone(),
    )));
    assert!(
        caps.get(CAP_CELL_1).is_some(),
        "CELL_5 (registry): caps.get({CAP_CELL_1:?}) must resolve"
    );
    assert!(
        caps.get(CAP_CELL_3).is_some(),
        "CELL_5 (registry): caps.get({CAP_CELL_3:?}) must resolve"
    );
    assert!(
        caps.get(CAP_CELL_5).is_some(),
        "CELL_5 (registry): caps.get({CAP_CELL_5:?}) must resolve"
    );

    // ─── CELLS 1 + 3 ─ program body + host-table plumbing ────────────
    // Drive via CrushRunner (the actual production code path).
    // Citation: `crush-pkg::runners.rs::CrushRunner.host_caps: Option<HostCaps>`.
    let runner = CrushRunner {
        host_caps: Some(caps),
    };
    let result = runner
        .run(&manifest, &main_path, &[])
        .expect("CELL_1/CELL_3: CrushRunner::run must drive main.crush to completion");
    assert!(
        matches!(result, ExecutionResult::Vm),
        "CELL_3 (host table): CrushRunner must hand the program to the VM (ExecutionResult::Vm); \
         got: {:?}",
        result
    );

    // ─── CELL 4 ─ built-in cap surface ───────────────────────────────
    // io.print is a built-in (loaded from `crush-vm::caps::capabilities`,
    // not from any `HostCap`). Pin it independently via the dedicated
    // `builtin_only.crush` fixture (loaded at compile time via
    // `include_str!` module-level `BUILTIN_ONLY_SRC` const) so a drift
    // in the built-in registry is attributable to this cell rather
    // than to the host table. The fixture-under-test calls io.print
    // only — no user-extended caps — which is what makes
    // `host_caps: None` the right tool here (a registry gap would
    // surface as `UnknownCap`, but only on the user-extended name,
    // not on `io.print`).
    let builtin_only_program = compile_crush_source(BUILTIN_ONLY_SRC)
        .expect("CELL_4 setup: compile_crush_source(builtin_only.crush) must succeed");
    let quotas = Quotas::default();
    let vm_out = run_with_caps(&builtin_only_program, &quotas, None)
        .expect("CELL_4 (built-in): run_with_caps must drive the built-in-only program");
    assert!(
        vm_out.output.contains("cell_4_dedicated_isolation"),
        "CELL_4 (built-in cap): io.print must reach VM output via run_with_caps; \
         got: {:?}",
        vm_out.output
    );

    // ─── Matrix-wide log + assertions ────────────────────────────────
    let log = map.log();

    // CELL_1: program body side-effect cap call — `fn main()` invoked
    // `capsule.cell_1("capsule-body")` and the argument landed in the
    // recorder.
    assert!(
        log.iter().any(|(c, args)| *c == CAP_CELL_1
            && args.first().map(String::as_str) == Some("capsule-body")),
        "CELL_1 (program body): main() must have invoked \
         `capsule.cell_1(\"capsule-body\")`; \
         full log: {log:?}"
    );

    // CELL_3: HostCap table via CrushRunner.host_caps — `fn main()`
    // reached the cap registered into CrushRunner's host_caps field
    // (proves the `Some(caps)` plumbing survived to the VM dispatch).
    assert!(
        log.iter().any(|(c, args)| *c == CAP_CELL_3
            && args.first().map(String::as_str) == Some("from CrushRunner.host_caps")
            && args.get(1).map(String::as_str) == Some("7")),
        "CELL_3 (host table): main() must have invoked \
         `capsule.cell_3(\"from CrushRunner.host_caps\", 7)`; \
         full log: {log:?}"
    );

    // CELL_5: user-extended HostCap registered via HostCaps::register
    // — `fn main()` reached a cap that lives ONLY in this test's
    // registry (proves a real `.crush` source can call any custom
    // cap registered into HostCaps).
    assert!(
        log.iter().any(|(c, args)| *c == CAP_CELL_5
            && args.first().map(String::as_str) == Some("from HostCaps::register")
            && args.get(1).map(String::as_str) == Some("42")),
        "CELL_5 (user-extended): main() must have invoked \
         `repl_runtime.cell_5(\"from HostCaps::register\", 42)`; \
         full log: {log:?}"
    );

    // Surface the full structural-locator log on stderr so
    // `cargo test -- --nocapture` makes every cell's reachability
    // visible at a glance.
    eprintln!("[sdk-matrix-structural-lockstep] captured cell log:");
    eprintln!(
        "    CELL_1 capsule.cell_1_program_body           → {:?}",
        log.iter().find(|(c, _)| *c == CAP_CELL_1).map(|(_, a)| a)
    );
    eprintln!(
        "    CELL_2 capsule.cell_2_language_dispatch       → manifest.capsule.language = {}",
        manifest.capsule.language
    );
    eprintln!(
        "    CELL_3 capsule.cell_3_host_table              → {:?}",
        log.iter().find(|(c, _)| *c == CAP_CELL_3).map(|(_, a)| a)
    );
    eprintln!(
        "    CELL_4 repl_runtime.cell_4_builtin_cap       → vm_out.output had sentinel = {}",
        vm_out.output.contains("cell_4_dedicated_isolation")
    );
    eprintln!(
        "    CELL_5 repl_runtime.cell_5_user_extended_cap → {:?}",
        log.iter().find(|(c, _)| *c == CAP_CELL_5).map(|(_, a)| a)
    );
    eprintln!(
        "    CELL_6 repl_runtime.cell_6_runtime_api        → runtime_result.output had sentinel = {}",
        runtime_result.output.contains("from Runtime API")
    );
    eprintln!(
        "    CELL_7 repl_runtime.cell_7_compile_api        → program.code.len() = {}",
        program_via_compile.code.len()
    );
}

/// One-off audit-trail test for the legacy `capsule_type = "Crush"` field's
/// auto-migration to `language` AND the dispatch shape that flows from it.
/// Both halves are pinned here so a future regression in either path
/// surfaces as a precise panic:
///
///   HALF 1 (auto-migrate case): `crates/crush-pkg/src/manifest.rs::Manifest::from_str`
///   consumes the legacy `capsule_type` field and NOW lowercases the value
///   on insert, so `capsule_type = "Crush"` migrates to `language = "crush"`.
///   A regression (case-preserving insert re-introduced) fails exactly
///   this assertion; the message names HALF 1 and points the fixer at
///   `Manifest::from_str`'s auto-migrate block.
///
///   HALF 2 (dispatch shape): legacy manifests dispatch through
///   `CapsuleType::Crush` because `crush_pkg::manifest::language_to_capsule_type`
///   is case-insensitive. A regression (the `_ => CapsuleType::Auto`
///   arm is reached again because someone removed the `.to_ascii_lowercase()`
///   call) fails exactly this assertion; the message names HALF 2 and
///   points the fixer at `language_to_capsule_type`.
///
///   These two halves were originally framed as a latent dispatch-shape
///   bug (CRUSH-SELFHOST-1-AMEND-1); the canonical resolution was to
///   flip both in lockstep so that legacy `capsule_type = "Crush"` reaches
///   `CapsuleType::Crush` directly. AMEND-1 is RESOLVED; this test is
///   the regression guard against the bug re-emerging.
#[test]
fn sdk_matrix_cell_2_legacy_capsule_type_auto_migrates_to_language() {
    let dir = tempfile::tempdir().expect("tempdir");
    let manifest_path = dir.path().join("capsule.toml");
    // `capsule_type = "Crush"` is auto-migrated by `Manifest::from_str` →
    // `language = "crush"` (HALF 1, lowercased on insert — see
    // `crates/crush-pkg/src/manifest.rs::from_str`).
    let legacy_src = r#"[capsule]
name = "legacy-migration-fixture"
capsule_type = "Crush"
entry = "main.crush"
"#;
    std::fs::write(&manifest_path, legacy_src).expect("write manifest");
    let manifest = Manifest::from_file(&manifest_path).expect("manifest must parse");
    // ─── HALF 1 — auto-migrate lowercases the value ─────────────────
    assert_eq!(
        manifest.capsule.language, "crush",
        "CELL_2 audit (HALF 1, auto-migrate case): legacy `capsule_type = \"Crush\"` \
         is consumed by `Manifest::from_str` for auto-migration; the resulting \
         `language` field MUST be lowercased to `\"crush\"` (the shard the \
         dispatcher keys on). If this assertion fails, the auto-migrate \
         path no longer normalises case — fix `Manifest::from_str`'s \
         auto-migrate block (HALF 1) and this assertion will pass again."
    );
    // ─── HALF 2 — dispatcher reaches Crush for legacy manifests ──────
    // The case-insensitive dispatcher in `language_to_capsule_type`
    // (HALF 2) catches mixed-case input directly, so legacy manifests
    // reach `CapsuleType::Crush` regardless of which HALF the regression
    // touches.
    assert_eq!(
        language_to_capsule_type(&manifest.capsule.language),
        CapsuleType::Crush,
        "CELL_2 audit (HALF 2, dispatch shape): `language_to_capsule_type` MUST \
         return `CapsuleType::Crush` for the legacy auto-migrated \
         `language = \"crush\"` value. If this fails, the dispatcher fell \
         through to `_ => CapsuleType::Auto` — fix `language_to_capsule_type`'s \
         case-insensitive shard (HALF 2) and this assertion will pass again."
    );
}
