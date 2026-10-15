#[derive(Clone, Copy)]
struct Point { x: i64 }
fn main() { let a = Point { x: 1 }; let mut b = a; b.x = 2; println!("{} {}", a.x, b.x); }
