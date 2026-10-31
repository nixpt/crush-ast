fn pick(n: i64) -> i64 { let y = if n > 5 { 1 } else { 2 }; y }
fn main() { println!("{} {}", pick(9), pick(3)); }
