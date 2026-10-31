fn main() { let mut a = vec![1]; let mut f = || { a = vec![2, 3]; }; f(); println!("{}", a.len()); }
