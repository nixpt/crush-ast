//! `crush-lang-sona` — A highly optimized parallel scripting language targeting the FastVM path.
//! Sona compiles directly to CASM and runs using the VM's optimized FastVM execution loop.

use anyhow::{Result, anyhow};
use std::collections::HashMap;
use casm::{Program, OpCode, Manifest, Function, Instruction};

/// AST Node for Sona
#[derive(Debug, Clone)]
pub enum SonaExpr {
    Int(i64),
    Float(f64),
    Str(String),
    Bool(bool),
    Var(String),
    Binary(String, Box<SonaExpr>, Box<SonaExpr>),
    Assign(String, Box<SonaExpr>),
    Spawn(Vec<SonaExpr>),
    Say(Box<SonaExpr>),
}

/// Simple Parser for Sona
pub struct SonaParser<'a> {
    input: &'a str,
    pos: usize,
}

impl<'a> SonaParser<'a> {
    pub fn new(input: &'a str) -> Self {
        Self { input, pos: 0 }
    }

    fn skip_whitespace(&mut self) {
        while self.pos < self.input.len() {
            let c = self.input.as_bytes()[self.pos] as char;
            if c.is_whitespace() {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn next_token(&mut self) -> Option<String> {
        self.skip_whitespace();
        if self.pos >= self.input.len() {
            return None;
        }

        let start = self.pos;
        let bytes = self.input.as_bytes();
        let c = bytes[self.pos] as char;

        if c == '(' || c == ')' || c == '{' || c == '}' || c == '=' || c == '+' || c == '-' || c == '*' || c == '/' {
            self.pos += 1;
            return Some(c.to_string());
        }

        if c == '"' {
            self.pos += 1;
            let str_start = self.pos;
            while self.pos < self.input.len() && bytes[self.pos] as char != '"' {
                self.pos += 1;
            }
            let val = self.input[str_start..self.pos].to_string();
            if self.pos < self.input.len() {
                self.pos += 1; // skip closing quote
            }
            return Some(format!("\"{}\"", val));
        }

        while self.pos < self.input.len() {
            let nc = bytes[self.pos] as char;
            if nc.is_whitespace() || "(){}=+-*/\"".contains(nc) {
                break;
            }
            self.pos += 1;
        }

        Some(self.input[start..self.pos].to_string())
    }

    pub fn parse(&mut self) -> Result<Vec<SonaExpr>> {
        let mut exprs = Vec::new();
        while let Some(tok) = self.next_token() {
            if tok == "let" {
                let name = self.next_token().ok_or_else(|| anyhow!("Expected variable name after let"))?;
                let eq = self.next_token().ok_or_else(|| anyhow!("Expected '='"))?;
                if eq != "=" { return Err(anyhow!("Expected '='")); }
                let val_expr = self.parse_expr()?;
                exprs.push(SonaExpr::Assign(name, Box::new(val_expr)));
            } else if tok == "say" {
                let lp = self.next_token().ok_or_else(|| anyhow!("Expected '('"))?;
                if lp != "(" { return Err(anyhow!("Expected '('")); }
                let arg = self.parse_expr()?;
                let rp = self.next_token().ok_or_else(|| anyhow!("Expected ')'"))?;
                if rp != ")" { return Err(anyhow!("Expected ')'")); }
                exprs.push(SonaExpr::Say(Box::new(arg)));
            } else if tok == "spawn" {
                let lb = self.next_token().ok_or_else(|| anyhow!("Expected '{{'"))?;
                if lb != "{" { return Err(anyhow!("Expected '{{'")); }
                let mut block_exprs = Vec::new();
                loop {
                    self.skip_whitespace();
                    if self.pos < self.input.len() && self.input.as_bytes()[self.pos] as char == '}' {
                        self.pos += 1; // skip '}'
                        break;
                    }
                    // Parse statement inside spawn
                    if let Some(inner_tok) = self.next_token() {
                        if inner_tok == "say" {
                            let lp = self.next_token().ok_or_else(|| anyhow!("Expected '('"))?;
                            if lp != "(" { return Err(anyhow!("Expected '('")); }
                            let arg = self.parse_expr()?;
                            let rp = self.next_token().ok_or_else(|| anyhow!("Expected ')'"))?;
                            if rp != ")" { return Err(anyhow!("Expected ')'")); }
                            block_exprs.push(SonaExpr::Say(Box::new(arg)));
                        } else {
                            // parse a basic expr
                            self.pos -= inner_tok.len(); // backtrack
                            block_exprs.push(self.parse_expr()?);
                        }
                    } else {
                        return Err(anyhow!("Unclosed block in spawn"));
                    }
                }
                exprs.push(SonaExpr::Spawn(block_exprs));
            } else {
                // Parse normal expression
                self.pos -= tok.len(); // backtrack
                exprs.push(self.parse_expr()?);
            }
        }
        Ok(exprs)
    }

    fn parse_expr(&mut self) -> Result<SonaExpr> {
        let tok = self.next_token().ok_or_else(|| anyhow!("Unexpected EOF"))?;
        if tok.starts_with('"') && tok.ends_with('"') {
            return Ok(SonaExpr::Str(tok[1..tok.len()-1].to_string()));
        }
        if tok == "true" { return Ok(SonaExpr::Bool(true)); }
        if tok == "false" { return Ok(SonaExpr::Bool(false)); }
        if let Ok(i) = tok.parse::<i64>() { return Ok(SonaExpr::Int(i)); }
        if let Ok(f) = tok.parse::<f64>() { return Ok(SonaExpr::Float(f)); }

        // Peek next token for binary operator
        self.skip_whitespace();
        if self.pos < self.input.len() {
            let next_c = self.input.as_bytes()[self.pos] as char;
            if "+-*/".contains(next_c) {
                self.pos += 1; // consume operator
                let right = self.parse_expr()?;
                return Ok(SonaExpr::Binary(next_c.to_string(), Box::new(SonaExpr::Var(tok)), Box::new(right)));
            }
        }

        Ok(SonaExpr::Var(tok))
    }
}

/// Helper to map typed OpCodes to CASM instructions
fn opcode_to_instruction(op: OpCode) -> Instruction {
    let (op_name, args) = match op {
        OpCode::PushInt(i) => ("push_int", serde_json::json!({ "value": i })),
        OpCode::PushFloat(f) => ("push_float", serde_json::json!({ "value": f })),
        OpCode::PushStr(s) => ("push_str", serde_json::json!({ "value": s })),
        OpCode::PushBool(b) => ("push_bool", serde_json::json!({ "value": b })),
        OpCode::PushNull => ("push_null", serde_json::json!({})),
        OpCode::Pop => ("pop", serde_json::json!({})),
        OpCode::Dup => ("dup", serde_json::json!({})),
        OpCode::Store(name) => ("store", serde_json::json!({ "name": name })),
        OpCode::Load(name) => ("load", serde_json::json!({ "name": name })),
        OpCode::Add => ("add", serde_json::json!({})),
        OpCode::Sub => ("sub", serde_json::json!({})),
        OpCode::Mul => ("mul", serde_json::json!({})),
        OpCode::Div => ("div", serde_json::json!({})),
        OpCode::Mod => ("mod", serde_json::json!({})),
        OpCode::Neg => ("neg", serde_json::json!({})),
        OpCode::Eq => ("eq", serde_json::json!({})),
        OpCode::Ne => ("ne", serde_json::json!({})),
        OpCode::Lt => ("lt", serde_json::json!({})),
        OpCode::Gt => ("gt", serde_json::json!({})),
        OpCode::Le => ("le", serde_json::json!({})),
        OpCode::Ge => ("ge", serde_json::json!({})),
        OpCode::CapCall { name, argc } => ("cap_call", serde_json::json!({ "name": name, "argc": argc })),
        OpCode::Spawn => ("spawn", serde_json::json!({})),
        OpCode::Yield => ("yield", serde_json::json!({})),
        OpCode::Ret => ("ret", serde_json::json!({})),
        OpCode::Call(name) => ("call", serde_json::json!({ "name": name })),
        OpCode::Jmp(addr) => ("jmp", serde_json::json!({ "target": addr })),
        OpCode::JmpIf(addr) => ("jmp_if", serde_json::json!({ "target": addr })),
        OpCode::JmpIfNot(addr) => ("jmp_if_not", serde_json::json!({ "target": addr })),
        OpCode::Break => ("break", serde_json::json!({})),
        OpCode::Continue => ("continue", serde_json::json!({})),
        OpCode::And => ("and", serde_json::json!({})),
        OpCode::Or => ("or", serde_json::json!({})),
        OpCode::Not => ("not", serde_json::json!({})),
        OpCode::ArrLen => ("arr_len", serde_json::json!({})),
        OpCode::ArrPush => ("arr_push", serde_json::json!({})),
        OpCode::ArrPop => ("arr_pop", serde_json::json!({})),
        OpCode::ArrGet => ("arr_get", serde_json::json!({})),
        OpCode::ArrSet => ("arr_set", serde_json::json!({})),
        OpCode::NewArray(n) => ("new_array", serde_json::json!({ "count": n })),
        OpCode::Swap => ("swap", serde_json::json!({})),
        _ => panic!("Unsupported Sona opcode"),
    };
    Instruction {
        op: op_name.to_string(),
        lang: Some("sona".to_string()),
        meta: None,
        args,
    }
}

/// Sona Compiler to CASM
pub struct SonaCompiler {
    functions: HashMap<String, Function>,
    next_anon_id: usize,
}

impl SonaCompiler {
    pub fn compile(exprs: Vec<SonaExpr>) -> Result<Program> {
        let mut compiler = Self {
            functions: HashMap::new(),
            next_anon_id: 0,
        };
        let mut body = Vec::new();

        for expr in exprs {
            compiler.compile_expr(expr, &mut body)?;
        }

        body.push(OpCode::PushNull);
        body.push(OpCode::Ret);

        let body_instructions: Vec<Instruction> = body.into_iter().map(opcode_to_instruction).collect();

        compiler.functions.insert("main".to_string(), Function {
            params: vec![],
            locals: vec![],
            body: body_instructions,
        });

        Ok(Program {
            version: "1.0".to_string(),
            functions: compiler.functions,
            manifest: Manifest { permissions: vec!["io.print".to_string()] },
            lang: Some("sona".to_string()),
        })
    }

    fn compile_expr(&mut self, expr: SonaExpr, body: &mut Vec<OpCode>) -> Result<()> {
        match expr {
            SonaExpr::Int(i) => body.push(OpCode::PushInt(i)),
            SonaExpr::Float(f) => body.push(OpCode::PushFloat(f)),
            SonaExpr::Str(s) => body.push(OpCode::PushStr(s)),
            SonaExpr::Bool(b) => body.push(OpCode::PushBool(b)),
            SonaExpr::Var(name) => body.push(OpCode::Load(name)),
            SonaExpr::Assign(name, val) => {
                self.compile_expr(*val, body)?;
                body.push(OpCode::Store(name));
            }
            SonaExpr::Binary(op, left, right) => {
                self.compile_expr(*left, body)?;
                self.compile_expr(*right, body)?;
                match op.as_str() {
                    "+" => body.push(OpCode::Add),
                    "-" => body.push(OpCode::Sub),
                    "*" => body.push(OpCode::Mul),
                    "/" => body.push(OpCode::Div),
                    _ => return Err(anyhow!("Unsupported operator: {}", op)),
                }
            }
            SonaExpr::Say(arg) => {
                self.compile_expr(*arg, body)?;
                body.push(OpCode::CapCall {
                    name: "io.print".to_string(),
                    argc: 1,
                });
            }
            SonaExpr::Spawn(block) => {
                let func_name = format!("anon_fn_{}", self.next_anon_id);
                self.next_anon_id += 1;

                let mut anon_body = Vec::new();
                for stmt in block {
                    self.compile_expr(stmt, &mut anon_body)?;
                }
                anon_body.push(OpCode::PushNull);
                anon_body.push(OpCode::Ret);

                let anon_instructions: Vec<Instruction> = anon_body.into_iter().map(opcode_to_instruction).collect();
                self.functions.insert(func_name.clone(), Function {
                    params: vec![],
                    locals: vec![],
                    body: anon_instructions,
                });

                body.push(OpCode::PushStr(func_name));
                body.push(OpCode::Spawn);
            }
        }
        Ok(())
    }
}

use crush_vm::run_fastvm_with_caps;
use crush_vm::fastvm::{FastYield, Capability};
use crush_vm::{Arena, RuntimeValue};
use std::sync::Arc;

#[derive(Debug)]
struct IoPrintCapability;

impl Capability for IoPrintCapability {
    fn name(&self) -> &str {
        "io.print"
    }

    fn call(&self, _arena: &mut Arena, args: Vec<RuntimeValue>, _hal: Arc<dyn crush_vm::fastvm::Hal>) -> anyhow::Result<RuntimeValue> {
        if let Some(arg) = args.first() {
            println!("Sona print: {:?}", arg);
        }
        Ok(RuntimeValue::Null)
    }
}

/// Sona Executor running on FastVM path
pub struct SonaRuntime;

impl SonaRuntime {
    pub fn run(source: &str) -> Result<FastYield> {
        let mut parser = SonaParser::new(source);
        let ast = parser.parse()?;
        let casm_prog = SonaCompiler::compile(ast)?;

        let caps: Vec<Arc<dyn Capability>> = vec![Arc::new(IoPrintCapability)];

        // Run directly on the highly optimized FastVM execution loop
        run_fastvm_with_caps(&casm_prog, caps).map_err(|e| {
            anyhow!("FastVM execution error: {:?}", e)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper: any yield that is not an error counts as successful execution.
    // The FastVM legitimately yields back to the host for CapCall (io.print),
    // Spawn requests, etc. This is correct cooperative-multitasking behaviour.
    fn assert_clean_yield(result: FastYield) {
        assert!(
            !result.is_err(),
            "Sona program produced a FastVM error: {:?}",
            result
        );
    }

    #[test]
    fn test_sona_compile_and_run() {
        let source = "let x = 10 + 32\nsay(x)";
        let result = SonaRuntime::run(source).unwrap();
        println!("FastYield: {:?}", result);
        assert_clean_yield(result);
    }

    #[test]
    fn test_sona_parallel_spawn() {
        let source = "spawn { let a = 5\nsay(a) }";
        let result = SonaRuntime::run(source).unwrap();
        println!("FastYield: {:?}", result);
        assert_clean_yield(result);
    }

    #[test]
    fn test_sona_compiler_output() {
        // Verify the CASM output is structurally sound before even running
        let source = "let y = 100\nlet z = 200\nsay(y)";
        let mut parser = SonaParser::new(source);
        let ast = parser.parse().unwrap();
        let prog = SonaCompiler::compile(ast).unwrap();
        assert_eq!(prog.lang.as_deref(), Some("sona"));
        let main = prog.functions.get("main").unwrap();
        // expect: push_int(100), store(y), push_int(200), store(z),
        //         load(y), cap_call(io.print, 1), push_null, ret
        assert_eq!(main.body.len(), 8);
    }
}

