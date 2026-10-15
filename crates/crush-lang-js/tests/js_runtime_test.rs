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

#[test]
fn loose_equality_of_object_and_primitive_needs_to_primitive() {
    loud("console.log([1] == 1);", "ToPrimitive");
    loud("console.log('a' != {});", "ToPrimitive");
}

#[test]
fn console_log_format_directives_are_loud() {
    loud("console.log('%s and %d', 'a', 1);", "format directives");
    loud("console.log('100%%', 1);", "format directives");
}

#[test]
fn console_log_of_a_circular_array_is_loud() {
    loud("const a = []; a.push(a); console.log(a);", "circular");
}

#[test]
fn the_right_operand_is_not_evaluated_when_the_left_decides() {
    // `boom()` would print; node prints just the two values.
    let out = run("function boom() { console.log('boom'); return 1; }\nconsole.log(1 || boom(), 0 && boom());").unwrap();
    assert_eq!(out.trim_end(), "1 0");
}
