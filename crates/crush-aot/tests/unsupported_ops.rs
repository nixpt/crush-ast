//! #72 / CRUSH-132: an op or capability the AOT backends can't translate is a
//! compile error naming it — not a no-op or a null stub that compiles and
//! then silently does nothing (a dropped `@python` block printed nothing).

fn casm(source: &str) -> casm::Program {
    crush_lang_sdk::compile::compile_crush_to_casm(source).expect("compiles to CASM")
}

const PYTHON_BLOCK: &str =
    "fn main() {\n  let base = 5\n  @python { result = base * 2 }\n  print(\"r=\" + result)\n}\n";

#[test]
fn python_block_is_rejected_by_both_backends() {
    let program = casm(PYTHON_BLOCK);
    for err in [
        crush_aot::codegen::gen_rust_source(&program).unwrap_err(),
        crush_aot::codegen_c::gen_c_source(&program).unwrap_err(),
    ] {
        assert_eq!(err.ops.len(), 1, "{err}");
        assert!(
            err.ops[0].starts_with("`exec_lang` (fn main, instruction "),
            "{err}"
        );
    }
}

#[test]
fn unknown_capability_is_rejected_by_both_backends() {
    let program = casm("fn main() { fs.mkdir(\"x\") }");
    for err in [
        crush_aot::codegen::gen_rust_source(&program).unwrap_err(),
        crush_aot::codegen_c::gen_c_source(&program).unwrap_err(),
    ] {
        assert!(err.to_string().contains("cap_call 'fs.mkdir'"), "{err}");
    }
}

#[test]
fn the_aot_compiler_surfaces_the_error() {
    let err = crush_aot::AotCompiler::new()
        .compile_casm(&casm(PYTHON_BLOCK), "unsupported_ops_test")
        .unwrap_err();
    assert!(
        err.to_string().contains("cannot compile: `exec_lang`"),
        "{err}"
    );
}

#[test]
fn supported_programs_still_compile() {
    let program = casm("fn add(a, b) { return a + b }\nfn main() { print(add(1, 2)) }");
    assert!(crush_aot::codegen::gen_rust_source(&program).is_ok());
    assert!(crush_aot::codegen_c::gen_c_source(&program).is_ok());
}

/// CRUSH-234: AI ops used to compile to an invented `{ok, kind, echo}` stub on
/// both backends; they are refused now, by name, in both spellings the
/// frontend emits (including `ai_tool_chain` / `ai_goal_decl`, which the old
/// codegen's long-name match never saw).
#[test]
fn ai_ops_are_rejected_by_both_backends() {
    use casm::{Function, Instruction};
    for op in [
        "ai_query", "ai_synthesize", "ai_agent_delegation", "ai_semantic_match",
        "ai_learning_loop", "ai_context_aware", "ai_toolchain", "ai_tool_chain",
        "ai_goal_declaration", "ai_goal_decl", "ai_progress_update", "ai_knowledge_sharing",
    ] {
        let mut program = casm("fn main() { print(1) }");
        let main: &mut Function = program.functions.get_mut("main").unwrap();
        main.body.insert(0, Instruction { op: op.into(), lang: None, meta: None, args: serde_json::json!({}) });
        for err in [
            crush_aot::codegen::gen_rust_source(&program).unwrap_err(),
            crush_aot::codegen_c::gen_c_source(&program).unwrap_err(),
        ] {
            assert!(err.to_string().contains(&format!("`{op}` (fn main, instruction 0)")), "{op}: {err}");
            assert!(err.to_string().contains("ai_native"), "{err}");
        }
    }
}

#[test]
fn ai_semantic_match_from_source_is_rejected() {
    let program = casm("fn main() { let m = ai_semantic_match(\"hi\", \"greeting\", 0.5); print(m) }");
    let err = crush_aot::codegen::gen_rust_source(&program).unwrap_err();
    assert!(err.to_string().contains("`ai_semantic_match`"), "{err}");
    let err = crush_aot::AotCompiler::new().compile_casm(&program, "ai_refuse").unwrap_err();
    assert!(err.to_string().contains("ai_semantic_match"), "{err}");
}
