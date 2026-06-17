//! Integration tests for crush-lang-sdk.

use crush_lang_sdk::{HostCapsBuilder, ProgramBuilder, Runtime};
use std::io::Write;

#[test]
fn hello_world_capsule() {
    let program = ProgramBuilder::new()
        .name("hello")
        .permission("io.print")
        .line(".func main")
        .line(r#"PUSH_STR "hello, world""#)
        .line(r#"CAP_CALL "io.print" 1"#)
        .line("HALT")
        .build()
        .expect("build");

    let result = Runtime::new().run(&program).expect("run");
    assert_eq!(result.output, "hello, world");
    assert!(result.halted);
}

#[test]
fn string_operations() {
    let program = ProgramBuilder::new()
        .name("strings")
        .permission("io.print")
        .permission("str.concat")
        .permission("str.len")
        .line(".func main")
        .line(r#"PUSH_STR "a""#)
        .line(r#"PUSH_STR "b""#)
        .line(r#"CAP_CALL "str.concat" 2"#)
        .line(r#"CAP_CALL "io.print" 1"#)
        .line(r#"PUSH_STR "xyz""#)
        .line(r#"CAP_CALL "str.len" 1"#)
        .line(r#"CAP_CALL "io.print" 1"#)
        .line("HALT")
        .build()
        .expect("build");

    let result = Runtime::new().run(&program).expect("run");
    assert_eq!(result.output, "ab3");
}

#[test]
fn run_from_blob_roundtrip() {
    let program = ProgramBuilder::new()
        .name("blobby")
        .permission("io.print")
        .line(".func main")
        .line(r#"PUSH_STR "from blob""#)
        .line(r#"CAP_CALL "io.print" 1"#)
        .line("HALT")
        .build()
        .expect("build");

    let blob = program.to_blob();
    let result = Runtime::new().run_blob(&blob).expect("run blob");
    assert_eq!(result.output, "from blob");
}

#[test]
fn host_fs_capabilities() {
    let mut tmp = tempfile::NamedTempFile::new().unwrap();
    writeln!(tmp, "file contents").unwrap();
    let dir = tmp.path().parent().unwrap().to_str().unwrap();
    let file_name = tmp.path().file_name().unwrap().to_string_lossy();

    let program = ProgramBuilder::new()
        .permission("io.print")
        .permission("fs.read")
        .line(".func main")
        .line(format!(r#"PUSH_STR "{}""#, file_name))
        .line(r#"CAP_CALL "fs.read" 1"#)
        .line(r#"CAP_CALL "io.print" 1"#)
        .line("HALT")
        .build()
        .expect("build");

    let host_caps = HostCapsBuilder::new().fs(true).fs_root(dir).build();

    let result = Runtime::new()
        .with_host_caps(host_caps)
        .run(&program)
        .expect("run");

    assert_eq!(result.output.trim(), "file contents");
}

#[test]
fn host_bus_capabilities() {
    let program = ProgramBuilder::new()
        .permission("io.print")
        .permission("message_bus.publish")
        .permission("message_bus.subscribe")
        .permission("message_bus.recv")
        .line(".func main")
        .line(r#"PUSH_STR "t1""#)
        .line(r#"CAP_CALL "message_bus.subscribe" 1"#)
        .line(r#"PUSH_STR "t1""#)
        .line(r#"PUSH_STR "hello-bus""#)
        .line(r#"CAP_CALL "message_bus.publish" 2"#)
        .line(r#"CAP_CALL "message_bus.recv" 0"#)
        .line(r#"CAP_CALL "io.print" 1"#)
        .line("HALT")
        .build()
        .expect("build");

    let host_caps = HostCapsBuilder::new().bus(true).build();

    let result = Runtime::new()
        .with_host_caps(host_caps)
        .run(&program)
        .expect("run");

    assert!(result.output.contains("hello-bus"));
}

#[test]
fn host_process_capabilities() {
    let program = ProgramBuilder::new()
        .permission("io.print")
        .permission("process.exec")
        .line(".func main")
        .line(r#"PUSH_STR "echo""#)
        .line(r#"PUSH_STR "crush-process""#)
        .line(r#"CAP_CALL "process.exec" 2"#)
        .line(r#"CAP_CALL "io.print" 1"#)
        .line("HALT")
        .build()
        .expect("build");

    let host_caps = HostCapsBuilder::new().process(true).build();
    let result = Runtime::new()
        .with_host_caps(host_caps)
        .run(&program)
        .expect("run");

    assert!(result.output.contains("crush-process"));
    assert!(result.output.contains("\"exit_code\":0"));
}

#[test]
fn host_crypto_capabilities() {
    let program = ProgramBuilder::new()
        .permission("io.print")
        .permission("crypto.sha256")
        .permission("crypto.random")
        .line(".func main")
        .line(r#"PUSH_STR "hello""#)
        .line(r#"CAP_CALL "crypto.sha256" 1"#)
        .line(r#"CAP_CALL "io.print" 1"#)
        .line("HALT")
        .build()
        .expect("build");

    let host_caps = HostCapsBuilder::new().crypto(true).build();
    let result = Runtime::new()
        .with_host_caps(host_caps)
        .run(&program)
        .expect("run");

    assert!(
        result
            .output
            .contains("2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824")
    );
}

#[cfg(feature = "graphics")]
#[test]
fn host_graphics_capabilities() {
    let program = ProgramBuilder::new()
        .permission("io.print")
        .permission("graphics.canvas")
        .permission("graphics.rect")
        .permission("graphics.to_svg")
        .line(".func main")
        .line(r#"PUSH 100"#)
        .line(r#"PUSH 50"#)
        .line(r#"CAP_CALL "graphics.canvas" 2"#)
        .line(r#"DUP"#)
        .line(r#"PUSH 10"#)
        .line(r#"PUSH 20"#)
        .line(r#"PUSH 30"#)
        .line(r#"PUSH 40"#)
        .line(r#"PUSH_STR "red""#)
        .line(r#"CAP_CALL "graphics.rect" 6"#)
        .line(r#"CAP_CALL "graphics.to_svg" 1"#)
        .line(r#"CAP_CALL "io.print" 1"#)
        .line("HALT")
        .build()
        .expect("build");

    let host_caps = HostCapsBuilder::new().graphics(true).build();
    let result = Runtime::new()
        .with_host_caps(host_caps)
        .run(&program)
        .expect("run");

    assert!(
        result
            .output
            .starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\"")
    );
    assert!(result.output.contains("<rect"));
}

#[test]
fn host_akg_capabilities() {
    let program = ProgramBuilder::new()
        .permission("io.print")
        .permission("akg.write")
        .permission("akg.read")
        .line(".func main")
        .line(r#"PUSH_STR "u1""#)
        .line(r#"PUSH_STR "{\"title\":\"hello\"}""#)
        .line(r#"CAP_CALL "akg.write" 2"#)
        .line(r#"PUSH_STR "u1""#)
        .line(r#"CAP_CALL "akg.read" 1"#)
        .line(r#"CAP_CALL "io.print" 1"#)
        .line("HALT")
        .build()
        .expect("build");

    let host_caps = HostCapsBuilder::new().akg(true).build();

    let result = Runtime::new()
        .with_host_caps(host_caps)
        .run(&program)
        .expect("run");

    assert!(result.output.contains("\"title\":\"hello\""));
}

#[test]
fn host_env_capabilities() {
    let program = ProgramBuilder::new()
        .permission("io.print")
        .permission("env.get")
        .line(".func main")
        .line(r#"PUSH_STR "CRUSH_TEST_VAR""#)
        .line(r#"CAP_CALL "env.get" 1"#)
        .line(r#"CAP_CALL "io.print" 1"#)
        .line("HALT")
        .build()
        .expect("build");

    let host_caps = HostCapsBuilder::new()
        .env(true)
        .with_env_var("CRUSH_TEST_VAR", "crush-value")
        .build();

    let result = Runtime::new()
        .with_host_caps(host_caps)
        .run(&program)
        .expect("run");

    assert_eq!(result.output, "crush-value");
}

#[test]
fn quotas_stop_infinite_loops() {
    let program = ProgramBuilder::new()
        .name("loop")
        .line(".func main")
        .line("loop:")
        .line("JMP loop")
        .line("HALT")
        .build()
        .expect("build");

    let mut quotas = crush_lang_sdk::Quotas::default();
    quotas.max_steps = 5;

    let err = Runtime::with_quotas(quotas)
        .run(&program)
        .expect_err("should exceed step quota");

    assert!(err.to_string().contains("instruction quota exceeded"));
}
