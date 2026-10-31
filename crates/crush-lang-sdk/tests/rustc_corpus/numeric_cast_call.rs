fn value() -> i64 { println!("called"); 300 }
fn main() { println!("{}", value() as u8); }
