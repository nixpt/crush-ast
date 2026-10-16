fn lower(s: &str) -> String { s.to_lowercase() }
fn main() {
    let original = "Straße ΟΣ";
    let lowered = lower(original);
    println!("{}", lowered);
    println!("{}", original.to_uppercase());
    println!("{}", "\u{a0}λ\u{2003}".trim());
    println!("{}", "aba".replace("", "λ"));
    println!("{} {} {}", lowered.contains("ß"), lowered.starts_with("str"), lowered.ends_with("ος"));
    println!("{} {} {}", "".contains(""), "λ".starts_with(""), "λ".ends_with("x"));
    let mut i = 0;
    while i < 120 { let s = "ΟΣ Straße".to_lowercase(); if i == 119 { println!("{}", s); } i += 1; }
}
