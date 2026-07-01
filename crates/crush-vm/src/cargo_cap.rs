use crate::host::{HostCap, HostCapSpec};
use crate::vm::Value;
use std::process::Command;

/// A capability to invoke the `cargo` command from within the Crush VM.
/// 
/// It expects:
/// 1. A string array of arguments (e.g. `["build", "--message-format=json"]`)
/// 2. An optional working directory string.
/// 
/// It returns a JSON string of the stdout output.
pub struct CargoCap;

impl HostCap for CargoCap {
    fn spec(&self) -> HostCapSpec {
        HostCapSpec {
            name: "os.cargo".to_string(),
            argc: Some(2),
            returns: true,
        }
    }

    fn call(&self, mut args: Vec<Value>) -> Result<Option<Value>, String> {
        let cwd = match args.pop() {
            Some(Value::Str(s)) => Some(s),
            Some(Value::Null) => None,
            _ => return Err("Expected string or null for cwd".to_string()),
        };

        let cmd_args = match args.pop() {
            Some(Value::Array(arr)) => {
                let mut out = Vec::new();
                for val in arr.borrow().iter() {
                    match val {
                        Value::Str(s) => out.push(s.clone()),
                        _ => return Err("Expected array of strings for cargo args".to_string()),
                    }
                }
                out
            }
            _ => return Err("Expected array of strings for cargo args".to_string()),
        };

        let mut command = Command::new("cargo");
        command.args(&cmd_args);
        
        if let Some(dir) = cwd {
            if !dir.is_empty() {
                command.current_dir(dir);
            }
        }

        match command.output() {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                let success = output.status.success();
                
                let mut map = std::collections::HashMap::new();
                map.insert("stdout".to_string(), Value::Str(stdout));
                map.insert("stderr".to_string(), Value::Str(stderr));
                map.insert("success".to_string(), Value::Bool(success));
                
                Ok(Some(Value::Map(std::rc::Rc::new(std::cell::RefCell::new(map)))))
            }
            Err(e) => Err(format!("Failed to execute cargo: {}", e)),
        }
    }
}
