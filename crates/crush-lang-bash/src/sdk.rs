//! Bash SDK — full pipeline from bash source to CVM1 execution.
#![cfg(test)]

use walker_core::AdapterRegistry;

pub fn run_bash(source: &str) -> anyhow::Result<String> {
    let adapter = crate::BashAdapter;
    let (_, cast) = adapter.walk(source, "test.sh").map_err(|e| anyhow::anyhow!("bash->CAST: {e}"))?;
    let mut compiler = crush_frontend::compiler::Compiler::new();
    let casm = compiler.compile(cast).map_err(|e| anyhow::anyhow!("CAST->CASM: {e}"))?;
    let vm_prog = crush_lang_sdk::compile::casm_to_vm(&casm).map_err(|e| anyhow::anyhow!("CASM->CVM1: {e}"))?;
    use crush_vm::host::{HostCap, HostCapSpec, HostCaps};
    let mut host_caps = HostCaps::new();
    struct NopCap { name: String }
    impl HostCap for NopCap { fn spec(&self) -> HostCapSpec { HostCapSpec { name: self.name.clone(), argc: None, returns: true } }
        fn call(&self, _: Vec<crush_vm::vm::Value>) -> Result<Option<crush_vm::vm::Value>, String> { Ok(Some(crush_vm::vm::Value::Null)) }
    }
    for name in &["__crush_assign__", "append", "push"] { host_caps.register(Box::new(NopCap { name: name.to_string() })); }
    let quotas = crush_vm::vm::Quotas { max_steps: 10_000_000, ..Default::default() };
    let result = crush_vm::vm::run_with_caps(&vm_prog, &quotas, Some(&host_caps)).map_err(|e| anyhow::anyhow!("CVM1: {e}"))?;
    Ok(result.output.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bash_variable() {
        let src = "x=42; echo $x";
        let r = run_bash(src);
        assert!(r.is_ok(), "bash should compile and run: {r:?}");
    }

    #[test]
    fn test_bash_adapter_registry() {
        let mut r = AdapterRegistry::new();
        r.register(Box::new(crate::BashAdapter));
        assert!(r.walk("x=1", "test.sh").is_ok());
        assert!(r.walk("x=1", "test.py").is_err());
    }
}
