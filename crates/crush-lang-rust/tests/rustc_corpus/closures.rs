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
}
