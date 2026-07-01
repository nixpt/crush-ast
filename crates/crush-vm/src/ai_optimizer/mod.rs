use anyhow::Result;
use ort::session::Session;
use ort::value::Tensor;
use parking_lot::Mutex;
use std::sync::Arc;

/// Default path for the GC prediction model
pub const GC_MODEL_PATH: &str = "crates/crush-vm/models/gc_model.onnx";
/// Fallback path for when running from crate root (e.g. tests)
pub const GC_MODEL_FALLBACK_PATH: &str = "models/gc_model.onnx";

/// AI-powered optimizer for the NanoVM.
pub struct VmOptimizer {
    /// Predictive GC session
    gc_session: Option<Arc<Mutex<Session>>>,
    /// Test mode for verification without ONNX files
    pub(crate) test_mode: bool,
    /// Telemetry log for training data collection
    pub(crate) telemetry_log: Vec<(Vec<f32>, bool)>,
    /// Whether to log telemetry
    pub(crate) log_enabled: bool,
}

impl VmOptimizer {
    /// Create a new VM Optimizer.
    pub fn new() -> Self {
        let mut opt = Self {
            gc_session: None,
            test_mode: false,
            telemetry_log: Vec::new(),
            log_enabled: false,
        };
        
        // Attempt to auto-load the default model if it exists
        if std::path::Path::new(GC_MODEL_PATH).exists() {
            let _ = opt.load_gc_model(GC_MODEL_PATH);
        } else if std::path::Path::new(GC_MODEL_FALLBACK_PATH).exists() {
            let _ = opt.load_gc_model(GC_MODEL_FALLBACK_PATH);
        }
        
        opt
    }

    /// Enable telemetry logging
    pub fn enable_logging(&mut self, enabled: bool) {
        self.log_enabled = enabled;
    }

    /// Dump telemetry log to CSV format
    pub fn dump_telemetry_csv(&self) -> String {
        let mut csv = String::from("current_mem,peak_mem,instr_since_gc,alloc_rate,label\n");
        for (inputs, label) in &self.telemetry_log {
            let row = format!(
                "{},{},{},{},{}\n",
                inputs[0], inputs[1], inputs[2], inputs[3],
                if *label { 1 } else { 0 }
            );
            csv.push_str(&row);
        }
        csv
    }

    /// Enable test mode
    pub fn enable_test_mode(&mut self) {
        self.test_mode = true;
    }

    /// Load the target GC prediction model.
    pub fn load_gc_model(&mut self, path: &str) -> Result<()> {
        let session = Session::builder()?
            .commit_from_file(path)?;
        self.gc_session = Some(Arc::new(Mutex::new(session)));
        Ok(())
    }

    /// Predict if a GC cycle should be triggered based on current VM state.
    /// 
    /// inputs: [current_memory, peak_memory, instructions_since_gc, alloc_rate]
    pub fn should_gc(&mut self, inputs: Vec<f32>) -> bool {
        if self.log_enabled {
            // Heuristic labels for training: 
            // If instructions > 500 or memory > 70% of peak, suggest GC
            let label = inputs[2] > 512.0 || (inputs[0] > 0.7 * inputs[1] && inputs[0] > 1024.0);
            self.telemetry_log.push((inputs.clone(), label));
        }

        if self.test_mode {
            // In test mode: trigger GC if instructions_since_gc > 10
            return inputs.get(2).cloned().unwrap_or(0.0) > 10.0;
        }

        let session_lock = match &self.gc_session {
            Some(s) => s,
            None => return false,
        };
        
        let mut session = session_lock.lock();
        
        // Prepare input tensor [1, 4]
        let input_tensor = match Tensor::from_array((vec![1, 4], inputs.clone())) {
            Ok(t) => t,
            Err(_) => return false,
        };
        
        // Run inference
        let outputs = match session.run(ort::inputs!["float_input" => input_tensor]) {
            Ok(out) => out,
            Err(_) => return false,
        };
        
        // Extract predicted label (usually the first output)
        if let Some(label_value) = outputs.get("output_label") {
            if let Ok((_shape, data)) = label_value.try_extract_tensor::<i64>() {
                return data.first().cloned().unwrap_or(0) == 1;
            }
        } else if let Some(label_value) = outputs.get("label") {
             if let Ok((_shape, data)) = label_value.try_extract_tensor::<i64>() {
                return data.first().cloned().unwrap_or(0) == 1;
            }
        }
        
        false
    }
}
