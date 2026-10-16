fn cast(n: u8) -> i64 { n as i64 }
fn bump(n: u8) -> u8 { n + 10 }
fn main() { println!("{}", cast(bump(250))); }
