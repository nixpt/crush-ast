fn main() {
    let a = [1, 2, 3];
    let b = a.clone();
    let mut v = a.to_vec();
    v.push(4);
    println!("{} {} {} {}", a.len(), b.len(), b[1], v.len());
}
