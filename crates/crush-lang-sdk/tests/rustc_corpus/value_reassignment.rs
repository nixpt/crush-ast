fn main() {
    let a = vec![1, 2, 3];
    let mut b = a.clone();
    b = vec![9];
    b.push(8);
    println!("{} {} {}", a.len(), b.len(), b[0]);
}
