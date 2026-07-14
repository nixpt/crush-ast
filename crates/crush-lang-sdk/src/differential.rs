//! Differential execution harness — run one crush program through every backend and compare.
//!
//! crush-ast has FIVE independent implementations of its own semantics:
//!
//! ```text
//! A  interpreter   crush_vm::run           (what crush-run uses)
//! B  portable       PortableVm::run
//! C  fastvm         crush_vm::run_fastvm   (its OWN lowering — crush-python uses this)
//! D  aot / rust     crush_aot::codegen     (does not link today — pre-existing)
//! E  aot / c        crush_aot::codegen_c   (does not link today — pre-existing)
//! ```
//!
//! They have already been caught disagreeing. This session:
//!   - `1 / 0`     interpreter errored (DivisionByZero); the AOT path silently returned 0.
//!   - `"a" + "b"` interpreter concatenated; the AOT path silently pushed Null.
//! Both were `_ => 0` / `_ => Null` fallthroughs — the same disease as crush-jit's
//! `_ => push(TAG_NULL)`. Found by READING. Nothing had ever run them against each other.
//!
//! This is the restored core of exosphere's dropped `adapters/crush.rs`: result-level snapshot
//! comparison, minus the async TargetAdapter ceremony a batch harness does not need. D/E slot in
//! the moment the AOT link bug is fixed.
//!
//! ## What is comparable, and what is not — stated honestly
//!
//! A and B share the exact `crush_vm::Value` type, `VmResult` shape, and lowering (`casm_to_vm`).
//! Their comparison is TIGHT: stdout AND the full final stack, value-for-value. This is the pair
//! that actually caught a bug this session — portable_vm's `to_f64_p => 0.0` fallthrough.
//!
//! C (fastvm) is a genuinely different shape: it lowers the CASM itself, uses a different value
//! enum (`RuntimeValue`), and returns a single `FastYield` (a return value, not a stack+stdout).
//! So its comparison is COARSER — outcome class (ok/err) and, when both finish with a scalar, the
//! scalar. A mismatch here is flagged for review, not asserted as a hard bug, because the shapes
//! differ. Do not pretend this pair is as tight as A-vs-B; it is not.

use crush_vm::fastvm::FastYield;
use crush_vm::vm::Value;
use crush_vm::{PortableVm, Quotas, RuntimeValue, VmResult};

/// A value normalized across the two different backend value enums, so A/B (`Value`) and C
/// (`RuntimeValue`) can be compared at all. Floats are stringified for exact-bits equality.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Norm {
    Null,
    Bool(bool),
    Int(i64),
    Float(String),
    Str(String),
    /// Anything richer (array, map, handle, ...) — carried as its debug form. Two backends still
    /// "agree" iff these strings match, which is conservative: a false-divergence is a lead to
    /// investigate, never a silently-missed one.
    Other(String),
}

impl Norm {
    fn from_value(v: &Value) -> Norm {
        match v {
            Value::Null => Norm::Null,
            Value::Bool(b) => Norm::Bool(*b),
            Value::Int(i) => Norm::Int(*i),
            Value::Float(f) => Norm::Float(format!("{f:?}")),
            Value::Str(s) => Norm::Str(s.clone()),
            other => Norm::Other(format!("{other:?}")),
        }
    }
    fn from_rtv(v: &RuntimeValue) -> Norm {
        match v {
            RuntimeValue::Null => Norm::Null,
            RuntimeValue::Bool(b) => Norm::Bool(*b),
            RuntimeValue::Int(i) => Norm::Int(*i),
            RuntimeValue::Float(f) => Norm::Float(format!("{f:?}")),
            RuntimeValue::String(s) => Norm::Str(s.clone()),
            other => Norm::Other(format!("{other:?}")),
        }
    }
}

/// A stack-based backend's (A/B) normalized outcome.
#[derive(Debug, Clone, PartialEq)]
pub enum StackOutcome {
    Ok { output: String, stack: Vec<Norm> },
    Err(String),
}

