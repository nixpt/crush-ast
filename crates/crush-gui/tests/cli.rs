//! The `crush-gui` binary: the grant gate, end to end. Stdin is always null.

use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

fn counter() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/gui/counter.crush")
}

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_crush-gui"))
        .args(args)
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

#[test]
fn refused_without_ui_naming_the_flag() {
    let out = run(&["run", counter().to_str().unwrap()]);
    let err = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "{err}");
    assert!(err.contains("nothing was run"), "{err}");
    assert!(
        err.contains("--ui: ui.button, ui.next_event, ui.render, ui.title"),
        "{err}"
    );
}

#[test]
fn net_and_fs_do_not_grant_ui() {
    let out = run(&[
        "run",
        counter().to_str().unwrap(),
        "--net",
        "--fs",
        "--env",
        "--time",
    ]);
    let err = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "{err}");
    assert!(err.contains("--ui:"), "{err}");
}

#[test]
fn with_ui_but_no_display_it_says_so_instead_of_panicking() {
    let out = run(&["run", counter().to_str().unwrap(), "--ui"]);
    let err = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "{err}");
    assert!(err.contains("no display"), "{err}");
    assert!(!err.contains("panicked"), "{err}");
}
