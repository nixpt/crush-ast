fn main() { let mut i = 0; let mut odd = 0; loop { i += 1; if i % 2 == 0 { continue; } odd += i; if i >= 9 { break; } } println!("{} {}", i, odd); }
