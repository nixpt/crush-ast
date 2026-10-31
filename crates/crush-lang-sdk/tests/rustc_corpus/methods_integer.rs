fn signed(x: i8) -> i8 { x.pow(3) }
fn magnitude(x: i8) -> i8 { x.abs() }
fn power(x: i64, exponent: u32) -> i64 { x.pow(exponent) }
fn main() {
    let a = signed(6);
    println!("{} {}", a, a / 2);
    println!("{} {}", magnitude(-128), (-7i64).abs() / 3);
    println!("{} {}", 20u8.pow(2), 2i64.pow(3) / 3);
    println!("{} {} {}", power(-2, 3), power(3, 0), 2u64.pow(3));
    println!("{} {} {}", 2usize.pow(3), 65535u16.pow(2), 123456i32.pow(3));
    println!("{} {} {}", 2i64.pow(3) / 3 == 2, 20u8.pow(2) > 100, 2i64.pow(3) % 3);
}
