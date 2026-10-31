fn tick(n: i64) -> i64 { println!("{}", n); n }
fn first(a: i64, b: i64, c: i64) -> i64 { println!("{} {} {}", a, b, c); a }
fn main() { println!("{}", first(tick(1), tick(2), tick(3)) as u8); }
