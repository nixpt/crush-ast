//! Shared Rust parity policy for SDK/CVM1 and both AOT backends.
//! Every exception names its fixing ticket and pins the observed failure.
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug)]
pub enum Engine {
    Vm,
    Rust,
    C,
}

pub enum Outcome {
    Output(String),
    Loud(String),
}

#[derive(Clone, Copy)]
pub enum Expected {
    Exact,
    Loud {
        ticket: &'static str,
        diagnostic: &'static str,
    },
    Silent {
        ticket: &'static str,
        output: &'static str,
    },
}

const ALL_ENGINES: &[(&str, Expected)] = &[
    (
        "debug_dynamic",
        Expected::Loud {
            ticket: "CRUSH-xps-9",
            diagnostic: "Debug requires a proven typed literal",
        },
    ),
    (
        "float_display",
        Expected::Loud {
            ticket: "CRUSH-xps-9",
            diagnostic: "Debug requires a proven typed literal",
        },
    ),
    (
        "format_repeated_effect",
        Expected::Loud {
            ticket: "CRUSH-xps-9",
            diagnostic: "effectful repeated/reordered format arguments",
        },
    ),
    (
        "format_reordered_effect",
        Expected::Loud {
            ticket: "CRUSH-xps-9",
            diagnostic: "effectful repeated/reordered format arguments",
        },
    ),
    (
        "format_owned_literal",
        Expected::Loud {
            ticket: "CRUSH-xps-7",
            diagnostic: "by-value transfer",
        },
    ),
    (
        "format_owned_empty",
        Expected::Loud {
            ticket: "CRUSH-xps-7",
            diagnostic: "by-value transfer",
        },
    ),
    (
        "numeric_unsigned_large",
        Expected::Loud {
            ticket: "CRUSH-xps-8",
            diagnostic: "not representable in Crush Int",
        },
    ),
    (
        "numeric_unsigned_overflow",
        Expected::Loud {
            ticket: "CRUSH-xps-8",
            diagnostic: "bounded unsigned arithmetic needs a proven canonical integer",
        },
    ),
    (
        "numeric_pointer_unbounded",
        Expected::Loud {
            ticket: "CRUSH-xps-8",
            diagnostic: "usize cast needs a nonnegative portable bound",
        },
    ),
    (
        "numeric_length_mutated",
        Expected::Loud {
            ticket: "CRUSH-xps-8",
            diagnostic: "length source needs proven unmodified sequence",
        },
    ),
    (
        "numeric_length_nul",
        Expected::Loud {
            ticket: "CRUSH-xps-8",
            diagnostic: "length source needs proven unmodified sequence",
        },
    ),
    (
        "numeric_cast_alias_call",
        Expected::Loud {
            ticket: "CRUSH-xps-8",
            diagnostic: "indirect calls need numeric argument provenance",
        },
    ),
    (
        "numeric_cast_recursive",
        Expected::Loud {
            ticket: "CRUSH-xps-8",
            diagnostic: "proven canonical integer",
        },
    ),
    (
        "numeric_cast_reserved",
        Expected::Loud {
            ticket: "CRUSH-xps-8",
            diagnostic: "reserved Rust numeric compiler helper",
        },
    ),
    (
        "value_array_assignment",
        Expected::Loud {
            ticket: "CRUSH-xps-7",
            diagnostic: "indexed/field assignment",
        },
    ),
    (
        "value_string_transfer",
        Expected::Loud {
            ticket: "CRUSH-xps-7",
            diagnostic: "by-value transfer",
        },
    ),
    (
        "value_array_transfer",
        Expected::Loud {
            ticket: "CRUSH-xps-7",
            diagnostic: "by-value transfer",
        },
    ),
    (
        "value_store_view",
        Expected::Loud {
            ticket: "CRUSH-xps-7",
            diagnostic: "storing borrowed views",
        },
    ),
    (
        "value_capture_assignment",
        Expected::Loud {
            ticket: "CRUSH-xps-7",
            diagnostic: "mutable capture semantics",
        },
    ),
    (
        "value_capture_owned",
        Expected::Loud {
            ticket: "CRUSH-xps-7",
            diagnostic: "closure ownership analysis",
        },
    ),
    (
        "value_borrow_parameter",
        Expected::Loud {
            ticket: "CRUSH-xps-7",
            diagnostic: "borrow checking",
        },
    ),
    (
        "value_nested_clone",
        Expected::Loud {
            ticket: "CRUSH-xps-7",
            diagnostic: "nested/unknown/reference values",
        },
    ),
    (
        "value_owned_return",
        Expected::Loud {
            ticket: "CRUSH-xps-7",
            diagnostic: "by-value transfer",
        },
    ),
    (
        "value_move_argument",
        Expected::Loud {
            ticket: "CRUSH-xps-7",
            diagnostic: "by-value transfer",
        },
    ),
    (
        "value_move_assignment",
        Expected::Loud {
            ticket: "CRUSH-xps-7",
            diagnostic: "by-value transfer",
        },
    ),
    (
        "value_pop_option",
        Expected::Loud {
            ticket: "CRUSH-xps-7",
            diagnostic: "Vec.pop needs Option value semantics",
        },
    ),
    (
        "binding_nested_function",
        Expected::Loud {
            ticket: "CRUSH-xps-6",
            diagnostic: "nested function at 2:5",
        },
    ),
    (
        "audit_string_clone",
        Expected::Loud {
            ticket: "CRUSH-xps-7",
            diagnostic: ".push_str()",
        },
    ),
    (
        "numeric_cast_mutated",
        Expected::Loud {
            ticket: "CRUSH-xps-8",
            diagnostic: "proven canonical integer",
        },
    ),
    (
        "numeric_cast_overflow_producer",
        Expected::Loud {
            ticket: "CRUSH-xps-8",
            diagnostic: "proven canonical integer",
        },
    ),
    (
        "numeric_cast_float_producer",
        Expected::Loud {
            ticket: "CRUSH-xps-8",
            diagnostic: "proven canonical integer",
        },
    ),
    (
        "numeric_cast_bad_argument",
        Expected::Loud {
            ticket: "CRUSH-xps-8",
            diagnostic: "integer call argument needs a canonical value proof",
        },
    ),
    (
        "numeric_cast_capture_write",
        Expected::Loud {
            ticket: "CRUSH-xps-8",
            diagnostic: "proven canonical integer",
        },
    ),
    (
        "numeric_cast_closure",
        Expected::Loud {
            ticket: "CRUSH-xps-8",
            diagnostic: "closure casts need numeric capture/call analysis",
        },
    ),
    (
        "numeric_cast_float",
        Expected::Loud {
            ticket: "CRUSH-xps-8",
            diagnostic: "noninteger Rust cast operand",
        },
    ),
    (
        "numeric_cast_char",
        Expected::Loud {
            ticket: "CRUSH-xps-8",
            diagnostic: "noninteger Rust cast operand",
        },
    ),
    (
        "numeric_literal_f32",
        Expected::Loud {
            ticket: "CRUSH-xps-8",
            diagnostic: "unsupported Rust float suffix `f32`",
        },
    ),
    (
        "audit_integer_overflow",
        Expected::Silent {
            ticket: "CRUSH-xps-8",
            output: "260\n",
        },
    ),
    (
        "audit_struct_value",
        Expected::Loud {
            ticket: "CRUSH-xps-7 / COSM K3",
            diagnostic: "unsupported Rust top-level item",
        },
    ),
    (
        "audit_value_block",
        Expected::Loud {
            ticket: "CRUSH-xps-12",
            diagnostic: "block expression at 2:13",
        },
    ),
    (
        "audit_move_closure",
        Expected::Loud {
            ticket: "CRUSH-xps-11",
            diagnostic: "move closure",
        },
    ),
    (
        "audit_destructured_closure",
        Expected::Loud {
            ticket: "CRUSH-xps-11",
            diagnostic: "closure parameter",
        },
    ),
    (
        "audit_import",
        Expected::Loud {
            ticket: "CRUSH-xps-3",
            diagnostic: "Rust imports and crate APIs",
        },
    ),
    (
        "audit_format_precision",
        Expected::Loud {
            ticket: "CRUSH-xps-5",
            diagnostic: "format spec",
        },
    ),
    (
        "audit_range_pattern",
        Expected::Loud {
            ticket: "CRUSH-xps-5",
            diagnostic: "range patterns",
        },
    ),
];
const AOT_ONLY: &[(&str, Expected)] = &[
    (
        "capability_namespace_bindings",
        Expected::Loud {
            ticket: "CRUSH-xps-13",
            diagnostic: "cap_call 'math.pow'",
        },
    ),
    (
        "binding_intrinsic_names",
        Expected::Loud {
            ticket: "CRUSH-251",
            diagnostic: "make_closure",
        },
    ),
    (
        "binding_captures",
        Expected::Loud {
            ticket: "CRUSH-251",
            diagnostic: "make_closure",
        },
    ),
    (
        "int_arith",
        Expected::Loud {
            ticket: "CRUSH-xps-13",
            diagnostic: "cap_call 'math.pow'",
        },
    ),
    (
        "vecmut",
        Expected::Loud {
            ticket: "CRUSH-xps-13",
            diagnostic: "cap_call 'str.to_lower'",
        },
    ),
    (
        "closures",
        Expected::Loud {
            ticket: "CRUSH-251",
            diagnostic: "make_closure",
        },
    ),
    (
        "closure_blocks",
        Expected::Loud {
            ticket: "CRUSH-251",
            diagnostic: "make_closure",
        },
    ),
    (
        "typed_closure_captures",
        Expected::Loud {
            ticket: "CRUSH-251",
            diagnostic: "make_closure",
        },
    ),
];

