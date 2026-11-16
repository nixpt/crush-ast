//! Owner ids don't depend on the working directory (V3b, #522): lowering
//! reads no filesystem and no cwd, so the same source compiles to the same
//! owners in any checkout. Its own test binary: it changes the process's
//! working directory, which no other test may see.

use serde_json::json;

#[test]
fn lowering_is_the_same_in_any_working_directory() {
    let src = "fn helper(n) {\n    return n\n}\nfn other(n) {\n    return n\n}\nfn main() {\n    io.print(helper(1) + other(2))\n}\n";
    let lowered = || {
        let mut program = crush_frontend::parse_source(src).unwrap();
        for (f, dir) in [("helper", "."), ("other", "..")] {
            program
                .functions
                .get_mut(f)
                .unwrap()
                .meta
                .insert("package".to_string(), json!(format!("dir:{dir}")));
        }
        let module = crush_frontend::cosm_lower::lower(&mut program).unwrap();
        let (_, cosm) = crush_frontend::compile_cast_with_cosm(program.clone()).unwrap();
        (serde_json::to_value(&program).unwrap(), module, cosm)
    };
    let start = std::env::current_dir().unwrap();
    let here = lowered();
    let tmp = std::env::temp_dir();
    std::env::set_current_dir(&tmp).unwrap();
    let there = lowered();
    std::env::set_current_dir("/").unwrap();
    let root = lowered();
    std::env::set_current_dir(start).unwrap();
    assert_eq!(here, there);
    assert_eq!(here, root);
    let cosm = here.2.unwrap();
    let mut ids: Vec<&str> = ["helper", "other"]
        .iter()
        .map(|f| cosm.function_package(f).unwrap())
        .collect();
    ids.sort();
    assert_eq!(ids, ["dir:dir", "dir:dir#2"]);
}
