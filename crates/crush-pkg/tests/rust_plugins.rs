//! Host-approved Rust plugin grants through both `crush-pkg` launch paths.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug)]
enum Flow {
    Run,
    Bare,
}

impl Flow {
    const ALL: [Self; 2] = [Self::Run, Self::Bare];

    fn name(self) -> &'static str {
        match self {
            Self::Run => "run",
            Self::Bare => "bare",
        }
    }
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crates/crush-pkg is two levels below the workspace root")
        .to_path_buf()
}

fn plugin_library() -> &'static PathBuf {
    static PLUGIN_LIBRARY: OnceLock<PathBuf> = OnceLock::new();
    PLUGIN_LIBRARY.get_or_init(|| {
        let root = workspace_root();
        let target = tempfile::tempdir().expect("create plugin target directory");
        let output = Command::new("cargo")
            .current_dir(&root)
            .args(["build", "--locked", "--offline", "--manifest-path"])
            .arg(root.join("crates/crush-plugin-example/Cargo.toml"))
            .arg("--target-dir")
            .arg(target.path())
            .output()
            .expect("build crush-plugin-example cdylib");
        assert!(
            output.status.success(),
            "building crush-plugin-example failed:\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        let library = target.path().join("debug").join(format!(
            "{}crush_plugin_example{}",
            std::env::consts::DLL_PREFIX,
            std::env::consts::DLL_SUFFIX
        ));
        assert!(
            library.is_file(),
            "plugin library was not built: {}",
            library.display()
        );

        // Preserve the temporary target as long as this integration-test
        // process may load the library.
        let _target_dir = target.keep();
        library
    })
}

fn package(root: &Path, name: &str, body: &str, required: &[&str]) -> PathBuf {
    let dir = root.join(name);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    let mut manifest = format!(
        "[capsule]\nname = \"{name}\"\nversion = \"0.1.0\"\nentry = \"src/main.crush\"\nlanguage = \"crush\"\n"
    );
    if !required.is_empty() {
        manifest.push_str("\n[capabilities]\nrequired = [");
        for (i, capability) in required.iter().enumerate() {
            if i > 0 {
                manifest.push_str(", ");
            }
            manifest.push('"');
            manifest.push_str(capability);
            manifest.push('"');
        }
        manifest.push_str("]\n");
    }
    std::fs::write(dir.join("capsule.toml"), manifest).unwrap();
    std::fs::write(dir.join("src/main.crush"), body).unwrap();
    dir
}

fn invoke(dir: &Path, flow: Flow, library: Option<&Path>, exports: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_crush-pkg"));
    if matches!(flow, Flow::Run) {
        command.arg("run");
    }
    if let Some(library) = library {
        command.arg("--rust-plugin").arg(library);
    }
    for export in exports {
        command.arg("--plugin-export").arg(export);
    }
    command.current_dir(dir).output().expect("spawn crush-pkg")
}

fn output_text(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

#[test]
fn host_plugin_grants_work_through_run_and_bare_flows() {
    let root = tempfile::tempdir().unwrap();
    let library = plugin_library();

    for flow in Flow::ALL {
        let granted = package(
            root.path(),
            &format!("{}-granted", flow.name()),
            "fn main() { io.print(rust.greet(\"xps\")) }\n",
            &["rust.greet"],
        );
        let output = invoke(&granted, flow, Some(library), &["rust.greet"]);
        let text = output_text(&output);
        assert!(
            output.status.success(),
            "{} grant failed:\n{text}",
            flow.name()
        );
        assert!(
            text.contains("Hello, xps from Rust Extension!"),
            "{} did not call the granted export:\n{text}",
            flow.name()
        );

        let unselected = package(
            root.path(),
            &format!("{}-unselected", flow.name()),
            "fn main() { io.print(rust.version()) }\n",
            &["rust.version"],
        );
        let output = invoke(&unselected, flow, Some(library), &["rust.greet"]);
        let text = output_text(&output);
        assert!(
            !output.status.success(),
            "{} granted an unselected export:\n{text}",
            flow.name()
        );
        assert!(text.contains("unknown capability: rust.version"), "{text}");

        let ungranted = package(
            root.path(),
            &format!("{}-no-grant", flow.name()),
            "fn main() { io.print(rust.greet(\"xps\")) }\n",
            &["rust.greet"],
        );
        let output = invoke(&ungranted, flow, None, &[]);
        let text = output_text(&output);
        assert!(
            !output.status.success(),
            "{} ran without a host grant:\n{text}",
            flow.name()
        );
        assert!(text.contains("unknown capability: rust.greet"), "{text}");

        let unused = package(
            root.path(),
            &format!("{}-unused", flow.name()),
            "fn main() { io.print(42) }\n",
            &[],
        );
        for grant in [false, true] {
            let output = if grant {
                invoke(&unused, flow, Some(library), &["rust.greet"])
            } else {
                invoke(&unused, flow, None, &[])
            };
            let text = output_text(&output);
            assert!(
                output.status.success(),
                "{} unused plugin failed:\n{text}",
                flow.name()
            );
            assert!(
                text.contains("42"),
                "{} lost package output:\n{text}",
                flow.name()
            );
        }

        let missing_export = package(
            root.path(),
            &format!("{}-missing-export", flow.name()),
            "fn main() { io.print(42) }\n",
            &[],
        );
        let output = invoke(&missing_export, flow, Some(library), &["rust.nope"]);
        let text = output_text(&output);
        assert!(
            !output.status.success(),
            "{} accepted a missing export:\n{text}",
            flow.name()
        );
        assert!(
            text.contains("Plugin is missing allowed export(s): rust.nope"),
            "{text}"
        );

        let missing_library = package(
            root.path(),
            &format!("{}-missing-library", flow.name()),
            "fn main() { io.print(42) }\n",
            &[],
        );
        let output = invoke(
            &missing_library,
            flow,
            Some(Path::new("/no/such/crush-plugin.so")),
            &["rust.greet"],
        );
        let text = output_text(&output);
        assert!(
            !output.status.success(),
            "{} accepted a missing library:\n{text}",
            flow.name()
        );
        assert!(text.contains("Failed to load plugin"), "{text}");
    }

    let invalid_args = package(
        root.path(),
        "invalid-plugin-args",
        "fn main() { io.print(42) }\n",
        &[],
    );
    let output = invoke(&invalid_args, Flow::Run, None, &["rust.greet"]);
    assert!(!output.status.success());
    let text = output_text(&output);
    assert!(
        text.contains("--rust-plugin"),
        "unexpected CLI error:\n{text}"
    );

    let output = invoke(&invalid_args, Flow::Run, Some(library), &[]);
    assert!(!output.status.success());
    assert!(output_text(&output).contains("requires at least one --plugin-export"));

    let output = invoke(
        &invalid_args,
        Flow::Run,
        Some(library),
        &["rust.greet", "rust.greet"],
    );
    assert!(!output.status.success());
    assert!(output_text(&output).contains("must be non-empty and unique"));
}
