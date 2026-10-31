fn main() {
    let a: Vec<i64> = vec![];
    let mut b = a.clone();
    b.push(1);
    println!("{} {}", a.len(), b.len());
}