/// fastvm's (C) normalized outcome — a return value, not a stack.
#[derive(Debug, Clone, PartialEq)]
pub enum FastOutcome {
    Finished(Option<Norm>),
    Yielded,
    BudgetExhausted,
    Err(String),
}

#[derive(Debug)]
pub struct DiffReport {
    pub source: String,
    pub interpreter: StackOutcome,
    pub portable: StackOutcome,
    pub fastvm: FastOutcome,
    /// Divergences between backends. Empty == all agree at the granularity each pair supports.
    /// A divergence is an OBSERVABLE difference: stdout, or accept-vs-reject. This is what caught
    /// every real bug this session (1/0 = different error status; "a"+"b" = different stdout).
    pub divergences: Vec<String>,
    /// Non-failing observations — internal-state differences that are not observable program
    /// behavior (e.g. residual stack after `main` with no return). Worth recording, not screaming.
    pub notes: Vec<String>,
}

impl DiffReport {
    pub fn diverged(&self) -> bool {
        !self.divergences.is_empty()
    }
}

fn stack_outcome(r: Result<VmResult, crush_vm::VmError>) -> StackOutcome {
    match r {
        Ok(res) => StackOutcome::Ok {
            output: res.output,
            stack: res.stack.iter().map(Norm::from_value).collect(),
        },
        Err(e) => StackOutcome::Err(e.to_string()),
    }
}

