use crush_lang_sdk::{HostCapsBuilder, Runtime};
use crush_lang_sdk::compile::compile_crush_source;

#[test]
fn dashboard_comprehensive() {
    let source = r#"
fn greet(name) {
    let msg = str.concat("hello, ", name)
    io.print(msg)
}

fn hash(msg) {
    let h = crypto.sha256(msg)
    let label = str.concat("sha256: ", h)
    io.print(label)
}

fn check_env(key) {
    let val = env.get(key)
    if val != null {
        let line = str.concat(str.concat(key, "="), val)
        io.print(line)
    } else {
        let line = str.concat(key, " not set")
        io.print(line)
    }
}

fn main() {
    greet("crush")

    let x = 42
    let y = 10
    if x > y {
        io.print("x > y")
    }

    let msg = "hello " + "world"
    io.print(msg)
    io.print(str.len(msg))

    hash("crush-dashboard")

    check_env("USER")
    check_env("SHELL")
}
"#;

    let prog = compile_crush_source(source).expect("compile dashboard.crush");

    let host_caps = HostCapsBuilder::new()
        .env(true)
        .crypto(true)
        .build();

    let result = Runtime::new()
        .with_host_caps(host_caps)
        .run(&prog)
        .expect("run dashboard.crush");

    assert!(result.halted);
    assert!(result.output.contains("hello, crush"));
    assert!(result.output.contains("x > y"));
    assert!(result.output.contains("hello world"));
    assert!(result.output.contains("11"));
    assert!(result.output.contains("sha256:"));
    assert!(result.output.contains("USER="));
}
