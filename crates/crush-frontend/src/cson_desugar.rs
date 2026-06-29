use crush_cast::cson::{CsonKey, CsonNode, CsonValue};
use crush_cast::{Expression, CastType};
use crush_cast::ai::AIExpression;
use std::collections::HashMap;

/// Converts a CSON AST Node into a fully executable Crush Expression.
/// This allows CSON data blocks containing `@synthesize` or fuzzy keys 
/// to be compiled directly into executable CAST logic by the frontend.
pub fn desugar_cson_to_expr(node: &CsonNode) -> Expression {
    match &node.value {
        CsonValue::Null => Expression::NullLiteral { meta: HashMap::new() },
        CsonValue::Bool { value } => Expression::BoolLiteral { value: *value, meta: HashMap::new() },
        CsonValue::Int { value } => Expression::IntLiteral { value: *value, meta: HashMap::new() },
        CsonValue::Float { value } => Expression::FloatLiteral { value: *value, meta: HashMap::new() },
        CsonValue::String { value } => Expression::StringLiteral { value: value.clone(), meta: HashMap::new() },
        
        CsonValue::Array { elements } => {
            let exprs = elements.iter().map(desugar_cson_to_expr).collect();
            Expression::ArrayLiteral { elements: exprs, meta: HashMap::new() }
        }
        
        CsonValue::Object { properties } => {
            let mut props = Vec::new();
            for (k, v) in properties {
                props.push((k.clone(), desugar_cson_to_expr(v)));
            }
            Expression::ObjectLiteral { properties: props, meta: HashMap::new() }
        }
        
        CsonValue::SemanticObject { properties } => {
            // A SemanticObject requires runtime execution to match fuzzy keys.
            // We desugar this into an Array of Tuples where each tuple is `(key_expr, value_expr)`.
            // The CVM will interpret this array when evaluating CSON blocks.
            let mut elements = Vec::new();
            for (key, val) in properties {
                let key_expr = match key {
                    CsonKey::Exact { value } => Expression::StringLiteral { value: value.clone(), meta: HashMap::new() },
                    // Wrap semantic keys in a Call to indicate they are semantic anchors
                    CsonKey::Semantic { value } => Expression::Call {
                        function: "std::ai::semantic_anchor".to_string(),
                        args: vec![Expression::StringLiteral { value: value.clone(), meta: HashMap::new() }],
                        meta: HashMap::new(),
                    }
                };
                
                let val_expr = desugar_cson_to_expr(val);
                elements.push(Expression::ArrayLiteral {
                    elements: vec![key_expr, val_expr],
                    meta: HashMap::new()
                });
            }
            Expression::ArrayLiteral { elements, meta: HashMap::new() }
        }
        
        CsonValue::Synthesize { prompt } => {
            // Desugar `@synthesize` directly into an AIExpression::Synthesize block.
            // The CVM natively executes these blocks by querying the agent fabric.
            Expression::AI(AIExpression::Synthesize {
                output_type: CastType::Any, // By default we expect Any type back from a CSON synthesize
                constraints: vec![prompt.clone()],
                context_refs: vec![],
                examples: None,
            })
        }
    }
}
