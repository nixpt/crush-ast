use anyhow::{bail, Result};
use regex::Regex;
use serde_json::json;
use std::collections::HashMap;
use std::env;
use std::fs;

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        bail!("Usage: bash_walker <file.sh>");
    }

    let source_path = &args[1];
    let source_code = fs::read_to_string(source_path)?;

    let mut main_body = Vec::new();

    // Regex for variable assignment: NAME=VALUE
    let var_regex = Regex::new(r"(?m)^(\w+)=(.*)$")?;
    // Regex for echo: echo CONTENT
    let echo_regex = Regex::new(r"(?m)^echo\s+(.*)$")?;

    for line in source_code.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("#") {
            continue;
        }

        if let Some(caps) = var_regex.captures(line) {
            let name = caps.get(1).unwrap().as_str().to_string();
            let val_str = caps
                .get(2)
                .unwrap()
                .as_str()
                .trim_matches(|c| c == '"' || c == '\'');

            main_body.push(json!({
                "type": "VarDecl",
                "name": name,
                "meta": {},
                "value": {
                    "type": "StringLiteral",
                    "value": val_str.to_string()
                }
            }));
        } else if let Some(caps) = echo_regex.captures(line) {
            let content = caps
                .get(1)
                .unwrap()
                .as_str()
                .trim_matches(|c| c == '"' || c == '\'');

            let arg = if content.starts_with('$') {
                json!({
                    "type": "Var",
                    "name": content[1..].to_string()
                })
            } else {
                json!({
                    "type": "StringLiteral",
                    "value": content.to_string()
                })
            };

            main_body.push(json!({
                "type": "ExprStmt",
                "expr": {
                    "type": "CapabilityCall",
                    "name": "io.print",
                    "args": [arg],
                    "meta": {
                        "capability": true,
                        "namespace": "io",
                        "method": "print"
                    }
                },
                "meta": {}
            }));
        }
    }

    let mut functions = HashMap::new();
    functions.insert(
        "main".to_string(),
        json!({
            "params": [],
            "body": main_body,
            "meta": {}
        }),
    );

    let cast = json!({
        "version": "0.1",
        "entry": "main",
        "functions": functions
    });

    println!("{}", serde_json::to_string_pretty(&cast)?);

    Ok(())
}
