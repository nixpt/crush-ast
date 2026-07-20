//! Helper binary for running a JIT-compiled Crush program in a subprocess.
//!
//! The JIT generates native machine code directly into executable memory.
//! If the JIT compiler generates invalid code (e.g. SIGILL), the process
//! crashes. This binary isolates JIT execution so the differential harness
//! can safely include JIT comparisons without risking the test suite.
//!
//! Usage: jit-runner <path-to-lowered-program.json>

use crush_jit::JitEngine;
use crush_vm::fastvm::{FastYield, LoweredProgram};
use crush_vm::memory::Arena;
use crush_vm::value::RuntimeValue;
use std::env;
use std::fs;

fn main() {
    let path = env::args()
        .nth(1)
        .expect("usage: jit-runner <path-to-lowered-program.json>");

    let json = match fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("jit-runner: failed to read '{}': {e}", path);
            std::process::exit(1);
        }
    };

    let program: LoweredProgram = match serde_json::from_str(&json) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("jit-runner: failed to parse LoweredProgram: {e}");
            std::process::exit(1);
        }
    };

    let engine = match JitEngine::new() {
        Ok(e) => e,
        Err(e) => {
            eprintln!("jit-runner: JitEngine::new failed: {e}");
            std::process::exit(1);
        }
    };

    // Use run_with_ctx to control the arena so we can resolve Ref values
    // after execution. The JIT stores strings as Ref(idx) into the arena.
    let mut arena = Arena::new();
    let mut ctx = crush_jit::runtime::JitContext::new();
    ctx.budget = u64::MAX;

    match engine.run_with_ctx(&program, &mut ctx, &mut arena) {
        Ok(FastYield::Finished(Some(v))) => print_value(&v, &arena),
        Ok(FastYield::Finished(None)) => println!("null:null"),
        Ok(FastYield::Value(v)) => print_value(&v, &arena),
        Ok(FastYield::Error(e)) => {
            eprintln!("jit-runner: JIT error: {e:?}");
            std::process::exit(1);
        }
        Ok(FastYield::BudgetExhausted) => {
            eprintln!("jit-runner: budget exhausted");
            std::process::exit(1);
        }
        Ok(FastYield::Yielded) => {
            eprintln!("jit-runner: unexpected yield");
            std::process::exit(1);
        }
        Ok(FastYield::Request(_)) => {
            eprintln!("jit-runner: host request (unserviced)");
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("jit-runner: execution failed: {e}");
            std::process::exit(1);
        }
    }
}

fn print_value(v: &RuntimeValue, arena: &Arena) {
    match v {
        RuntimeValue::Int(i) => println!("int:{}", i),
        RuntimeValue::Float(f) => {
            if f.is_finite() && f.fract() == 0.0 {
                println!("float:{:.1}", f);
            } else {
                println!("float:{}", f);
            }
        }
        RuntimeValue::Bool(b) => println!("bool:{}", b),
        RuntimeValue::Null => println!("null:null"),
        RuntimeValue::String(s) => println!("str:{}", s),
        RuntimeValue::Ref(idx) => {
            // Resolve arena ref — JIT stores strings/objects as Ref pointers.
            match arena.get(*idx) {
                Some(crush_vm::memory::Object::Str(s)) => println!("str:{}", s),
                _ => {
                    eprintln!(
                        "jit-runner: unsupported arena ref {}: {:?}",
                        idx,
                        arena.get(*idx)
                    );
                    std::process::exit(1);
                }
            }
        }
    }
}
