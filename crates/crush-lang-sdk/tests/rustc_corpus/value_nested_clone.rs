fn main() {
    let a = vec![vec![1]];
    let mut b = a.clone();
    b[0].push(2);
    println!("{} {}", a[0].len(), b[0].len());
}
