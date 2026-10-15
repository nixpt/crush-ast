fn value() -> i64 { 8 }
fn main() {
    let initial = value();
    { let value = || 9; println!("{}", value()); }
    println!("{} {}", value(), initial);
    let x = 1;
    let f = || x;
    { let x = 2; let g = || x; println!("{} {}", f(), g()); }
    let x = 3;
    println!("{} {}", f(), x);
    let f = |x: i64| x + 1;
    { let f = |x: i64| x + 10; println!("{}", f(2)); }
    println!("{}", f(2));
    let offset = 4;
    let apply = |x: i64| { let x = x + offset; x };
    println!("{}", apply(2));
}
