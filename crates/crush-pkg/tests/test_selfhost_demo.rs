//! Integration test for the `crush-selfhost-demo` reference fixture.
//!
//! Closes the "Crush can grow itself" gray zone by proving that a
//! non-trivial feature — **three user-defined fns plus three
//! host-table command registrations** — can be authored entirely
//! in `.crush` source without touching Rust.
//!
//! ## Mechanism
//!
//! 1. Build a `crush-pkg` [`CapsuleRunner`] wired with a recording
//!    [`HostCap`] for `gui.register_command`.
//! 2. Run via [`CapsuleRunner::run`] (the real
//!    `CrushRunner.host_caps` plumbing), which routes
//!    `main.crush::main()`'s three `gui.register_command(...)`
//!    calls through the recorder.
//! 3. Assert the captured `Vec<Vec<String>>` carries all three
//!    expected command IDs.
//!
//! Now that `crush-pkg` ships with a lib facade
//! (`TICKETS/CRUSH-SELFHOST-1.md#constraint-4` fix), this test
//! truly exercises `crush_pkg::runners::CrushRunner` — no longer
//! parallel to its body via `crush_lang_sdk::compile +
//! crush_vm::run_with_caps`. A regression on either side surfaces
//! here as a synchronous build error or test panic.

use std::path::Path;
use std::sync::{Arc, Mutex};

use crush_pkg::manifest::Manifest;
use crush_pkg::runners::{CapsuleRunner, CrushRunner, ExecutionResult};
use crush_vm::host::{HostCap, HostCapSpec, HostCaps};
use crush_vm::vm::Value;

mod test_paths;

/// Records every invocation of the `gui.register_command` host
/// cap. `HostCap::call` takes `&self`, so interior mutability is
/// required to accumulate captures — the `log` is shared with the
/// test via `Arc<Mutex<Vec<Vec<String>>>>`.
struct Recorder {
    log: Arc<Mutex<Vec<Vec<String>>>>,
}

