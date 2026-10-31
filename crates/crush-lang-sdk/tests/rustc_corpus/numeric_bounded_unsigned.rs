fn narrow(n: u64) -> u8 { n as u8 }
fn main() {
    let a = 5u64;
    let b: u64 = 6;
    let c = 5usize;
    println!("{} {} {} {}", a + 1, b * 7, c + 1, a as usize);
    let v: Vec<u64> = vec![3, 4];
    let x = v[1];
    println!("{} {}", x + 1, narrow(300));
}
