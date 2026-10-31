fn main() {
    let mut a = vec![1, 2, 3];
    let mut b = a.clone().clone();
    b.push(4);
    a.push(5);
    a.push(6);
    println!("{} {} {} {}", a.len(), b.len(), a[3], b[3]);
}
