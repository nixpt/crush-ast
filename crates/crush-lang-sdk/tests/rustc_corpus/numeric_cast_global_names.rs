fn len(x: i64, _y: i64) -> i64 { x }
fn print(x: i64) -> i64 { x }
fn __crush_neg__(x: i64) -> i64 { x }
fn __crush_assign__(x: i64, _y: i64) -> i64 { x }
fn main() { println!("{} {} {} {}", len(300, 99) as u8, print(255) as i8, __crush_neg__(-1) as u8, __crush_assign__(300, 99) as u8); let v = vec![1,2,3]; println!("{}", v.len() as i64); }