pub fn expectation(name: &str, engine: Engine) -> Expected {
    let aot = if matches!(engine, Engine::Vm) {
        &[][..]
    } else {
        AOT_ONLY
    };
    ALL_ENGINES
        .iter()
        .chain(aot)
        .find(|(n, _)| *n == name)
        .map_or(Expected::Exact, |(_, policy)| *policy)
}

pub fn corpus() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let dir = root.join("crush-lang-sdk/tests/rustc_corpus");
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .expect("Rust corpus")
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "rs"))
        .collect();
    files.sort();
    for (name, _) in ALL_ENGINES.iter().chain(AOT_ONLY) {
        assert!(
            files.iter().any(|p| p.file_stem().unwrap() == *name),
            "orphan Rust parity exception: {name}"
        );
    }
    assert!(files.len() >= 58, "Rust corpus shrank: {}", files.len());
    files
}

/// Returns an actionable failure, including stale exception entries.
pub fn check(name: &str, engine: Engine, expected: &str, got: Outcome) -> Result<(), String> {
    let policy = expectation(name, engine);
    match (policy, got) {
        (Expected::Exact, Outcome::Output(output)) if output == expected => Ok(()),
        (
            Expected::Loud { ticket, .. } | Expected::Silent { ticket, .. },
            Outcome::Output(output),
        ) if output == expected => Err(format!(
            "{name}: now exact on {engine:?} — remove it from the exception list ({ticket})"
        )),
        (Expected::Loud { ticket, diagnostic }, Outcome::Loud(error))
            if error.contains(diagnostic) =>
        {
            eprintln!("{engine:?} {name}: expected rejection ({ticket}): {error}");
            Ok(())
        }
        (Expected::Silent { ticket, output }, Outcome::Output(got)) if got == output => {
            eprintln!("{engine:?} {name}: KNOWN SILENT DIVERGENCE ({ticket}): {got:?}");
            Ok(())
        }
        (_, Outcome::Output(got)) => Err(format!(
            "{name}: UNLISTED DIVERGENCE on {engine:?}\n  rustc: {expected:?}\n  crush: {got:?}"
        )),
        (_, Outcome::Loud(error)) => {
            Err(format!("{name}: unexpected failure on {engine:?}: {error}"))
        }
    }
}

