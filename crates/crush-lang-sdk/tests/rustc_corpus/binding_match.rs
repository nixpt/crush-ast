fn main() {
    let n = 99;
    let result = match 3 { n => { let n = n + 1; n } };
    println!("{} {}", result, n);
}
