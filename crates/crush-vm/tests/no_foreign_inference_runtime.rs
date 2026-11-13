//! ONNX Runtime and other foreign inference runtimes were removed from
//! crush-ast (CRUSH-256, #159/#160) and must not come back: model inference
//! goes through zorro. This fails if any of them is anywhere in the
//! workspace's dependency graph — every member, normal, dev and build
//! dependencies, all features, all targets.
//!
//! Modelled on zpmk's `crates/zpmk-ai/tests/no_onnx.rs`. It lives in
//! crush-vm only so that it runs in CI's `Test (core)` job; it checks the
//! whole workspace and the two crates it excludes (crush-web, crush-vm-py),
//! not just crush-vm.
//!
//! Not covered: hand-written FFI or a `dlopen` of a runtime library (no
//! crate shows up), and the two benchmark harnesses under `docs/benchmarks/`
//! (standalone, not built in CI; their only deps are workspace crates and
//! `anyhow`).
//!
//! **If this fails:** do not add the crate to an allow list. Drop the
//! dependency (or the feature that pulls it in) and route inference through
//! zorro.

use std::collections::BTreeSet;
use std::process::Command;

/// Crates that are an inference runtime or a binding to one.
const FORBIDDEN: &[&str] = &[
    "ort",
    "ort-sys",
    "onnxruntime",
    "onnxruntime-sys",
    "tract",
    "candle",
    "fastembed",
    "tch",
    "torch-sys",
    "burn",
    "rust-bert",
    "wonnx",
    "llama-cpp-2",
    "llama_cpp",
    "llama-cpp-sys-2",
    "ggml",
    "rten",
    "tensorflow",
    "tflitec",
    "openvino",
    "ct2rs",
    "ctranslate2",
    "mlx-rs",
    "mlx-sys",
    "ncnn-rs",
    "tvm",
    "mistralrs",
    "kalosm",
    // rustformers' ggml runtime.
    "llm",
    // These vendor ggml / ONNX Runtime inside a differently named crate.
    "whisper-rs",
    "whisper-rs-sys",
    "sherpa-rs",
    "sherpa-rs-sys",
];

/// Families published as many crates (`tract-onnx`, `candle-nn`,
/// `burn-core`, `rten-tensor`, ...).
const FORBIDDEN_PREFIXES: &[&str] = &[
    "ort-",
    "onnxruntime",
    "tract-",
    "candle-",
    "burn-",
    "rten-",
    "llama-cpp",
    "llama_cpp",
    "ggml",
    "wonnx",
    "tensorflow",
    "tflite",
    "openvino",
    "ctranslate2",
    "mlx-",
    "ncnn",
    "tvm-",
    "mistralrs",
    "kalosm",
    "whisper-rs",
    "sherpa-rs",
];

fn is_forbidden(name: &str) -> bool {
    FORBIDDEN.contains(&name)
        || FORBIDDEN_PREFIXES.iter().any(|p| name.starts_with(p))
        || name.contains("onnx")
}

/// Every package name in `cargo tree` for one manifest (with `--workspace`
/// when it is the root workspace).
fn packages(manifest_dir: &str, workspace: bool) -> BTreeSet<String> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    // The root lockfile is kept current (`scripts/bump-version.sh` refreshes
    // it), so `--locked` there. The excluded crates' lockfiles are not: a
    // version bump leaves their path-crate entries behind, and `--locked`
    // would then fail every PR after a release. For those, let `cargo tree`
    // resolve and put the lockfile back afterwards, so the check never
    // changes the tree.
    let lock_path = format!("{manifest_dir}/Cargo.lock");
    let saved_lock = (!workspace)
        .then(|| std::fs::read(&lock_path).ok())
        .flatten();
    let mut args = vec!["tree"];
    if workspace {
        args.extend(["--locked", "--workspace"]);
    }
    args.extend([
        "--all-features",
        "--edges",
        "normal,dev,build",
        "--target",
        "all",
        "--prefix",
        "none",
        "--format",
        "{p}",
    ]);
    let out = Command::new(cargo)
        .args(&args)
        .current_dir(manifest_dir)
        .output()
        .expect("run cargo tree");
    if let Some(saved) = saved_lock {
        std::fs::write(&lock_path, saved).expect("restore Cargo.lock");
    }
    assert!(
        out.status.success(),
        "cargo tree in {manifest_dir} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| l.split_whitespace().next())
        .map(str::to_string)
        .collect()
}

#[test]
fn no_foreign_inference_runtime_in_the_workspace_dependency_graph() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    // The workspace, plus the two crates its `exclude` keeps out of it (each
    // is its own workspace). Each entry: dir, `--workspace`, packages that
    // must show up (so an empty or misparsed tree can't pass).
    let graphs: [(String, bool, &[&str]); 3] = [
        (
            root.to_string(),
            true,
            &["crush-vm", "crush-lang-sdk", "crush-gui"],
        ),
        (
            format!("{root}/crates/crush-web"),
            false,
            &["crush-web", "crush-vm"],
        ),
        (
            format!("{root}/crates/crush-vm-py"),
            false,
            &["crush-vm-py", "crush-vm"],
        ),
    ];
    let mut hits = BTreeSet::new();
    for (dir, workspace, expected) in &graphs {
        let pkgs = packages(dir, *workspace);
        for member in *expected {
            assert!(
                pkgs.contains(*member),
                "sanity: `{member}` missing from cargo tree in {dir} ({} packages)",
                pkgs.len()
            );
        }
        hits.extend(pkgs.into_iter().filter(|p| is_forbidden(p)));
    }
    assert!(
        hits.is_empty(),
        "foreign inference runtime crates in the dependency graph: {hits:?} \
         (ONNX & co. were removed, CRUSH-256; use zorro)"
    );
}
