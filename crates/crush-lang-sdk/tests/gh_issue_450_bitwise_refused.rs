//! GitHub issue #450 (part 1): native Crush source has no bitwise operators,
//! and `a &= 10` silently did nothing.
//!
//! The lexer turned a lone `&` into an identifier named `&`, so
//! `fn main() { let a = 12; a &= 10; print(a); }` parsed as `a;` followed by
//! an assignment to a variable called `&`, and printed `12`. `a & 10` failed
//! as "Undefined variable: &".
//!
//! Every bitwise operator and compound form is now a compile error. The ones
//! the lexer sees (`&`, `&=`, `^`, `^=`, `~`, `|=`) say that native Crush has
//! no bitwise operators. The others (`|`, `<<`, `>>`, `<<=`, `>>=`) were
//! already parse errors and stay so. Whether native Crush gets bitwise
//! operators is a language decision (#450 part 2); walker CAST compiles them
//! (#449), which this does not touch.

fn compile_err(source: &str) -> String {
    match crush_lang_sdk::compile::compile_crush_source(source) {
        Ok(_) => panic!("expected a compile error for:\n{source}"),
        Err(e) => format!("{e:#}"),
    }
}

fn program(stmt: &str) -> String {
    format!("fn main() {{ let a = 12; {stmt}; print(a); }}\n")
}

const NO_BITWISE: &str = "native Crush has no bitwise operators";

/// The issue's repro: a compile error, never a run that prints `12`.
#[test]
fn and_assign_is_refused() {
    let e = compile_err(&program("a &= 10"));
    assert!(e.contains("`&=`"), "{e}");
    assert!(e.contains(NO_BITWISE), "{e}");
    assert!(e.contains("line 1, column 27"), "{e}");
}

/// The operators and compound forms the lexer sees name themselves.
#[test]
fn lexer_level_forms_name_the_operator() {
    for (stmt, op) in [
        ("a &= 10", "`&=`"),
        ("a = a & 10", "`&`"),
        ("a ^= 10", "`^=`"),
        ("a = a ^ 10", "`^`"),
        ("a = ~a", "`~`"),
        ("a |= 10", "`|=`"),
    ] {
        let e = compile_err(&program(stmt));
        assert!(e.contains(op), "{stmt}: {e}");
        assert!(e.contains(NO_BITWISE), "{stmt}: {e}");
    }
}

/// Every other bitwise operator or compound form is a compile error too.
#[test]
fn every_bitwise_form_is_a_compile_error() {
    for stmt in [
        "a = a | 10",
        "a = a << 1",
        "a = a >> 1",
        "a <<= 1",
        "a >>= 1",
    ] {
        compile_err(&program(stmt));
    }
}

/// What a lone `&`, `^`, `~` or `|` must not break: `&&`, `||`, lambdas
/// (`|x| x`, `|a| => a`, `|| 1`), and the characters inside strings and
/// comments.
#[test]
fn neighbouring_syntax_is_unchanged() {
    let src = "fn main() {\n    let t = true && !false || false\n    let f = |x| x * 2\n    let g = |a| => a + 1\n    let h = || 7\n    // a & b, a ^ b, ~a, a |= b\n    print(t)\n    print(f(2) + g(1) + h())\n    print(\"a & b ^ ~c |= d\")\n}\n";
    let program = crush_lang_sdk::compile::compile_crush_source(src).expect("compiles");
    let out = crush_lang_sdk::Runtime::new()
        .with_host_caps(crush_lang_sdk::HostCapsBuilder::new().build())
        .run(&program)
        .expect("runs")
        .output;
    assert_eq!(out, "true\n13\na & b ^ ~c |= d\n");
}
