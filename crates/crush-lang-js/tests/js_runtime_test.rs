//! JS semantics the `js.*` runtime refuses must fail loudly on the VM, with a
//! message naming what isn't supported — never print something node wouldn't.
//! (What it does support is pinned by the node corpus.)

// `js_to_cast(.., "js")` is the swc lowering while `swc-backend` is on.
#![cfg(feature = "swc-backend")]

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
fn strict_equality_never_needs_to_primitive() {
    // `==` is loud here (see above); `===` is simply false — no coercion.
    let out = run("console.log([1] === 1, 'a' !== {}, null === 0, 1 === '1', 1 === 1, 'a' === 'a');").unwrap();
    assert_eq!(out.trim_end(), "false true false false true true");
}

/// The checked door (CRUSH-290): `compile_cast` runs the semantics checker,
/// which rejected the bare `BinaryOp "==="` this used to lower to.
#[test]
fn strict_equality_compiles_through_the_checked_door() {
    let cast = crush_lang_js::js_to_cast(
        "function f(x) { return x === null; }\nconsole.log(1 === 1, 1 === '1', f(null), f(0), 1 !== '1');",
        "js",
    )
    .unwrap();
    let casm = crush_frontend::compile_cast(&cast).expect("compile_cast (checked door)");
    let vm = crush_lang_sdk::compile::casm_to_vm(&casm).unwrap();
    let out = crush_vm::run_with_caps(&vm, &crush_vm::Quotas::default(), None).unwrap().output;
    assert_eq!(out.trim_end(), "true false true false true");
}

/// CRUSH-290: a `switch` case test is strict equality — its generated
/// comparison is the `js.strict_eq` capability, so a switch also compiles
/// through `compile_cast`. Expected output is what `node` prints.
#[test]
fn switch_case_matching_compiles_through_the_checked_door() {
    let source = r#"
const n = 2;
switch (n) {
  case 1: console.log('one'); break;
  case 2: console.log('two'); break;
  default: console.log('many');
}
const s = 'b';
switch (s) {
  case 'a': console.log('A');
  case 'b': console.log('B');
  case 'c': console.log('C'); break;
  default: console.log('D');
}
switch ('3') { case 3: console.log('number 3'); break; default: console.log('not coerced'); }
switch (n) { case 9: console.log('nine'); break; default: console.log('default only'); }
"#;
    let cast = crush_lang_js::js_to_cast(source, "js").unwrap();
    let casm = crush_frontend::compile_cast(&cast).expect("compile_cast (checked door)");
    let vm = crush_lang_sdk::compile::casm_to_vm(&casm).unwrap();
    let out = crush_vm::run_with_caps(&vm, &crush_vm::Quotas::default(), None).unwrap().output;
    assert_eq!(out, "two\nB\nC\nnot coerced\ndefault only\n");
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
