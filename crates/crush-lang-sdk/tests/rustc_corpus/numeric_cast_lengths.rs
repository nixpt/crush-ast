fn length(v: Vec<i64>) -> i64 { v.len() as i64 }
fn bytes(s: &str) -> usize { s.len() }
fn widen(n: usize) -> u64 { n as u64 }
fn main() {
    let v = vec![5i64, 6, 7];
    println!("{} {} {} {}", v.len() as i32, v.len() as i64, v.len() as u32, v.len() as u64);
    for i in 0..3 { println!("{}", v[i as usize]); }
    let mut sum = 0;
    for i in 0..v.len() { sum += v[i as usize]; }
    println!("{} {}", sum, length(v.clone()));
    let s = "é🦀";
    println!("{} {}", bytes(s) as i64, widen(bytes(s)));
    let i: i64 = 1;
    println!("{} {}", v[i as usize], v[1i64 as usize]);
}
