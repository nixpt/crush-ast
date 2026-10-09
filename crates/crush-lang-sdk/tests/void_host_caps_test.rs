//! CRUSH-183: host capabilities that return nothing (`fs.write`, `akg.write`,
//! `message_bus.publish`, `message_bus.subscribe`) used to end every program with
//! `stack underflow`: the compiler pops the result of every host call, and they
//! pushed none. Now they leave Null, as a statement or as a value.

use crush_lang_sdk::compile::compile_crush_source;
use crush_lang_sdk::{HostCapsBuilder, Runtime};

fn run(src: &str, caps: HostCapsBuilder) -> String {
    let program = compile_crush_source(src).expect("compile");
    Runtime::new().with_host_caps(caps.build()).run(&program).expect("run").output
}

#[test]
fn void_host_caps_as_statements_and_values() {
    let dir = tempfile::tempdir().unwrap();
    let fs = HostCapsBuilder::new().fs(true).fs_root(dir.path().to_string_lossy());
    assert_eq!(
        run("fn main() { fs.write(\"w.txt\", \"x\")\n let r = fs.write(\"v.txt\", \"y\")\n io.print(r)\n return 0 }", fs),
        "null\n"
    );
    assert_eq!(std::fs::read_to_string(dir.path().join("w.txt")).unwrap(), "x");

    let src = "fn main() { akg.write(\"k\", \"1\")\n io.print(\"ok\")\n return 0 }";
    assert_eq!(run(src, HostCapsBuilder::new().akg(true)), "ok\n");

    let src = "fn main() { message_bus.subscribe(\"t\")\n message_bus.publish(\"t\", \"m\")\n io.print(\"ok\")\n return 0 }";
    assert_eq!(run(src, HostCapsBuilder::new().bus(true)), "ok\n");
}
