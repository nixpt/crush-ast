fn f(n: i64) -> i64 { n + 1 }
fn g(n: i64) -> i64 { if n > 0 { n * 2 } else { 0 - n } }
fn main() { println!("{} {} {}", f(4), g(3), g(-5)); }
