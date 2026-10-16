fn __crush_rust_int_cast__(x: i64, width: i64, signed: bool) -> i64 { if signed { x + width } else { x } }
fn main() { println!("{}", __crush_rust_int_cast__(1, 8, true)); }
