//! `HostCapsBuilder::fs_memory()` gives the same answers as `--fs` on an empty
//! `fs_root` directory: every script below runs, step by step, through the
//! real `fs.*` / `text.*` caps on a temporary directory (the host's disk) and
//! on a fresh in-memory tree, and each step must return the same value or fail
//! with the same message.

use crush_lang_sdk::HostCapsBuilder;
use crush_vm::HostCaps;
use crush_vm::vm::Value;

fn s(v: &str) -> Value {
    Value::Str(v.to_string())
}

fn b(v: bool) -> Value {
    Value::Bool(v)
}

/// One step's outcome as text: the value, or the error with the disk root
/// (only `text.*` errors name a host path) replaced by `<root>`.
///
/// `fs.stat`'s `modified` is a wall-clock time, so two runs a moment apart
/// cannot share it: both sides are reduced to `modified=<time>` and the rest
/// of the map is still compared (CRUSH-240).
fn run(caps: &HostCaps, root: &str, cap: &str, args: &[Value]) -> String {
    let c = caps.get(cap).unwrap_or_else(|| panic!("{cap} not registered"));
    match c.call(args.to_vec()) {
        Ok(Some(v)) => format!("ok {}", outcome(cap, &v)),
        Ok(None) => "ok".to_string(),
        Err(e) => {
            let e = if root.is_empty() { e } else { e.replace(root, "<root>") };
            format!("err {}", e.replace("<root>/", "/"))
        }
    }
}

fn outcome(cap: &str, v: &Value) -> String {
    if cap != "fs.stat" {
        return v.to_string();
    }
    let Value::Map(m) = v else {
        panic!("fs.stat did not return a map: {v:?}");
    };
    let mut parts: Vec<String> = m
        .borrow()
        .iter()
        .map(|(k, v)| {
            if k == "modified" {
                assert!(matches!(v, Value::Int(_)), "fs.stat modified is not an int: {v:?}");
                "modified=<time>".to_string()
            } else {
                format!("{k}: {v}")
            }
        })
        .collect();
    parts.sort();
    format!("{{{}}}", parts.join(", "))
}

