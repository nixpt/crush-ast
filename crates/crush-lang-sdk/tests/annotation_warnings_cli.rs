
/// CRUSH-370: `--invariant-runtime` emitted a call to a capability nobody
/// provides; it is now a clear error rather than a program that dies later.
#[test]
fn invariant_runtime_flag_is_a_clear_error() {
    let (d, p) = file(CLEAN);
    let out = d.path().join("p.cvm1");
    let o = Command::new(bin("crushc")).args([&p, "--invariant-runtime", "-o", out.to_str().unwrap()]).output().unwrap();
    assert!(!o.status.success());
    assert!(err(&o).contains("--invariant-runtime was removed") && err(&o).contains("invariant.evaluate"), "{}", err(&o));
    assert!(!out.exists());
}
