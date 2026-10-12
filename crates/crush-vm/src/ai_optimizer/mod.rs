//! GC-timing predictor for FastVM (CRUSH-256).
//!
//! FastVM asks [`VmOptimizer::should_gc`] every 64 instructions whether to
//! collect, from four features: current arena memory, peak memory,
//! instructions since the last collection, and allocation rate.
//!
//! This used to run an ONNX model through `ort` (ONNX Runtime). No model was
//! ever shipped, so the session never loaded and the answer was always "no" —
//! while every default build downloaded and linked ONNX Runtime. The runtime
//! is gone; the default policy is unchanged ([`GcPolicy::Never`]: the arena
//! still collects on its own thresholds). A policy fitted offline from
//! [`VmOptimizer::dump_telemetry_csv`] should land as plain Rust (a few
//! comparisons, nanoseconds, every target incl. wasm); loading a model file,
//! if wanted, belongs behind a pure-Rust backend such as tract (CRUSH-256).

use anyhow::{bail, Result};

/// Default path for the GC prediction model (no model is shipped).
pub const GC_MODEL_PATH: &str = "crates/crush-vm/models/gc_model.onnx";
/// Fallback path for when running from crate root (e.g. tests).
pub const GC_MODEL_FALLBACK_PATH: &str = "models/gc_model.onnx";

/// How [`VmOptimizer::should_gc`] decides.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GcPolicy {
    /// Never ask for an extra collection (the behaviour of every build so far,
    /// since no model was ever present).
    #[default]
    Never,
    /// The telemetry labelling rule: more than 512 instructions since the last
    /// collection, or above 70% of peak with more than 1 KiB live.
    Heuristic,
}

/// GC-timing predictor for the NanoVM / FastVM.
pub struct VmOptimizer {
    policy: GcPolicy,
    /// Test mode for verification: collect when more than 10 instructions
    /// have run since the last collection.
    pub(crate) test_mode: bool,
    /// Telemetry log for training data collection.
    pub(crate) telemetry_log: Vec<([f32; 4], bool)>,
    /// Whether to log telemetry.
    pub(crate) log_enabled: bool,
}

impl Default for VmOptimizer {
    fn default() -> Self {
        Self::new()
    }
}

impl VmOptimizer {
    /// Create an optimizer with the default policy ([`GcPolicy::Never`]).
    pub fn new() -> Self {
        Self { policy: GcPolicy::default(), test_mode: false, telemetry_log: Vec::new(), log_enabled: false }
    }

    /// Choose the decision policy.
    pub fn set_policy(&mut self, policy: GcPolicy) {
        self.policy = policy;
    }

    /// The current decision policy.
    pub fn policy(&self) -> GcPolicy {
        self.policy
    }

    /// Enable telemetry logging.
    pub fn enable_logging(&mut self, enabled: bool) {
        self.log_enabled = enabled;
    }

    /// Dump telemetry log to CSV format (training data for a fitted policy).
    pub fn dump_telemetry_csv(&self) -> String {
        let mut csv = String::from("current_mem,peak_mem,instr_since_gc,alloc_rate,label\n");
        for (inputs, label) in &self.telemetry_log {
            csv.push_str(&format!(
                "{},{},{},{},{}\n",
                inputs[0],
                inputs[1],
                inputs[2],
                inputs[3],
                if *label { 1 } else { 0 }
            ));
        }
        csv
    }

    /// Enable test mode.
    pub fn enable_test_mode(&mut self) {
        self.test_mode = true;
    }

    /// Loading an ONNX model is not supported: ONNX Runtime was removed from
    /// crush-vm (CRUSH-256). Errors instead of silently keeping the old policy.
    pub fn load_gc_model(&mut self, path: &str) -> Result<()> {
        bail!(
            "cannot load GC model {path}: ONNX model loading was removed from crush-vm (CRUSH-256); \
             fit a policy from the telemetry CSV instead"
        )
    }

    fn heuristic(inputs: &[f32; 4]) -> bool {
        inputs[2] > 512.0 || (inputs[0] > 0.7 * inputs[1] && inputs[0] > 1024.0)
    }

    /// Should a collection run now?
    ///
    /// `inputs`: `[current_memory, peak_memory, instructions_since_gc, alloc_rate]`.
    pub fn should_gc(&mut self, inputs: [f32; 4]) -> bool {
        if self.log_enabled {
            self.telemetry_log.push((inputs, Self::heuristic(&inputs)));
        }
        if self.test_mode {
            return inputs[2] > 10.0;
        }
        match self.policy {
            GcPolicy::Never => false,
            GcPolicy::Heuristic => Self::heuristic(&inputs),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_policy_never_collects() {
        let mut o = VmOptimizer::new();
        assert!(!o.should_gc([1e9, 1e9, 1e6, 0.0]));
    }

    #[test]
    fn heuristic_and_test_mode() {
        let mut o = VmOptimizer::new();
        o.set_policy(GcPolicy::Heuristic);
        assert!(o.should_gc([0.0, 0.0, 513.0, 0.0]));
        assert!(o.should_gc([2000.0, 2500.0, 1.0, 0.0]));
        assert!(!o.should_gc([500.0, 2500.0, 1.0, 0.0]));
        let mut t = VmOptimizer::new();
        t.enable_test_mode();
        assert!(t.should_gc([0.0, 0.0, 11.0, 0.0]));
        assert!(!t.should_gc([0.0, 0.0, 10.0, 0.0]));
    }

    #[test]
    fn telemetry_is_labelled_with_the_heuristic() {
        let mut o = VmOptimizer::new();
        o.enable_logging(true);
        o.should_gc([0.0, 0.0, 600.0, 0.0]);
        assert!(o.dump_telemetry_csv().ends_with("0,0,600,0,1\n"));
    }

    #[test]
    fn loading_a_model_is_an_error() {
        let err = VmOptimizer::new().load_gc_model("x.onnx").unwrap_err().to_string();
        assert!(err.contains("CRUSH-256"), "{err}");
    }
}
