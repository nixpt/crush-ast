fn decorate(mut s: String) -> String { s += "."; s.clone() }
fn main() {
    let a = "seed".to_string();
    let mut b = a.clone();
    b += "!";
    let c = b.clone() + "?";
    println!("{} {} {} {}", a, b, c, decorate(a.clone()));
}
