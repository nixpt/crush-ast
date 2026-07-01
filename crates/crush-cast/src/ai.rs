use crate::Expression;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Extended AST with AI-native constructs
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
#[serde(tag = "ai_type")]
pub enum AIExpression {
    /// AI-specific expressions
    Query {
        /// Natural language query for AI processing
        query: String,
        /// Expected result type
        result_type: Option<String>,
        /// Context information
        #[serde(default)]
        context: HashMap<String, serde_json::Value>,
    },

    ToolChain {
        /// Chain of tool calls
        tools: Vec<ToolCall>,
        /// Execution strategy (sequential, parallel, conditional)
        strategy: ExecutionStrategy,
        /// Error handling strategy
        error_handling: ErrorHandling,
    },

    AgentDelegation {
        /// Task description for delegation
        task: String,
        /// Target agents (can be patterns or specific IDs)
        agents: Vec<String>,
        /// Delegation strategy
        delegation_strategy: DelegationStrategy,
        /// Expected result format
        expected_format: Option<String>,
    },

    LearningLoop {
        /// What to learn from
        learning_target: LearningTarget,
        /// Learning strategy
        strategy: LearningStrategy,
        /// Adaptation actions
        adaptations: Vec<AdaptationAction>,
    },

    ContextAware {
        /// The wrapped expression
        expression: Box<Expression>,
        /// Context requirements
        requires_context: Vec<String>,
        /// Context providers
        provides_context: Vec<String>,
    },

    SemanticMatch {
        /// The expression to evaluate
        target: Box<Expression>,
        /// The natural language concept to match against
        concept: String,
        /// Confidence threshold for the match (e.g., 0.85)
        confidence_threshold: f64,
    },

    Synthesize {
        /// The target type to generate
        output_type: crate::CastType,
        /// Constraints or instructions for generation
        constraints: Vec<String>,
        /// Values or variables to feed into the prompt context
        context_refs: Vec<Expression>,
        /// Optional few-shot examples
        examples: Option<Vec<Expression>>,
    },
}

/// Tool call specification
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct ToolCall {
    pub tool_name: String,
    pub parameters: HashMap<String, serde_json::Value>,
    pub result_binding: Option<String>,
    pub condition: Option<String>, // When to execute this tool
    pub required_capability: Option<String>, // The capability required to call this tool
}

/// Execution strategies for tool chains
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
#[serde(tag = "type")]
pub enum ExecutionStrategy {
    Sequential,
    Parallel,
    Conditional {
        conditions: Vec<String>,
        // We use Strings for labels or indices into a body vector elsewhere,
        // but here it's cleaner to keep it simple or use Expression
    },
    Retry {
        max_attempts: u32,
        backoff_strategy: BackoffStrategy,
    },
}

/// Error handling strategies
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
#[serde(tag = "type")]
pub enum ErrorHandling {
    FailFast,
    ContinueOnError,
    Retry {
        max_retries: u32,
        retry_condition: Option<String>,
    },
    Fallback {
        fallback_tools: Vec<ToolCall>,
    },
}

/// Delegation strategies for agent coordination
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub enum DelegationStrategy {
    FirstAvailable,
    CapabilityMatch,
    ParallelSplit,
    Hierarchical,
    Consensus {
        threshold: f64,
    },
    /// Dispatch to all listed agents in parallel.
    Broadcast,
    /// Pick highest-rated agent for the task domain.
    Best,
    /// Cycle through the list across calls.
    RoundRobin,
}

/// Learning targets
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub enum LearningTarget {
    UserBehavior,
    ExecutionPatterns,
    ErrorPatterns,
    PerformanceMetrics,
    ToolUsage,
}

/// Learning strategies
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub enum LearningStrategy {
    PatternRecognition,
    StatisticalAnalysis,
    MachineLearning,
    RuleBased,
}

/// Adaptation actions
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub enum AdaptationAction {
    OptimizeToolChain,
    ImproveErrorHandling,
    UpdateAgentSelection,
    ModifyExecutionStrategy,
    LearnNewPatterns,
}

