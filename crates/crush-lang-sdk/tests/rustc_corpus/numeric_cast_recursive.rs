fn recurse(n: i64) -> i64 { if n == 0 { 0 } else { recurse(n - 1) } }
fn main() { println!("{}", recurse(3) as u8); }
