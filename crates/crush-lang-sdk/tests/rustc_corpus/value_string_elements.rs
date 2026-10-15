fn main() {
    let a = vec!["one".to_string()];
    let mut b = a.clone();
    b.push("two".to_string());
    let mut c = a.as_slice().to_vec();
    c.push("three".to_owned());
    println!("{} {} {} {} {}", a.len(), b.len(), c.len(), a[0], b[1]);
}
