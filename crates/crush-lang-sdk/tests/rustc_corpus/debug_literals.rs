fn main() {
    println!("{:?} {:?} {:?} {:?}", "", "a\"b\\c\n\t\0", "é🦀", 'é');
    println!("{:?} {:?} {:?} {:?}", '\n', '\'', '"', '\0');
    println!("{:?} {:?} {:?} {:?}", true, false, (), -128i8);
    println!("{:?} {:?} {:?} {:?}", 255u8 as i8, 6.0f64, -0.0f64, 1e21f64);
    println!("{:?} {:?}", ["a", "b\n"], [['x', '\t'], ['é', '\0']]);
    println!("{} {:?} {0} {1:?}", "display", 'x');
    let s: String = format!("{:?}", "hello");
    println!("{}", s);
    let empty: String = format!("");
    println!("empty={}", empty);
}
