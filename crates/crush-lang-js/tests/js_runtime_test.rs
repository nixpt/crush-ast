//! JS semantics the `js.*` runtime refuses must fail loudly on the VM, with a
//! message naming what isn't supported — never print something node wouldn't.
//! (What it does support is pinned by the node corpus.)

#![cfg(not(feature = "boa-backend"))]

fn run(source: &str) -> anyhow::Result<String> {
    let cast = crush_lang_js::js_to_cast(source, "js")?;
    let casm = crush_frontend::compiler::Compiler::new()
        .compile(cast)
        .map_err(|e| anyhow::anyhow!("CAST→CASM: {e}"))?;
    let vm = crush_lang_sdk::compile::casm_to_vm(&casm)?;
    Ok(crush_vm::run_with_caps(&vm, &crush_vm::Quotas::default(), None)?.output)
}

fn loud(source: &str, reason: &str) {
    match run(source) {
        Ok(out) => panic!("{source:?} printed {out:?}; expected a loud error about {reason}"),
        Err(e) => {
            let msg = format!("{e:#}");
            assert!(msg.contains("not supported") && msg.contains(reason), "{source:?}: {msg}");
        }
    }
}

#[test]
fn plus_with_an_object_needs_to_primitive() {
    loud("console.log('a' + [1, 2]);", "ToPrimitive");
    loud("console.log(`${{a: 1}}`);", "ToPrimitive");
    loud("let s = 'x'; s += {}; console.log(s);", "ToPrimitive");
}
