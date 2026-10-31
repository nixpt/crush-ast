fn main() {
    let mut x = 1;
    { x += 1; let mut x = 10; x += 1; println!("{x}"); }
    println!("{x}");
    let len = 100;
    let xs = vec![1, 2];
    println!("{} {}", xs.len(), len);
}
