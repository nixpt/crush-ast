fn main() {
    let a = "hi".to_string();
    let mut b = a.clone();
    b.push_str("!");
    println!("{} {}", a, b);
}
