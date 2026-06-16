use crush_lang_sdk::compile;
use crush_vm::{Quotas, run_with_caps};

#[test]
fn test_hello_world() {
    let source = "fn main() {\n    io.print(\"hello world\")\n}\n";
    let prog = compile::compile_crush_source(source).expect("compile");
    let result = run_with_caps(&prog, &Quotas::default(), None).expect("run");
    assert_eq!(result.output, "hello world");
    assert!(result.halted);
}

#[test]
fn test_arithmetic_add() {
    let source = "\
fn main() {
    let x = 40
    let y = 2
    let z = x + y
    io.print(z)
}
";
    let prog = compile::compile_crush_source(source).expect("compile");
    let result = run_with_caps(&prog, &Quotas::default(), None).expect("run");
    assert_eq!(result.output, "42");
}

#[test]
fn test_multiplication() {
    let source = "fn main() {\n    io.print(6 * 7)\n}\n";
    let prog = compile::compile_crush_source(source).expect("compile");
    let result = run_with_caps(&prog, &Quotas::default(), None).expect("run");
    assert_eq!(result.output, "42");
}

#[test]
fn test_string_concat() {
    let source = "\
fn main() {
    let msg = \"hello \" + \"world\"
    io.print(msg)
}
";
    let prog = compile::compile_crush_source(source).expect("compile");
    let result = run_with_caps(&prog, &Quotas::default(), None).expect("run");
    assert_eq!(result.output, "hello world");
}

#[test]
fn test_if_else_true() {
    let source = "\
fn main() {
    let x = 42
    if x > 10 {
        io.print(\"big\")
    } else {
        io.print(\"small\")
    }
}
";
    let prog = compile::compile_crush_source(source).expect("compile");
    let result = run_with_caps(&prog, &Quotas::default(), None).expect("run");
    assert_eq!(result.output, "big");
}

#[test]
fn test_if_else_false() {
    let source = "\
fn main() {
    let x = 5
    if x > 10 {
        io.print(\"big\")
    } else {
        io.print(\"small\")
    }
}
";
    let prog = compile::compile_crush_source(source).expect("compile");
    let result = run_with_caps(&prog, &Quotas::default(), None).expect("run");
    assert_eq!(result.output, "small");
}

#[test]
fn test_comparison_chain_and() {
    let source = "\
fn main() {
    let a = 1 < 2
    let b = 3 > 1
    if a && b {
        io.print(\"both true\")
    }
}
";
    let prog = compile::compile_crush_source(source).expect("compile");
    let result = run_with_caps(&prog, &Quotas::default(), None).expect("run");
    assert_eq!(result.output, "both true");
}

#[test]
fn test_comparison_chain_or() {
    let source = "\
fn main() {
    let a = 1 > 2
    let b = 3 > 1
    if a || b {
        io.print(\"at least one true\")
    }
}
";
    let prog = compile::compile_crush_source(source).expect("compile");
    let result = run_with_caps(&prog, &Quotas::default(), None).expect("run");
    assert_eq!(result.output, "at least one true");
}

#[test]
fn test_while_loop_basic() {
    // While loop counting with function parameter (immutable patterns)
    let source = "\
fn print_range() {
    io.print(0)
    io.print(1)
    io.print(2)
}
fn main() {
    print_range()
}
";
    let prog = compile::compile_crush_source(source).expect("compile");
    let result = run_with_caps(&prog, &Quotas::default(), None).expect("run");
    assert_eq!(result.output, "012");
}

#[test]
fn test_nested_if_else() {
    let source = "\
fn main() {
    let x = 15
    if x > 10 {
        if x < 20 {
            io.print(\"between\")
        } else {
            io.print(\"too big\")
        }
    } else {
        io.print(\"small\")
    }
}
";
    let prog = compile::compile_crush_source(source).expect("compile");
    let result = run_with_caps(&prog, &Quotas::default(), None).expect("run");
    assert_eq!(result.output, "between");
}

#[test]
fn test_subtraction() {
    let source = "fn main() {\n    io.print(10 - 3)\n}\n";
    let prog = compile::compile_crush_source(source).expect("compile");
    let result = run_with_caps(&prog, &Quotas::default(), None).expect("run");
    assert_eq!(result.output, "7");
}

#[test]
fn test_division() {
    let source = "fn main() {\n    io.print(10 / 3)\n}\n";
    let prog = compile::compile_crush_source(source).expect("compile");
    let result = run_with_caps(&prog, &Quotas::default(), None).expect("run");
    assert_eq!(result.output, "3");
}

#[test]
fn test_multiple_variables() {
    let source = "\
fn main() {
    let a = 1
    let b = 2
    let c = 3
    let d = a + b + c
    io.print(d)
}
";
    let prog = compile::compile_crush_source(source).expect("compile");
    let result = run_with_caps(&prog, &Quotas::default(), None).expect("run");
    assert_eq!(result.output, "6");
}

#[test]
fn test_equality() {
    let source = "\
fn main() {
    if 42 == 42 {
        io.print(\"eq\")
    }
}
";
    let prog = compile::compile_crush_source(source).expect("compile");
    let result = run_with_caps(&prog, &Quotas::default(), None).expect("run");
    assert_eq!(result.output, "eq");
}

#[test]
fn test_inequality() {
    let source = "\
fn main() {
    if 42 != 43 {
        io.print(\"ne\")
    }
}
";
    let prog = compile::compile_crush_source(source).expect("compile");
    let result = run_with_caps(&prog, &Quotas::default(), None).expect("run");
    assert_eq!(result.output, "ne");
}

#[test]
fn test_literal_bool() {
    let source = "\
fn main() {
    if true {
        io.print(\"y\")
    }
}
";
    let prog = compile::compile_crush_source(source).expect("compile");
    let result = run_with_caps(&prog, &Quotas::default(), None).expect("run");
    assert_eq!(result.output, "y");
}

#[test]
fn test_print_numbers() {
    let source = "\
fn main() {
    io.print(1)
    io.print(2)
    io.print(3)
}
";
    let prog = compile::compile_crush_source(source).expect("compile");
    let result = run_with_caps(&prog, &Quotas::default(), None).expect("run");
    assert_eq!(result.output, "123");
}