/// Backoff strategies for retries
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
#[serde(tag = "type")]
pub enum BackoffStrategy {
    Fixed {
        delay_ms: u64,
    },
    Exponential {
        base_delay_ms: u64,
        max_delay_ms: u64,
    },
    Linear {
        increment_ms: u64,
    },
}

/// AI-Native Statement extensions
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
#[serde(tag = "ai_type")]
pub enum AIStatement {
    GoalDeclaration {
        goal_id: String,
        description: String,
        success_criteria: Vec<String>,
        priority: Priority,
        deadline: Option<String>,
    },

    ProgressUpdate {
        goal_id: String,
        progress: f64, // 0.0 to 1.0
        status_message: String,
        #[serde(default)]
        metrics: HashMap<String, f64>,
    },

    KnowledgeSharing {
        knowledge_type: KnowledgeType,
        content: serde_json::Value,
        recipients: Vec<String>, // Agent IDs or patterns
        retention_policy: RetentionPolicy,
    },

    CapabilityDiscovery {
        domain: String,
        requirements: Vec<String>,
        discovery_strategy: DiscoveryStrategy,
    },

    AdaptationRequest {
        adaptation_type: AdaptationType,
        reason: String,
        #[serde(default)]
        parameters: HashMap<String, serde_json::Value>,
    },

    SemanticSwitch {
        /// The expression to evaluate
        target: Box<Expression>,
        /// Cases map a natural language concept to a body of statements
        cases: Vec<(String, Vec<crate::Statement>)>,
        /// Optional fallback block if no concept matches well enough
        fallback: Option<Vec<crate::Statement>>,
    },
}

/// Priority levels for goals and tasks
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub enum Priority {
    Low,
    Medium,
    High,
    Critical,
}

/// Types of knowledge for sharing
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub enum KnowledgeType {
    Pattern,
    Solution,
    BestPractice,
    Warning,
    Insight,
}

/// Knowledge retention policies
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub enum RetentionPolicy {
    Ephemeral,
    Session,
    Persistent,
    Conditional { condition: String },
}

/// Discovery strategies for capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub enum DiscoveryStrategy {
    Broadcast,
    Targeted,
    Hierarchical,
    LearningBased,
}

/// Types of adaptation requests
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub enum AdaptationType {
    Performance,
    Reliability,
    Usability,
    Compatibility,
    Learning,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct AIMetadata {
    pub description: String,
    pub ai_tags: Vec<String>,
    pub required_capabilities: Vec<String>,
    pub execution_context: ExecutionContext,
    pub learning_objectives: Vec<String>,
    pub collaboration_patterns: Vec<CollaborationPattern>,
    #[serde(default)]
    pub inputs: Vec<ParameterSchema>,
    #[serde(default)]
    pub outputs: Vec<ParameterSchema>,
    #[serde(default)]
    pub complexity: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct ParameterSchema {
    pub name: String,
    pub description: String,
    pub type_hint: String,
    pub required: bool,
    pub default_value: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct ToolSchema {
    pub name: String,
    pub description: String,
    pub parameters: HashMap<String, ParameterSchema>,
    pub return_type: String,
    pub mcp_server: Option<String>,
    pub mcp_method: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub enum LearningSource {
    ExecutionResults,
    UserFeedback,
    EnvironmentObservations,
    PeerAgents { agent_ids: Vec<String> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub enum AdaptationStrategy {
    PerformanceOptimization,
    Personalization,
    CapabilityExpansion,
    CollaborationEnhancement,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct ExecutionContext {
    pub environment: Vec<String>,
    pub resources: Vec<String>,
    pub permissions: Vec<String>,
    pub dependencies: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct CollaborationPattern {
    pub pattern_type: String,
    pub participants: Vec<String>,
    pub communication_style: String,
    pub decision_making: String,
}
