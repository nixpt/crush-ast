fn main() {
    println!("{} {} {} {} {}", 300i64 as u8, -1i16 as u8, 255u8 as i8, 3_000_000_000i64 as i32, -128i8 as i64);
    println!("{} {} {} {} {}", 4294967295u32 as i64, (255u8 as i8) as i64, -9223372036854775808i64 as u32, 0xffffu16 as i16, 127 as i8);
    println!("{} {} {}", 2147483648 as i64, 4294967295 as u32, -128 as i8);
    println!("{} {} {} {}", 65535u16 as i8, 65535u16 as i16, -32768i16 as u16, 9223372036854775807i64 as u32);
    let a = 255u8 as i8 as i64;
    let b = 300i64 as u8;
    println!("{} {}", a / 2, b / 3);
}
