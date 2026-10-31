fn main() {
    let a = vec![1, 2, 3];
    let mut b = a.clone();
    b.push(4);
    println!("{} {}", a.len(), b.len());
}
