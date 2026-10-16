fn tick(n: i64) -> i64 { println!("{}", n); n }
fn main() { println!("{1} {0}", tick(1), tick(2)); }
