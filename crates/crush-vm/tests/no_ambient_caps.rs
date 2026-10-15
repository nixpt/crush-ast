use std::process::Command;

use crush_vm::assemble;

fn run_program(asm: &str, permissions: &[&str], suffix: &str) -> std::process::Output {
    let program = assemble(asm, Some(permissions), None).expect("assemble fixture");
    let path = std::env::temp_dir().join(format!(
        "crush-vm-no-ambient-{}-{suffix}.cvm1",
        std::process::id()
    ));
    std::fs::write(&path, program.to_blob()).expect("write fixture");
    let result = Command::new(env!("CARGO_BIN_EXE_crush-vm"))
        .arg("run")
        .arg(&path)
        .output()
        .expect("run crush-vm");
    let _ = std::fs::remove_file(path);
    result
}

#[test]
fn cargo_is_not_ambient_in_standalone_runner() {
    let result = run_program(
        r#"PUSH_STR "--version"
NEW_ARRAY 1
PUSH_NULL
CAP_CALL "os.cargo" 2
HALT"#,
        &["os.cargo"],
        "cargo",
    );

    assert!(!result.status.success());
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(stderr.contains("os.cargo"), "unexpected error: {stderr}");
}

#[test]
fn arbitrary_library_loading_is_not_ambient_in_standalone_runner() {
    let result = run_program(
        r#"PUSH_STR "/tmp/untrusted-crush-plugin.so"
PUSH_STR "greet"
CAP_CALL "__crush_ffi__" 2
HALT"#,
        &["__crush_ffi__"],
        "ffi",
    );

    assert!(!result.status.success());
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(
        stderr.contains("__crush_ffi__"),
        "unexpected error: {stderr}"
    );
}
