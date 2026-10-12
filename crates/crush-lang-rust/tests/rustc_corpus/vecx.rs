fn main() { let v = vec![1, 2, 3]; let mut s = 0; for x in v.iter() { s += x; } println!("{} {}", v.len(), s); let msg = format!("sum={}", s); println!("{}", msg); }
