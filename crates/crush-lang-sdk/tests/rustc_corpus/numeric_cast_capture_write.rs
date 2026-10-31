fn main() {
    let mut n = 300i64;
    let _bump = || { n += 1; };
    println!("{}", n as u8);
}
