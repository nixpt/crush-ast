fn main() {
    let a = vec![1, 2];
    let mut b = if true { a.clone() } else { vec![9] };
    b.push(3);
    let s = "hi".to_string();
    let t = (if true { s.clone() } else { "no".to_string() }) + "!";
    println!("{} {} {} {}", a.len(), b.len(), s, t);
}