type Step = (&'static str, Vec<Value>);

fn scripts() -> Vec<(&'static str, Vec<Step>)> {
    vec![
        ("read write exists", vec![
            ("fs.exists", vec![s("notes.txt")]),
            ("fs.read", vec![s("notes.txt")]),
            ("fs.write", vec![s("notes.txt"), s("hello")]),
            ("fs.exists", vec![s("notes.txt")]),
            ("fs.read", vec![s("notes.txt")]),
            ("fs.write", vec![s("notes.txt"), s("hi")]),
            ("fs.read", vec![s("notes.txt")]),
            ("fs.cat", vec![s("notes.txt")]),
            ("fs.write", vec![s("n.txt"), Value::Int(42)]),
            ("fs.read", vec![s("n.txt")]),
        ]),
        ("the guide's examples", vec![
            ("fs.write", vec![s("out.txt"), s("hello")]),
            ("fs.read", vec![s("out.txt")]),
            ("fs.mkdir", vec![s("sub")]),
            ("fs.write", vec![s("sub/a.txt"), s("a")]),
            ("fs.list", vec![s("sub")]),
            ("fs.read", vec![s("../outside.txt")]),
            ("fs.exists", vec![s("missing.txt")]),
        ]),
        ("escapes", vec![
            ("fs.read", vec![s("../x")]),
            ("fs.write", vec![s("a/../../x"), s("1")]),
            ("fs.read", vec![s("/etc/hostname")]),
            ("fs.list", vec![s("..")]),
            ("fs.cd", vec![s("..")]),
            ("fs.mkdir", vec![s("d")]),
            ("fs.cd", vec![s("d")]),
            ("fs.read", vec![s("../../x")]),
            ("fs.pwd", vec![]),
        ]),
        ("errors", vec![
            ("fs.read", vec![s("nope")]),
            ("fs.mkdir", vec![s("d")]),
            ("fs.read", vec![s("d")]),
            ("fs.write", vec![s("d"), s("x")]),
            ("fs.mkdir", vec![s("d")]),
            ("fs.mkdir", vec![s("x/y")]),
            ("fs.write", vec![s("f"), s("1")]),
            ("fs.mkdir", vec![s("f")]),
            ("fs.mkdir", vec![s("f/g")]),
            ("fs.mkdir", vec![s("f"), b(true)]),
            ("fs.mkdir", vec![s("f/g/h"), b(true)]),
            ("fs.read", vec![s("f/x")]),
            ("fs.write", vec![s("f/x"), s("1")]),
            ("fs.write", vec![s("nodir/x"), s("1")]),
            ("fs.list", vec![s("f")]),
            ("fs.list", vec![s("nope")]),
            ("fs.ls", vec![s("f")]),
            ("fs.cd", vec![s("f")]),
            ("fs.cd", vec![s("nope")]),
        ]),
        ("mkdir -p, ls, find", vec![
            ("fs.mkdir", vec![s("a/b/c"), b(true)]),
            ("fs.mkdir", vec![s("a/b/c"), b(true)]),
            ("fs.write", vec![s("a/one.txt"), s("1")]),
            ("fs.write", vec![s("a/b/two.txt"), s("2")]),
            ("fs.write", vec![s("a/b/c/three.md"), s("3")]),
            ("fs.ls", vec![]),
            ("fs.ls", vec![s("a")]),
            ("fs.find", vec![s("a")]),
            ("fs.find", vec![s("a"), s("*.txt")]),
            ("fs.find", vec![s("."), s("t??ee.*")]),
            ("fs.find", vec![s("nope")]),
            ("fs.list", vec![s(".")]),
        ]),
        ("cd pwd", vec![
            ("fs.pwd", vec![]),
            ("fs.mkdir", vec![s("x/y"), b(true)]),
            ("fs.cd", vec![s("x")]),
            ("fs.pwd", vec![]),
            ("fs.write", vec![s("in-x.txt"), s("x")]),
            ("fs.cd", vec![s("y")]),
            ("fs.pwd", vec![]),
            ("fs.read", vec![s("../in-x.txt")]),
            ("fs.cd", vec![s("../..")]),
            ("fs.pwd", vec![]),
            ("fs.read", vec![s("x/in-x.txt")]),
            ("fs.rm", vec![s("x"), b(true)]),
        ]),
        ("rm", vec![
            ("fs.rm", vec![s("nope")]),
            ("fs.write", vec![s("f"), s("1")]),
            ("fs.rm", vec![s("f")]),
            ("fs.exists", vec![s("f")]),
            ("fs.mkdir", vec![s("d/e"), b(true)]),
            ("fs.write", vec![s("d/e/x"), s("1")]),
            ("fs.rm", vec![s("d")]),
            ("fs.rm", vec![s("d"), b(true)]),
            ("fs.exists", vec![s("d")]),
            ("fs.rm", vec![s(".")]),
            ("fs.rm", vec![s("."), b(true)]),
        ]),
        ("mv", vec![
            ("fs.mv", vec![s("nope"), s("x")]),
            ("fs.write", vec![s("a"), s("A")]),
            ("fs.mv", vec![s("a"), s("b")]),
            ("fs.exists", vec![s("a")]),
            ("fs.read", vec![s("b")]),
            ("fs.write", vec![s("c"), s("C")]),
            ("fs.mv", vec![s("c"), s("b")]),
            ("fs.read", vec![s("b")]),
            ("fs.mkdir", vec![s("d")]),
            ("fs.mv", vec![s("b"), s("d")]),
            ("fs.mv", vec![s("b"), s("d/b")]),
            ("fs.read", vec![s("d/b")]),
            ("fs.mkdir", vec![s("e")]),
            ("fs.mv", vec![s("e"), s("d")]),
            ("fs.mv", vec![s("d"), s("d/inner")]),
            ("fs.write", vec![s("f"), s("F")]),
            ("fs.mv", vec![s("d"), s("f")]),
            ("fs.mv", vec![s("f"), s("nodir/f")]),
            ("fs.mv", vec![s("f"), s("f")]),
            ("fs.mkdir", vec![s("empty")]),
            ("fs.mv", vec![s("e"), s("empty")]),
            ("fs.ls", vec![]),
        ]),
        ("cp", vec![
            ("fs.cp", vec![s("nope"), s("x")]),
            ("fs.write", vec![s("a"), s("A")]),
            ("fs.cp", vec![s("a"), s("b")]),
            ("fs.read", vec![s("b")]),
            ("fs.mkdir", vec![s("t/u"), b(true)]),
            ("fs.write", vec![s("t/u/x"), s("X")]),
            ("fs.cp", vec![s("t"), s("t2")]),
            ("fs.cp", vec![s("t"), s("t2"), b(true)]),
            ("fs.find", vec![s("t2")]),
            ("fs.read", vec![s("t2/u/x")]),
            ("fs.cp", vec![s("t"), s("t/inside"), b(true)]),
            ("fs.cp", vec![s("a"), s("t")]),
            ("fs.cp", vec![s("a"), s("nodir/a")]),
        ]),
        ("touch", vec![
            ("fs.touch", vec![s("new")]),
            ("fs.read", vec![s("new")]),
            ("fs.write", vec![s("kept"), s("K")]),
            ("fs.touch", vec![s("kept")]),
            ("fs.read", vec![s("kept")]),
            ("fs.mkdir", vec![s("d")]),
            ("fs.touch", vec![s("d")]),
            ("fs.touch", vec![s("nodir/x")]),
        ]),
        // CRUSH-240: type, size and time, and appending instead of replacing.
        ("stat, type and append", vec![
            ("fs.stat", vec![s("nope")]),
            ("fs.is_file", vec![s("nope")]),
            ("fs.is_dir", vec![s("nope")]),
            ("fs.write", vec![s("f.txt"), s("hello")]),
            ("fs.stat", vec![s("f.txt")]),
            ("fs.is_file", vec![s("f.txt")]),
            ("fs.is_dir", vec![s("f.txt")]),
            ("fs.append", vec![s("f.txt"), s("!")]),
            ("fs.read", vec![s("f.txt")]),
            ("fs.stat", vec![s("f.txt")]),
            ("fs.append", vec![s("new.txt"), s("1")]),
            ("fs.read", vec![s("new.txt")]),
            ("fs.mkdir", vec![s("d")]),
            ("fs.stat", vec![s("d")]),
            ("fs.is_dir", vec![s("d")]),
            ("fs.is_file", vec![s("d")]),
            ("fs.append", vec![s("d"), s("x")]),
            ("fs.append", vec![s("nodir/x"), s("x")]),
            ("fs.append", vec![s("../x"), s("x")]),
            ("fs.touch", vec![s("f.txt")]),
            ("fs.stat", vec![s("f.txt")]),
        ]),
        ("text tools", vec![
            ("fs.write", vec![s("t.txt"), s("a,1\nb,2\nc,3\nd,4\n")]),
            ("text.head", vec![s("t.txt"), Value::Int(2)]),
            ("text.tail", vec![s("t.txt"), Value::Int(1)]),
            ("text.wc", vec![s("t.txt")]),
            ("text.cut", vec![s("t.txt"), s(","), Value::Int(2)]),
            ("text.head", vec![s("missing"), Value::Int(1)]),
            ("fs.mkdir", vec![s("g/h"), b(true)]),
            ("fs.write", vec![s("g/one"), s("needle\nhay\n")]),
            ("fs.write", vec![s("g/h/two"), s("hay\nneedle here\n")]),
            ("text.grep", vec![s("needle"), s("g/one")]),
            ("text.grep", vec![s("needle"), s("g"), b(true)]),
            ("text.grep", vec![s("NEEDLE"), s("g"), b(true), b(true)]),
            ("text.grep", vec![s("needle"), s("g")]),
            ("text.grep", vec![s("x"), s("missing")]),
        ]),
    ]
}

#[test]
fn memory_and_disk_give_the_same_answers() {
    let mut differ = Vec::new();
    let mut steps = 0;
    for (name, script) in scripts() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap().to_string_lossy().into_owned();
        let disk = HostCapsBuilder::new().fs(true).fs_root(&root).build();
        let memory = HostCapsBuilder::new().fs_memory().build();
        for (cap, args) in &script {
            steps += 1;
            let on_disk = run(&disk, &root, cap, args);
            let in_memory = run(&memory, "", cap, args);
            if std::env::var_os("PARITY_SHOW").is_some() {
                eprintln!("[{name}] {cap} -> {in_memory}");
            }
            if on_disk != in_memory {
                differ.push(format!("[{name}] {cap}{args:?}\n    disk:   {on_disk}\n    memory: {in_memory}"));
            }
        }
    }
    assert!(steps > 120, "{steps} steps");
    assert!(differ.is_empty(), "{} of {steps} steps differ:\n{}", differ.len(), differ.join("\n"));
}

/// The in-memory tree belongs to one registry: a fresh one starts empty.
#[test]
fn each_registry_has_its_own_tree() {
    let a = HostCapsBuilder::new().fs_memory().build();
    run(&a, "", "fs.write", &[s("x"), s("1")]);
    assert_eq!(run(&a, "", "fs.read", &[s("x")]), "ok 1");
    let b = HostCapsBuilder::new().fs_memory().build();
    assert_eq!(run(&b, "", "fs.exists", &[s("x")]), "ok 0");
}
