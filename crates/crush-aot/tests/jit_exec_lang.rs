//! #73 / CRUSH-133: the JIT compiled `ExecLang` to a host-request yield that
//! carried only a tag — no language, code or variables — and a string result
//! resumed as null. It now refuses to compile `ExecLang`, so `JitEngine`
//! falls back to FastVM and the host gets the same full request FastVM
//! yields.

use crush_vm::fastvm::{FastVM, FastYield, Hal, HostRequest};
use crush_vm::value::RuntimeValue;

#[derive(Debug)]
struct NoHal;
impl Hal for NoHal {}

fn lowered(source: &str) -> crush_vm::fastvm::LoweredProgram {
    let casm = crush_lang_sdk::compile::compile_crush_to_casm(source).expect("compiles");
    crush_vm::fastvm::lower_program(&casm).expect("lowers")
}

/// (lang, code, variables) of an ExecLang request, or a panic naming the tier.
fn python_request(outcome: FastYield, tier: &str) -> (String, String, String) {
    match outcome {
        FastYield::Request(HostRequest::ExecLang {
            lang,
            code,
            variables,
        }) => {
            assert_eq!(lang, "python", "{tier}");
            assert!(code.contains("base * 2"), "{tier}: code = {code:?}");
            let mut vars: Vec<_> = variables
                .iter()
                .map(|(k, v)| format!("{k}={v:?}"))
                .collect();
            vars.sort();
            (lang, code, vars.join(","))
        }
        other => panic!("{tier}: expected an ExecLang request, got {other:?}"),
    }
}

#[test]
fn jit_yields_the_same_exec_lang_request_as_fastvm() {
    let program = lowered(
        "fn main() {\n  let base = 5\n  @python { result = base * 2 }\n  print(\"r=\" + result)\n}\n",
    );

    let mut fastvm = FastVM::new(program.clone(), Vec::new(), std::sync::Arc::new(NoHal));
    let from_fastvm = python_request(fastvm.run(1_000_000), "FastVM");

    let jit = crush_jit::JitEngine::new().expect("JitEngine::new");
    let from_jit = python_request(jit.run(&program).expect("jit runs"), "JIT");

    // Same request as FastVM. (FastVM itself doesn't marshal the block's
    // input variables — noted on CRUSH-133 — so this pins agreement, not a
    // particular variable set.)
    assert_eq!(from_jit, from_fastvm);
}
