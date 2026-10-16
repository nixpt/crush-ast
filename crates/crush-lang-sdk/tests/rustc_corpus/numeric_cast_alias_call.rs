fn cast(n: u8) -> i64 { n as i64 }
fn bump(n: u8) -> u8 { n + 10 }
fn main() { let f = cast; println!("{}", f(bump(250))); }
