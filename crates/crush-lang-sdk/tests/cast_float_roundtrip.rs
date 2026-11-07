//! CRUSH-373: a float literal keeps its exact bits on the `crush-run` route,
//! from CAST JSON on disk through to the running program.

use std::process::Command;

/// Deterministic doubles across a range of magnitudes.
fn sample_floats(n: usize) -> Vec<f64> {
    let mut state = 0x9E37_79B9_7F4A_7C15_u64;
    (0..n)
        .map(|i| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let unit = (state >> 11) as f64 / (1u64 << 53) as f64;
            unit * 10f64.powi((i % 8) as i32 - 3)
        })
        .collect()
}

fn print_program(values: &[f64]) -> String {
    let body: Vec<serde_json::Value> = values
        .iter()
        .map(|v| {
            serde_json::json!({
                "type": "ExprStmt",
                "expr": {
                    "type": "CapabilityCall",
                    "name": "py.print",
                    "args": [{ "type": "FloatLiteral", "value": v, "meta": {} }],
                    "meta": {},
                },
                "meta": {},
            })
        })
        .collect();
    serde_json::json!({
        "cast_version": "0.1.0",
        "entry": "main",
        "lang": "python",
        "functions": { "main": { "params": [], "body": body, "meta": {} } },
    })
    .to_string()
}

#[test]
fn crush_run_keeps_float_literals_exact() {
    let values = sample_floats(1000);
    let dir = tempfile::tempdir().unwrap();
    let cast = dir.path().join("floats.cast.json");
    std::fs::write(&cast, print_program(&values)).unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_crush-run"))
        .args(["run", cast.to_str().unwrap()])
        .output()
        .expect("run crush-run");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let printed: Vec<f64> = stdout.lines().map(|l| l.parse().unwrap()).collect();
    assert_eq!(printed.len(), values.len());
    let changed: Vec<(f64, f64)> = values
        .iter()
        .zip(&printed)
        .filter(|(a, b)| a.to_bits() != b.to_bits())
        .map(|(a, b)| (*a, *b))
        .collect();
    assert!(
        changed.is_empty(),
        "{} of {} literals changed, first: {:?}",
        changed.len(),
        values.len(),
        changed.first()
    );
}
