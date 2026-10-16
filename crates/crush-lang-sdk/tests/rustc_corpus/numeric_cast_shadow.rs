fn main() {
    let n: i64 = 300;
    { let n: i16 = -1; println!("{}", n as u8); }
    { let mut n: i64 = 1; n += 1; println!("{}", n); }
    println!("{}", n as u8);
}