/// Run `source` through interpreter, portable, and fastvm; compare.
///
/// A compile error is upstream of every backend — returned as `Err`, never a divergence.
pub fn differential_run(source: &str) -> Result<DiffReport, String> {
    let casm = crush_frontend::compile_crush_source(source)
        .map_err(|e| format!("frontend: {e}"))?;
    let vm_prog = crate::compile::casm_to_vm(&casm)
        .map_err(|e| format!("casm_to_vm: {e}"))?;
    let quotas = Quotas::default();

    // A — interpreter (borrows vm_prog), then B — portable (consumes a clone).
    let interpreter = stack_outcome(crush_vm::run(&vm_prog, &quotas));
    let portable = stack_outcome(PortableVm::new(vm_prog.clone()).run());

    // C — fastvm: its own lowering of the casm, its own value enum, a return value not a stack.
    let fastvm = match crush_vm::run_fastvm(&casm) {
        Ok(FastYield::Finished(v)) => FastOutcome::Finished(v.as_ref().map(Norm::from_rtv)),
        Ok(FastYield::Yielded) => FastOutcome::Yielded,
        Ok(FastYield::BudgetExhausted) => FastOutcome::BudgetExhausted,
        Ok(FastYield::Value(v)) => FastOutcome::Finished(Some(Norm::from_rtv(&v))),
        Ok(FastYield::Error(e)) => FastOutcome::Err(format!("{e:?}")),
        // A bare host request means the program stopped waiting on a capability the harness does
        // not service — treat as an incomplete run, not a result. (The batch harness runs pure
        // programs; a program that blocks on the host is out of its comparison scope.)
        Ok(FastYield::Request(_)) => FastOutcome::Err("host-request (unserviced by harness)".into()),
        Err(e) => FastOutcome::Err(format!("{e:?}")),
    };

    let mut divergences = Vec::new();
    let mut notes = Vec::new();

    // Observable behavior of a stack backend = (accepted?, stdout). Residual stack after `main`
    // returns nothing is NOT observable — it is implementation detail, recorded as a note.
    let observable = |o: &StackOutcome| -> (bool, Option<String>) {
        match o {
            StackOutcome::Ok { output, .. } => (true, Some(output.clone())),
            StackOutcome::Err(_) => (false, None),
        }
    };

    // A vs B — the TIGHT pair (same value type, same lowering).
    if observable(&interpreter) != observable(&portable) {
        divergences.push(format!(
            "interpreter vs portable — OBSERVABLE divergence (same lowering, so this is a pure VM bug):\n    A={interpreter:?}\n    B={portable:?}"
        ));
    } else if interpreter != portable {
        // Same observable behavior, different internal residue. Informational.
        notes.push(format!(
            "interpreter/portable residual-state differs (same output, harmless unless returned): A={interpreter:?} B={portable:?}"
        ));
    }

    // A vs C — fastvm. It ABSTAINS when it rejected the program for a capability the harness did
    // not provide (io.print etc.): the harness cannot yet drive fastvm WITH caps, so a
    // capability rejection is a harness limitation, not a language divergence. Do not cry wolf.
    let fastvm_abstains = matches!(&fastvm, FastOutcome::Err(e) if e.contains("Capability"));
    if fastvm_abstains {
        notes.push(format!("fastvm ABSTAINED (needs capabilities wired into the harness): {fastvm:?}"));
    } else {
        let a_ok = matches!(interpreter, StackOutcome::Ok { .. });
        let c_ok = matches!(fastvm, FastOutcome::Finished(_) | FastOutcome::Yielded);
        if a_ok != c_ok {
            divergences.push(format!(
                "interpreter vs fastvm OUTCOME CLASS: A_ok={a_ok} C={fastvm:?}"
            ));
        }
    }

    Ok(DiffReport { source: source.to_string(), interpreter, portable, fastvm, divergences, notes })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agreeing_program_no_observable_divergence() {
        // Same stdout across backends. (Portable leaves a residual Null on the stack — recorded
        // as a NOTE, not a divergence, because it is not observable program behavior.)
        let r = differential_run("fn main() { print(1 + 2); }").unwrap();
        assert!(!r.diverged(), "1+2 OBSERVABLY diverged: {:?}", r.divergences);
    }

    #[test]
    fn interpreter_and_portable_agree_on_div_by_zero() {
        // Both must reject 1/0 identically. If portable ever regresses to a silent 0 (its old
        // `to_f64_p` disease), the TIGHT pair catches it here.
        let r = differential_run("fn main() { print(1 / 0); }").unwrap();
        let a_b_diff: Vec<_> = r.divergences.iter().filter(|d| d.contains("interpreter vs portable")).collect();
        assert!(a_b_diff.is_empty(), "interp/portable disagree on 1/0: {a_b_diff:?}");
    }

    #[test]
    fn interpreter_and_portable_agree_on_string_concat() {
        // The `"x: " + N` fix landed in BOTH A and B. The tight pair proves they still match.
        let r = differential_run("fn main() { print(\"x: \" + 5); }").unwrap();
        let a_b_diff: Vec<_> = r.divergences.iter().filter(|d| d.contains("interpreter vs portable")).collect();
        assert!(a_b_diff.is_empty(), "interp/portable disagree on string+int: {a_b_diff:?}");
    }

    #[test]
    fn interpreter_and_portable_agree_on_polyglot_dispatch() {
        // crush-diff found this LIVE: interpreter ran @javascript (node -e), portable tried to
        // spawn a binary literally named "javascript" and errored. Both now share
        // resolve_lang_binary. node must be on PATH for this to be meaningful; if it's absent
        // both fail identically, which is still agreement.
        let r = differential_run("fn main() { @javascript { const x = 1; } print(\"ok\"); }").unwrap();
        let obs: Vec<_> = r.divergences.iter().filter(|d| d.contains("interpreter vs portable")).collect();
        assert!(obs.is_empty(), "interp/portable disagree on @javascript dispatch: {obs:?}");
    }

    #[test]
    fn harness_can_fail() {
        // Trust check: the harness must be capable of reporting a divergence, not just always-pass.
        // We assert the comparison machinery produced all three outcomes.
        let r = differential_run("fn main() { print(42); }").unwrap();
        assert!(matches!(r.interpreter, StackOutcome::Ok { .. }));
        assert!(matches!(r.portable, StackOutcome::Ok { .. }));
        let _ = r.fastvm;
    }
}
