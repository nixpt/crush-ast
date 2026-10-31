// Closures: typed and untyped parameters, a parameter shadowing an outer
// variable, a capture, `_` parameters, closures passed to functions.
fn apply(f: impl Fn(i32) -> i32, v: i32) -> i32 {
    f(v)
}

fn main() {
    let x = 5;
    let add_typed = |x: i32| x + 1;
    let add_untyped = |y| y + 1;
    let scale = 3;
    let times = |v: i32| v * scale;
    let first = |a: i32, _: i32| a;
    println!("{}", add_typed(41));
    println!("{}", add_untyped(41));
    println!("{}", times(7));
    println!("{}", first(9, 100));
    println!("{}", apply(add_typed, 1));
    println!("{}", x);
    // Block bodies: statements before the tail, `return`, a nested block.
    let bump = |mut v: i32| {
        v += 1;
        v
    };
    let early = |v: i32| {
        if v > 10 {
            return 100;
        }
        v * 2
    };
    let with_let = |v: i32| {
        let w = v * 10;
        let z = w + 1;
        z
    };
    println!("{}", bump(1));
    println!("{}", early(3));
    println!("{}", early(30));
    println!("{}", with_let(4));
}
