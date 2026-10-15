fn main() {
    let x = 100;
    let sub = |x: i64, y: i64| x - y;
    println!("{} {}", sub(50, 8), x);
    let offset = 40;
    let add = |x: i64| offset + x;
    println!("{}", add(2));
}
