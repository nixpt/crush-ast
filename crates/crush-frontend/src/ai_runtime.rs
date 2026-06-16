use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct AIExecutionContext {
    pub current_goals: HashMap<String, Goal>,
    pub active_adaptations: Vec<Adaptation>,
    pub collaboration_state: HashMap<String, CollaborationState>,
    pub knowledge_base: KnowledgeBase,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Goal {
    pub id: String,
    pub description: String,
    pub progress: f64,
    pub success_criteria: Vec<String>,
    pub created_at: u64,
    pub deadline: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Adaptation {
    pub adaptation_type: AdaptationType,
    pub parameters: HashMap<String, serde_json::Value>,
    pub applied_at: u64,
    pub effectiveness: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AdaptationType {
    Performance,
    Reliability,
    Usability,
    Compatibility,
    Learning,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollaborationState {
    pub partner_id: String,
    pub state: String,
    pub last_interaction: u64,
    pub trust_level: f64,
}

#[derive(Debug, Default)]
pub struct KnowledgeBase {
    pub patterns: Vec<LearnedPattern>,
    pub solutions: Vec<Solution>,
    pub warnings: Vec<Warning>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearnedPattern {
    pub pattern: String,
    pub confidence: f64,
    pub usage_count: u32,
    pub last_used: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Solution {
    pub problem: String,
    pub solution: String,
    pub success_rate: f64,
    pub usage_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Warning {
    pub condition: String,
    pub message: String,
    pub severity: String,
    pub frequency: u32,
}

#[derive(Debug, Default)]
pub struct LearningEngine {
    pub patterns: HashMap<String, PatternData>,
    pub adaptations: Vec<AdaptationRecord>,
}

#[derive(Debug, Clone)]
pub struct PatternData {
    pub occurrences: u32,
    pub success_rate: f64,
    pub average_duration: f64,
    pub last_seen: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdaptationRecord {
    pub timestamp: u64,
    pub adaptation_type: String,
    pub reason: String,
    pub outcome: String,
    pub metrics_before: HashMap<String, f64>,
    pub metrics_after: HashMap<String, f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AIExecutionResult {
    pub results: Vec<serde_json::Value>,
    pub insights: Vec<String>,
    pub suggestions: Vec<String>,
    pub metrics: HashMap<String, f64>,
}

pub struct AIRuntime {
    context: AIExecutionContext,
    learning_engine: LearningEngine,
}

impl Default for AIRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl AIRuntime {
    pub fn new() -> Self {
        Self {
            context: AIExecutionContext::default(),
            learning_engine: LearningEngine::default(),
        }
    }

    pub fn context(&self) -> &AIExecutionContext {
        &self.context
    }

    pub fn context_mut(&mut self) -> &mut AIExecutionContext {
        &mut self.context
    }

    pub fn learning_engine(&self) -> &LearningEngine {
        &self.learning_engine
    }

    pub fn learning_engine_mut(&mut self) -> &mut LearningEngine {
        &mut self.learning_engine
    }

    pub fn generate_insights(&self) -> Vec<String> {
        let mut insights = Vec::new();

        for (pattern_name, data) in &self.learning_engine.patterns {
            if data.occurrences > 5 {
                insights.push(format!(
                    "Pattern '{}' used {} times, avg duration: {:.2}s",
                    pattern_name, data.occurrences, data.average_duration
                ));
            }
        }

        let active_goals = self.context.current_goals.len();
        if active_goals > 0 {
            insights.push(format!("{} active goals being tracked", active_goals));
        }

        insights
    }

    pub fn generate_suggestions(&self) -> Vec<String> {
        let mut suggestions = Vec::new();

        if self
            .learning_engine
            .patterns
            .contains_key("crush_statement")
        {
            suggestions.push("Consider using AI-native constructs for better performance".to_string());
        }

        if self.context.knowledge_base.patterns.len() > 3 {
            suggestions.push("Consider creating a tool chain for repeated patterns".to_string());
        }

        suggestions
    }

    pub fn collect_metrics(&self) -> HashMap<String, f64> {
        let mut metrics = HashMap::new();

        metrics.insert(
            "active_goals".to_string(),
            self.context.current_goals.len() as f64,
        );
        metrics.insert(
            "learned_patterns".to_string(),
            self.context.knowledge_base.patterns.len() as f64,
        );
        metrics.insert(
            "active_adaptations".to_string(),
            self.context.active_adaptations.len() as f64,
        );

        metrics
    }

    pub fn create_result(&self, results: Vec<serde_json::Value>) -> AIExecutionResult {
        AIExecutionResult {
            results,
            insights: self.generate_insights(),
            suggestions: self.generate_suggestions(),
            metrics: self.collect_metrics(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runtime_creation() {
        let runtime = AIRuntime::new();
        assert!(runtime.context().current_goals.is_empty());
        assert!(runtime.learning_engine().patterns.is_empty());
    }

    #[test]
    fn test_insights_generation() {
        let runtime = AIRuntime::new();
        let insights = runtime.generate_insights();
        assert!(insights.is_empty());
    }

    #[test]
    fn test_metrics_collection() {
        let runtime = AIRuntime::new();
        let metrics = runtime.collect_metrics();

        assert_eq!(metrics.get("active_goals"), Some(&0.0));
        assert_eq!(metrics.get("learned_patterns"), Some(&0.0));
        assert_eq!(metrics.get("active_adaptations"), Some(&0.0));
    }
}
