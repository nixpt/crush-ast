fn main() {
    let a = vec![1, 2];
    let mut b = a.as_slice().to_vec();
    let mut c = a.to_owned();
    b.push(3);
    c.push(4);
    c.push(5);
    let s = "text".to_owned();
    let mut t = s.as_str().to_owned();
    t += "!";
    println!("{} {} {} {} {}", a.len(), b.len(), c.len(), s, t);
}
