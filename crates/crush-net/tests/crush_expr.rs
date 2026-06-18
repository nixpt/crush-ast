//! Crush-bytecode integration test: a tiny capsule calls `net.ping`, runs
//! through the SDK runtime, and asserts the top-of-stack is the literal
//! `"pong"` Value.
//!
//! Confirmed SDK signatures (verified against `crush-vm/src/{assembler,vm}.rs`):
//!   `crush_lang_sdk::assemble(source, permissions, name) -> Result<Program, _>`
//!   `crush_lang_sdk::run_with_caps(program, quotas, Option<&HostCaps>) -> Result<VmResult, _>`
//!
//! `VmResult` exposes `stack: Vec<Value>`; the cap return value lives on the
//! stack (not in `output`, which is empty unless the capsule called `io.print`).

use crush_lang_sdk::{HostCaps, HostCapsBuilder, Quotas, Value};

#[test]
fn capsule_calls_net_ping_returns_pong() {
    // Tiny capsule: push "ping" then CAP_CALL net.ping (which returns "pong"),
    // then HALT. The VM leaves the return value on the stack.
    let src = r#"
.func main
PUSH_STR "ping"
CAP_CALL "net.ping" 1
HALT
"#;

    // permissions list must include `net.ping` for the cap to fire.
    let program =
        crush_lang_sdk::assemble(src, Some(&["net.ping"]), Some("net_ping_capsule"))
            .expect("crush assembler accepts the capsule");

    let mut caps: HostCaps = HostCapsBuilder::new()
        .build();
    crush_net::register(&mut caps);

    // run_with_caps signature is (&Program, &Quotas, Option<&HostCaps>).
    let result = crush_lang_sdk::run_with_caps(
        &program,
        &Quotas::default(),
        Some(&caps),
    )
    .expect("crush runtime executes the capsule successfully");

    // Defense-in-depth: ensure HALT actually ran.
    assert!(result.halted, "expected HALT to fire; got {:?}", result);

    // Cap return value is on the stack; output is empty unless io.print was called.
    let top = result
        .stack
        .last()
        .expect("vm left at least one value on the stack at HALT");
    assert!(
        matches!(top, Value::Str(s) if s == "pong"),
        "expected top-of-stack to be Value::Str(\"pong\"); got {:?}; full stack={:?}; output={:?}",
        top,
        result.stack,
        result.output,
    );
}
