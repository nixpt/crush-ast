fn main() {
    let s = "A\0B".to_lowercase();
    println!("{} {}", s.len(), s.contains("\0"));
}
