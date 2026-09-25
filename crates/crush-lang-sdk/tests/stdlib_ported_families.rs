//! W10 / CRUSH-122: exosphere's stdlib families and nanovm's System Bytecode
//! Layer, exercised through the real pipeline — Crush source → parse →
//! compile → execute on the runtime — not by calling the caps directly
//! (the per-cap unit tests live next to each family in `src/stdlib/`).
#![cfg(feature = "stdlib")]

use crush_lang_sdk::{HostCapsBuilder, Runtime};

fn run(source: &str, caps: HostCapsBuilder) -> Result<String, String> {
    let program = crush_lang_sdk::compile::compile_crush_source(source)
        .map_err(|e| format!("compile: {e}"))?;
    Runtime::new()
        .with_host_caps(caps.build())
        .run(&program)
        .map(|r| r.output)
        .map_err(|e| e.to_string())
}

fn run_stdlib(source: &str) -> String {
    run(source, HostCapsBuilder::new().stdlib(true)).expect("program runs")
}

#[test]
fn sbl_system_caps_run_the_crush_implementation() {
    let out = run_stdlib(
        r#"
        let path = "/mnt/data/../user/./projects//exosphere";
        io.print(system.path_normalize(path));
        io.print(system.format_info("node-001", "1.2.3"));
        io.print(system.format_node_info("n", "2"));
        "#,
    );
    assert_eq!(
        out,
        "/mnt/user/projects/exosphere\nNode: node-001 (v1.2.3)\nNode: n (v2)\n"
    );
}

#[test]
fn sbl_caps_need_the_stdlib() {
    let err = run(
        r#"io.print(system.path_normalize("/a/../b"));"#,
        HostCapsBuilder::new(),
    )
    .unwrap_err();
    assert!(err.contains("system.path_normalize"), "{err}");
}

#[test]
fn buffers_bytes_and_binary_codecs() {
    let out = run_stdlib(
        r#"
        fn main() {
            let buf = buffer.alloc(6);
            binary.write_u16_be(buf, 0, 513);
            buffer.write(buf, 2, bytes.from_string("hi"));
            print(binary.read_u16_le(buf, 0));
            print(bytes.to_string(buffer.read(buf, 2, 2)));
            let frozen = buffer.freeze(buf);
            print(bytes.len(frozen));
            print(bytes.len(buf));
        }
        "#,
    );
    assert_eq!(out, "258\nhi\n6\n0\n");
}

#[test]
fn results_records_text_time_env() {
    let out = run_stdlib(
        r#"
        fn main() {
            let r = result.ok(42);
            print(result.is_ok(r));
            print(result.unwrap(r));
            print(result.is_ok(result.err("boom")));

            let people = [{name: "cy", age: 40}, {name: "al", age: 30}];
            print(collections.pluck(collections.sort_by(people, "age"), "name"));
            print(collections.keys(collections.merge({b: 2}, {a: 1})));
            print(collections.all(people, "age", 30));

            print(text.uniq(text.sort(["b", "a", "b"])));
            let t = time.parse("2026-09-25 12:34:56", "%Y-%m-%d %H:%M:%S");
            print(time.format(t, "%d/%m/%Y"));
            print(len(env.os()) > 0);
        }
        "#,
    );
    assert_eq!(
        out,
        "true\n42\nfalse\n[al, cy]\n[a, b]\nfalse\n[a, b]\n25/09/2026\ntrue\n"
    );
}

#[test]
fn unwrapping_an_err_is_a_runtime_error() {
    let err = run(
        r#"io.print(result.unwrap(result.err("boom")));"#,
        HostCapsBuilder::new().stdlib(true),
    )
    .unwrap_err();
    assert!(err.contains("called on err: boom"), "{err}");
}

#[test]
fn file_text_tools_are_fs_gated_and_sandboxed() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("notes.txt"), "b 1\na 2\nb 3\n").unwrap();
    let fs = || {
        HostCapsBuilder::new()
            .stdlib(true)
            .fs(true)
            .fs_root(dir.path().to_str().unwrap())
    };

    let out = run(
        r#"
        io.print(text.head("notes.txt", 2));
        io.print(text.tail("notes.txt", 1));
        io.print(text.uniq(text.sort(text.cut("notes.txt", " ", 1))));
        io.print(collections.values(text.wc("notes.txt")));
        io.print(len(text.grep("^b", "notes.txt")));
        "#,
        fs(),
    )
    .expect("runs");
    assert_eq!(out, "[b 1, a 2]\n[b 3]\n[a, b]\n[12, 3, 6]\n2\n");

    // without --fs the file tools do not exist, even with the stdlib on
    let err = run(
        r#"io.print(text.wc("notes.txt"));"#,
        HostCapsBuilder::new().stdlib(true),
    )
    .unwrap_err();
    assert!(err.contains("text.wc"), "{err}");

    // and with it they cannot leave the sandbox root
    let err = run(r#"io.print(text.wc("../outside.txt"));"#, fs()).unwrap_err();
    assert!(err.contains("escapes sandbox"), "{err}");
}
