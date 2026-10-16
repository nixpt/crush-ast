fn id(n: i64) -> i64 { n }
fn cast(n: i64) -> u8 { n as u8 }
fn signed(n: i64) -> i8 { n as i8 }
fn choose(flag: bool) -> i64 { if flag { -1 } else { 300 } }
fn main() {
    let a: i64 = 300;
    let b: i16 = -1;
    let c: u8 = 255;
    let d: i64 = 3_000_000_000;
    let e: i8 = -128;
    let f: u32 = 4294967295;
    println!("{} {} {} {} {} {}", a as u8, b as u8, c as i8, d as i32, e as i64, f as i32);
    println!("{} {} {}", id(-9223372036854775808) as u32, id(9223372036854775807) as u32, cast(300) as i64);
    println!("{} {} {}", choose(true) as u8, choose(false) as u8, (10i16 + 20) as u8);
    let byte: u8 = cast(300);
    println!("{}", byte / 3);
    let narrowed: i8 = signed(255);
    println!("{} {} {} {}", narrowed as i64, c as i8 as i64, b as u8 as i16, (a + 1) as u8);
}
