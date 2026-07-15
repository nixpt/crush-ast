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
        CsonValue::Boolean(b) => Expression::BoolLiteral { value: *b, meta: HashMap::new() },
        CsonValue::Number(n) => {
            if n.fract() == 0.0 && *n <= i64::MAX as f64 && *n >= i64::MIN as f64 {
                Expression::IntLiteral { value: *n as i64, meta: HashMap::new() }
            } else {
                Expression::FloatLiteral { value: *n, meta: HashMap::new() }
            }
        }
        CsonValue::String(s) => Expression::StringLiteral { value: s.clone(), meta: HashMap::new() },

        CsonValue::Array(elements) => {
            let exprs = elements.iter().map(desugar_cson_to_expr).collect();
            Expression::ArrayLiteral { elements: exprs, meta: HashMap::new() }
        }

        CsonValue::Object(properties) => {
            let mut elements = Vec::new();
            for (k, v) in properties {
                if let Some(semantic_key) = k.strip_prefix('~') {
                    // Semantic object: wrap key as std::ai::semantic_anchor call
                    let key_expr = Expression::Call {
                        function: "std::ai::semantic_anchor".to_string(),
                        args: vec![Expression::StringLiteral {
                            value: semantic_key.to_string(),
                            meta: HashMap::new(),
                        }],
                        meta: HashMap::new(),
                    };
                    let val_expr = desugar_cson_to_expr(v);
                    elements.push(Expression::ArrayLiteral {
                        elements: vec![key_expr, val_expr],
                        meta: HashMap::new(),
                    });
                } else {
                    // Regular object: return as ObjectLiteral
                    // For simple objects, build properties directly
                    break; // Fall through to ObjectLiteral path below
                }
            }
            if elements.is_empty() {
                // No semantic keys — build as ObjectLiteral
                let mut props = Vec::new();
                for (k, v) in properties {
                    props.push((k.clone(), desugar_cson_to_expr(v)));
                }
                Expression::ObjectLiteral { properties: props, meta: HashMap::new() }
            } else {
                Expression::ArrayLiteral { elements, meta: HashMap::new() }
            }
        }

        CsonValue::Synthesize(prompt) => {
            Expression::AI(AIExpression::Synthesize {
                output_type: CastType::Any,
                constraints: vec![prompt.clone()],
                context_refs: vec![],
                examples: None,
            })
        }
    }
}
