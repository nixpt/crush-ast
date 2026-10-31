//! #256: a `for` loop variable is a local, so `s.area()` on it is a method
//! call — not a capability named `s.area` ("not available in this host").

use crush_vm::{Quotas, run};

fn run_source(source: &str) -> String {
    let casm = crush_frontend::compile_crush_source(source).expect("compile");
    let perms = &casm.manifest.permissions;
    assert!(
        !perms.iter().any(|p| p == "s.area" || p == "a.push"),
        "a loop variable's method became a capability: {perms:?}"
    );
    let program = crush_lang_sdk::compile::casm_to_vm(&casm).expect("lower");
    run(&program, &Quotas::default()).expect("run").output
}

#[test]
fn methods_on_a_for_loop_variable() {
    let out = run_source(
        "struct Sq { side: Int }\nimpl Sq {\n    fn area(self) {\n        return self.side * self.side\n    }\n}\nfor s in [Sq { side: 2 }] {\n    print(s.area())\n}\nfor a in [[1]] {\n    a.push(2)\n    print(a)\n}\nfn main2() {\n    for s in [Sq { side: 3 }] {\n        print(s.area())\n    }\n    return 0\n}\nmain2()\n",
    );
    assert_eq!(out, "4\n[1, 2]\n9\n");
}
