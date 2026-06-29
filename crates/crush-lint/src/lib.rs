use crush_diagnostics::DiagRecord;
use ort::session::{Session, builder::GraphOptimizationLevel};
use std::sync::Arc;
use parking_lot::Mutex;

pub const LINT_MODEL_PATH: &str = "crates/crush-lint/models/lint_model.onnx";

/// The AI Linter Engine that augments standard compiler errors.
pub struct AiLinter {
    session: Option<Arc<Mutex<Session>>>,
    enabled: bool,
}

impl AiLinter {
    pub fn new(enabled: bool) -> Self {
        let mut session = None;
        if enabled {
            // ORT 2.0.0 automatically initializes the environment on first use in most configurations.
            if std::path::Path::new(LINT_MODEL_PATH).exists() {
                if let Ok(sess) = ort::session::Session::builder()
                    .unwrap()
                    .with_optimization_level(GraphOptimizationLevel::Level3)
                    .unwrap()
                    .commit_from_file(LINT_MODEL_PATH)
                {
                    session = Some(Arc::new(Mutex::new(sess)));
                }
            }
        }
        
        Self { session, enabled }
    }

    /// Augment a standard diagnostic record with an AI-generated hint.
    pub fn augment_diagnostic<'a>(&self, diag: DiagRecord<'a>, source_context: &str) -> DiagRecord<'a> {
        if !self.enabled {
            return diag;
        }

        // In a real implementation, we would convert `source_context` to an embedding 
        // vector and run it through `self.session`. 
        
        // --- Dejavue Context Protocol (DCP) Integration ---
        // An AI-Native linter should be aware of project memory.
        let mut dejavue_context = String::new();
        if let Ok(decisions) = std::fs::read_to_string(".dejavue/decisions.md") {
            dejavue_context.push_str(" Project Context from .dejavue: ");
            // Grab the first 100 chars as a summary for the hint
            dejavue_context.push_str(&decisions.chars().take(100).collect::<String>());
            dejavue_context.push_str("...");
        }

        // --- Simulated AI Inference ---
        let predicted_hint: Option<String> = if diag.message.contains("Missing semicolon") {
            Some(format!("Hint: It looks like you forgot a semicolon after '{}'.{}", source_context.trim(), dejavue_context))
        } else if diag.message.contains("Unexpected token") {
            Some(format!("Hint: Check for unmatched brackets near '{}'.{}", source_context.trim(), dejavue_context))
        } else {
            // General embedding-based similarity hint
            Some(format!("Hint: Based on typical patterns, you might need to refactor '{}'.{}", source_context.trim(), dejavue_context))
        };
        // ------------------------------

        // To make the lifetimes work, the caller typically passes owned strings or we 
        // leak it / store it in an arena. Since DiagRecord borrows from the caller, 
        // we use a static string for the mock, or the caller must manage memory.
        
        // For the sake of this SDK demo, we just print the AI hint to stderr, 
        // because DiagRecord only accepts `&'a str`.
        if let Some(hint) = predicted_hint {
            eprintln!("\n🤖 [AI Linter] {}", hint);
            
            // --- Dejavue Context Protocol (DCP) Write Integration ---
            // We log compiler/linter errors directly to the project memory!
            // This way, the next AI Agent immediately sees what failed in its memory trace.
            if std::path::Path::new(".dejavue").exists() {
                use std::io::Write;
                if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(".dejavue/timeline.jsonl") {
                    let ts = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
                    let file_path = diag.file.unwrap_or("unknown");
                    // We construct the jsonl manually for simplicity here
                    let log_entry = format!(
                        r#"{{"ts": {}, "agent": "crush-lint", "event": "lint_error", "path": "{}", "summary": "{}", "hint": "{}"}}"#,
                        ts, file_path, diag.message, hint.replace("\"", "\\\"")
                    );
                    writeln!(file, "{}", log_entry).ok();
                }
            }
        }

        diag
    }
}
