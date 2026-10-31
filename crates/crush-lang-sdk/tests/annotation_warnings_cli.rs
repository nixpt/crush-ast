//! CRUSH-369: `crushc` and `crush-run` print the annotation checks
//! (W-WIP-001, W-TMP-001, ...) as warnings; `crushc --deny-warnings` fails the
//! build on them.

use std::process::{Command, Output, Stdio};

fn bin(n: &str) -> String {
    match n {
        "crushc" => option_env!("CARGO_BIN_EXE_crushc").unwrap_or("crushc").to_string(),
        _ => option_env!("CARGO_BIN_EXE_crush-run").unwrap_or("crush-run").to_string(),
    }
}
fn err(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).to_string()
}

const WIP: &str = "@wip {\n    intent: \"in progress\"\n    todo: [emit, tests]\n}\nfn main() { print(\"ran\") }\n";
const STALE_TMP: &str = "@temporary {\n    reason: \"old hack\"\n    added: \"2020-01-01\"\n}\nfn main() { print(\"ran\") }\n";
const CLEAN: &str = "fn main() { print(\"ran\") }\n";

fn file(src: &str) -> (tempfile::TempDir, String) {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("p.crush");
    std::fs::write(&p, src).unwrap();
    let s = p.to_str().unwrap().to_string();
    (d, s)
}

#[test]
fn crushc_warns_but_still_builds() {
    for (src, code) in [(WIP, "W-WIP-001"), (STALE_TMP, "W-TMP-001")] {
        let (d, p) = file(src);
        let out = d.path().join("p.cvm1");
        let o = Command::new(bin("crushc")).args([&p, "-o", out.to_str().unwrap()]).output().unwrap();
        assert!(o.status.success(), "{}", err(&o));
        assert!(err(&o).contains(code), "{}", err(&o));
        assert!(out.exists());
    }
}

#[test]
fn crushc_json_records_follow_the_diagnostic_schema() {
    let (_d, p) = file(WIP);
    let o = Command::new(bin("crushc")).args(["--message-format", "json", "--check", &p]).output().unwrap();
    assert!(o.status.success(), "{}", err(&o));
    let line = err(&o).lines().find(|l| l.contains("W-WIP-001")).expect("record").to_string();
    let v: serde_json::Value = serde_json::from_str(&line).unwrap();
    assert_eq!(v["code"], "W-WIP-001");
    assert_eq!(v["level"], "warning");
    assert!(v["file"].as_str().unwrap().ends_with("p.crush"));
    assert!(v["message"].as_str().unwrap().contains("2 todo"));
    for k in ["line", "col", "hint"] {
        assert!(v.get(k).is_some(), "{k} present: {line}");
    }
}

#[test]
fn deny_warnings_fails_the_build_and_clean_programs_pass() {
    let (d, p) = file(WIP);
    let out = d.path().join("p.cvm1");
    let o = Command::new(bin("crushc")).args([&p, "--deny-warnings", "-o", out.to_str().unwrap()]).output().unwrap();
    assert!(!o.status.success());
    assert!(err(&o).contains("W-WIP-001") && err(&o).contains("--deny-warnings"), "{}", err(&o));
    assert!(!out.exists(), "no artifact");
    let (d, p) = file(CLEAN);
    let out = d.path().join("p.cvm1");
    let o = Command::new(bin("crushc")).args([&p, "--deny-warnings", "-o", out.to_str().unwrap()]).output().unwrap();
    assert!(o.status.success() && err(&o).is_empty(), "{}", err(&o));
}

#[test]
fn crush_run_warns_on_stderr_and_still_runs() {
    let (_d, p) = file(WIP);
    let o = Command::new(bin("crush-run")).args(["run", &p]).stdin(Stdio::null()).output().unwrap();
    assert!(o.status.success(), "{}", err(&o));
    assert!(String::from_utf8_lossy(&o.stdout).contains("ran"));
    assert!(err(&o).contains("W-WIP-001"), "{}", err(&o));
    // JSON mode: a record on stderr, program output untouched on stdout.
    let o = Command::new(bin("crush-run")).args(["run", "--message-format", "json", &p]).stdin(Stdio::null()).output().unwrap();
    assert!(err(&o).lines().any(|l| l.contains("\"code\":\"W-WIP-001\"") && l.contains("\"level\":\"warning\"")), "{}", err(&o));
    let (_d, p) = file(CLEAN);
    let o = Command::new(bin("crush-run")).args(["run", &p]).stdin(Stdio::null()).output().unwrap();
    assert!(!err(&o).contains("W-"), "{}", err(&o));
}