/// Use files for capture so a child cannot deadlock on a full stdout pipe.
/// Infrastructure failures/timeouts fail the test; they never count as an
/// allowed language rejection.
pub fn run_bounded(command: &mut Command) -> Output {
    let stdout = tempfile::tempfile().unwrap();
    let stderr = tempfile::tempfile().unwrap();
    let mut child = command
        .stdin(Stdio::null())
        .stdout(stdout.try_clone().unwrap())
        .stderr(stderr.try_clone().unwrap())
        .spawn()
        .expect("spawn parity subprocess");
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if start.elapsed() > Duration::from_secs(60) {
            let _ = child.kill();
            let _ = child.wait();
            panic!("Rust parity subprocess timed out: {command:?}");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    use std::io::{Read, Seek, SeekFrom};
    fn read(mut file: std::fs::File) -> Vec<u8> {
        file.seek(SeekFrom::Start(0)).unwrap();
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).unwrap();
        bytes
    }
    Output {
        status,
        stdout: read(stdout),
        stderr: read(stderr),
    }
}

pub fn assert_rustc_freshness() {
    let temp = tempfile::tempdir().unwrap();
    for path in corpus() {
        let binary = temp.path().join(path.file_stem().unwrap());
        let compile = run_bounded(
            Command::new("rustc")
                .arg("--edition=2021")
                .arg("-O")
                .arg("-o")
                .arg(&binary)
                .arg(&path),
        );
        assert!(
            compile.status.success(),
            "rustc failed on {}: {}",
            path.display(),
            String::from_utf8_lossy(&compile.stderr)
        );
        let output = run_bounded(&mut Command::new(&binary));
        assert!(
            output.status.success(),
            "rustc fixture failed: {}",
            path.display()
        );
        assert!(
            output.stderr.is_empty(),
            "unexpected fixture stderr: {}",
            path.display()
        );
        let expected = std::fs::read_to_string(path.with_extension("out")).unwrap();
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            expected,
            "{}.out is stale",
            path.display()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unlisted_output_and_failures_fail_the_gate() {
        assert!(
            check(
                "new_fixture",
                Engine::Vm,
                "42\n",
                Outcome::Output("41\n".into())
            )
            .is_err()
        );
        assert!(
            check(
                "new_fixture",
                Engine::Vm,
                "42\n",
                Outcome::Loud("unsupported".into())
            )
            .is_err()
        );
    }

    #[test]
    fn exact_comparison_preserves_whitespace() {
        assert!(check("hello", Engine::Vm, "42\n", Outcome::Output("42 \n".into())).is_err());
        assert!(
            check(
                "hello",
                Engine::Vm,
                "42\n",
                Outcome::Output("42\n\n".into())
            )
            .is_err()
        );
    }

    #[test]
    fn fixed_loud_and_silent_entries_must_be_removed() {
        for (name, expected) in [
            ("audit_value_block", "42\n"),
            ("audit_integer_overflow", "4\n"),
        ] {
            let error =
                check(name, Engine::Vm, expected, Outcome::Output(expected.into())).unwrap_err();
            assert!(
                error.contains("now exact") && error.contains("remove it"),
                "{error}"
            );
        }
    }

    #[test]
    fn exceptions_only_accept_the_pinned_failure() {
        assert!(
            check(
                "audit_vec_clone",
                Engine::Vm,
                "3 4\n",
                Outcome::Output("5 5\n".into())
            )
            .is_err()
        );
        assert!(
            check(
                "audit_value_block",
                Engine::Vm,
                "42\n",
                Outcome::Loud("compiler crashed".into())
            )
            .is_err()
        );
        assert!(
            check(
                "audit_value_block",
                Engine::Vm,
                "42\n",
                Outcome::Output("0\n".into())
            )
            .is_err()
        );
    }
}