impl Recorder {
    fn new() -> Self {
        Self {
            log: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl HostCap for Recorder {
    fn spec(&self) -> HostCapSpec {
        HostCapSpec {
            name: "gui.register_command".to_string(),
            // `returns: true` is required because the crush-frontend
            // compiler treats every unknown call as an expression that
            // yields one value, then appends a `POP` opcode to discard
            // the result when the call sits in statement position. If
            // we instead return zero, the `POP` runs against an empty
            // stack and the VM raises `StackUnderflow`. Pushing
            // `Value::Null` lets the statement-cleanup POP find its
            // target.
            //
            // This is the contract for any HostCap invoked in
            // STATEMENT position under the current single-shot emitter.
            // Full venue: `TICKETS/CRUSH-FRONTEND-1.md` (option (b)
            // bundles a `register_void_cap` seam in `crush-lang-sdk`
            // that auto-translates user-facing `returns: false` /
            // `Ok(None)` into the VM-required `Ok(Some(Value::Null))`).
            argc: Some(3),
            returns: true,
        }
    }

    fn call(&self, args: Vec<Value>) -> Result<Option<Value>, String> {
        let mut row = Vec::with_capacity(args.len());
        for a in args {
            match a {
                Value::Null => row.push("null".to_string()),
                Value::Bool(b) => row.push(b.to_string()),
                Value::Int(i) => row.push(i.to_string()),
                Value::Float(f) => row.push(f.to_string()),
                Value::Str(s) => row.push(s),
                Value::Error(e) => row.push(format!("error({e})")),
                // Array / Map / Bytes / Handle are not what
                // `register_command` should ever see from a
                // well-formed `.crush` caller; record a tag so a
                // regression is loud instead of silent.
                _ => row.push("<unprintable>".to_string()),
            }
        }
        self.log.lock().unwrap().push(row);
        Ok(Some(Value::Null))
    }
}

#[test]
fn crush_selfhost_demo_runs_and_registers_three_commands() {
    let fixture = test_paths::fixture_root("crush-selfhost-demo");
    let manifest_path = fixture.join("capsule.toml");
    let payload = fixture.join("main.crush");
    assert!(
        manifest_path.exists(),
        "fixture manifest missing at {}",
        manifest_path.display()
    );
    assert!(
        payload.exists(),
        "fixture main.crush missing at {}",
        payload.display()
    );

    let manifest =
        Manifest::from_file(&manifest_path).expect("crush-selfhost-demo/capsule.toml must parse");
    let rec = Recorder::new();
    let calls_handle = rec.log.clone();

    let mut caps = HostCaps::new();
    caps.register(Box::new(rec));

    // Drive the real `CrushRunner` struct end-to-end via the lib
    // facade. Closes the "test bypasses CrushRunner" gap
    // (`CRUSH-SELFHOST-1.md#constraint-4`) — now the integration
    // test exercises the same code path the `crush-pkg run`
    // subcommand takes in production.
    let runner = CrushRunner { host_caps: Some(caps) };
    let result = runner
        .run(&manifest, &payload, &[])
        .expect("CrushRunner::run must drive main.crush to completion with the host table wired in");
    assert!(
        matches!(result, ExecutionResult::Vm),
        "CrushRunner must hand the program to the VM (ExecutionResult::Vm)"
    );

    let calls = calls_handle.lock().unwrap();
    // Surface the captured rows so the test output is itself the
    // evidence — the three `gui.register_command(...)` calls from
    // `main.crush::main()` arrived in the host table with the IDs
    // expected below. Visible in `cargo test -- --nocapture`.
    eprintln!(
        "[crush-selfhost-demo] commands captured by host table: {calls:#?}"
    );
    assert!(
        calls
            .iter()
            .any(|c| c.first().map(String::as_str) == Some("capsule.demo.greet")),
        "expected `gui.register_command(\"capsule.demo.greet\", …)` to land; got {calls:?}"
    );
    assert!(
        calls
            .iter()
            .any(|c| c.first().map(String::as_str) == Some("capsule.demo.echo")),
        "expected `gui.register_command(\"capsule.demo.echo\", …)` to land; got {calls:?}"
    );
    assert!(
        calls
            .iter()
            .any(|c| c.first().map(String::as_str) == Some("capsule.demo.describe_action")),
        "expected `gui.register_command(\"capsule.demo.describe_action\", …)` to land; got {calls:?}"
    );
    assert!(
        calls.len() >= 3,
        "expected at least three register_command calls; got {} ({calls:?})",
        calls.len()
    );
}

#[test]
fn crush_selfhost_demo_runtime_dispatch_is_crush_via_language() {
    // Belt-and-braces: the demo's runtime dispatch must key on
    // `language = "crush"` (the modern field), not on the legacy
    // `capsule_type` (which `Manifest::from_str` auto-migrates
    // into `language` and thus doesn't survive a round-trip
    // through the struct). This is the same signal that
    // `get_runner_for_payload` keys on in production.
    let fixture = test_paths::fixture_root("crush-selfhost-demo").join("capsule.toml");
    let manifest_bytes = std::fs::read(&fixture).expect("read fixture manifest");
    let manifest_str = std::str::from_utf8(&manifest_bytes).expect("manifest UTF-8");
    assert!(
        manifest_str.contains("language = \"crush\""),
        "fixture must keep `language = \"crush\"` (the field CrushRunner dispatches on)"
    );
}

#[test]
fn test_sno_execution() {
    use tempfile::tempdir;
    use casm::{Program, Function, Instruction, Manifest as CasmManifest};
    use std::collections::HashMap;

    let dir = tempdir().unwrap();
    let sno_path = dir.path().join("main.sno");

    let program = Program {
        version: "1.0".to_string(),
        lang: Some("sona".to_string()),
        functions: {
            let mut map = HashMap::new();
            map.insert("main".to_string(), Function {
                params: vec![],
                locals: vec![],
                body: vec![
                    Instruction {
                        op: "push_int".to_string(),
                        lang: Some("sona".to_string()),
                        meta: None,
                        args: serde_json::json!({ "value": 42 }),
                    },
                    Instruction {
                        op: "cap_call".to_string(),
                        lang: Some("sona".to_string()),
                        meta: None,
                        args: serde_json::json!({ "name": "io.print", "argc": 1 }),
                    },
                    Instruction {
                        op: "push_null".to_string(),
                        lang: Some("sona".to_string()),
                        meta: None,
                        args: serde_json::json!({}),
                    },
                    Instruction {
                        op: "ret".to_string(),
                        lang: Some("sona".to_string()),
                        meta: None,
                        args: serde_json::json!({}),
                    },
                ],
            });
            map
        },
        manifest: CasmManifest {
            permissions: vec!["io.print".to_string()],
        },
    };

    let serialized = serde_json::to_string_pretty(&program).unwrap();
    std::fs::write(&sno_path, serialized).unwrap();

    let manifest_toml = r#"
[capsule]
name = "test_sno"
entry = "main.sno"
language = "crush"
"#;
    let manifest_path = dir.path().join("Capsule.toml");
    std::fs::write(&manifest_path, manifest_toml).unwrap();

    let manifest = Manifest::from_file(&manifest_path).expect("manifest must parse");
    let runner = CrushRunner::default();
    let result = runner.run(&manifest, &sno_path, &[]).expect("run must succeed");

    assert!(matches!(result, ExecutionResult::Vm));
}

