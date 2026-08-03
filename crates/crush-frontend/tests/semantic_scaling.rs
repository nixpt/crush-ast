//! Semantic-analysis scaling measurement (CRUSH-71).
//!
//! Not a pass/fail test — prints median wall-clock for
//! `SemanticAnalyzer::check` over synthetic programs whose call-graph depth
//! is what the old whole-program fixed point was quadratic-ish in.
//! Run explicitly:
//!
//! ```sh
//! cargo test -p crush-frontend --test semantic_scaling --release -- --ignored --nocapture
//! ```

use crush_frontend::semantics::SemanticAnalyzer;
use std::time::Instant;

/// A linear call chain of `n` functions: chain_i calls chain_{i-1}.
///
/// `arith` shape (`return callee(x) + 1`): the lenient Null handling in
/// binary ops coalesces `Null + Int` to `Int`, so even unordered inference
/// converges in one pass — this measures the constant-factor win only.
///
/// `forward` shape (`return callee(x)`): the inferred type is exactly the
/// callee's currently-recorded type, so each whole-program pass propagates
/// types one chain level. The old capped fixed point did 12 full walks and
/// still left types unresolved past depth ~12; SCC ordering resolves the
/// whole chain in one walk.
fn chain_program(n: usize, forward: bool) -> String {
    let mut src = String::from("fn chain0000(x: Int) { return x + 42 }\n");
    for i in 1..n {
        if forward {
            src.push_str(&format!(
                "fn chain{:04}(x: Int) {{ return chain{:04}(x) }}\n",
                i,
                i - 1
            ));
        } else {
            src.push_str(&format!(
                "fn chain{:04}(x: Int) {{ return chain{:04}(x) + 1 }}\n",
                i,
                i - 1
            ));
        }
    }
    src.push_str(&format!("fn main() {{\n    chain{:04}(1)\n    return 0\n}}\n", n - 1));
    src
}

fn median_check_us(source: &str, runs: usize) -> u128 {
    let program = crush_frontend::parse_source(source).expect("scaling fixture parses");
    let mut samples: Vec<u128> = (0..runs)
        .map(|_| {
            let mut sema = SemanticAnalyzer::new();
            let start = Instant::now();
            sema.check(&program).expect("scaling fixture type-checks");
            start.elapsed().as_micros()
        })
        .collect();
    samples.sort_unstable();
    samples[samples.len() / 2]
}

#[test]
#[ignore = "measurement harness, run with --ignored --nocapture"]
fn semantic_check_scaling() {
    println!("functions,call_shape,median_check_us");
    for n in [25usize, 100, 300] {
        let us = median_check_us(&chain_program(n, false), 30);
        println!("{n},chain_arith,{us}");
    }
    for n in [25usize, 100, 300] {
        let us = median_check_us(&chain_program(n, true), 30);
        println!("{n},chain_forward,{us}");
    }
}
