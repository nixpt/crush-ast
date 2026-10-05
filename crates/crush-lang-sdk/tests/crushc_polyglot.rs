//! #70 / CRUSH-130: `crushc x.crush -o x.cvm1` must run the same polyglot
//! marshaling pass as `crush-run run x.crush`. It used to skip it, so a
//! `@python` block's output variable was undefined ("Undefined variable:
//! result" at type-check) and inputs weren't passed in.

use std::process::Command;

fn bin(name: &str) -> String {
    match name {
        "crushc" => option_env!("CARGO_BIN_EXE_crushc")
            .unwrap_or("crushc")
            .to_string(),
        _ => option_env!("CARGO_BIN_EXE_crush-run")
            .unwrap_or("crush-run")
            .to_string(),
    }
}

fn output(cmd: &mut Command) -> (String, bool) {
    let out = cmd.output().expect("spawn");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (text, out.status.success())
}

#[test]
fn crushc_cvm1_marshals_python_blocks_like_crush_run() {
    let dir = std::env::temp_dir().join(format!("crushc_polyglot_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("p.crush");
    std::fs::write(
        &src,
        "fn main() {\n  let base = 5\n  @python { result = base * 2 }\n  print(\"r=\" + result)\n}\n",
    )
    .unwrap();

    let (direct, ok) = output(
        Command::new(bin("crush-run"))
            .arg("run")
            .arg(&src)
            .arg("--polyglot"),
    );
    assert!(ok && direct.contains("r=10"), "crush-run x.crush: {direct}");

    for optimize in [false, true] {
        let blob = dir.join(format!("p{optimize}.cvm1"));
        let mut crushc = Command::new(bin("crushc"));
        if optimize {
            crushc.arg("--optimize");
        }
        let (compiled, ok) = output(crushc.arg(&src).arg("-o").arg(&blob));
        assert!(ok, "crushc (optimize={optimize}): {compiled}");
        let (ran, ok) = output(
            Command::new(bin("crush-run"))
                .arg("run")
                .arg(&blob)
                .arg("--polyglot"),
        );
        assert!(
            ok && ran.contains("r=10"),
            "crushc → .cvm1 (optimize={optimize}): {ran}"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}
