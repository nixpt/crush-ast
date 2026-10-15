fn main() {
    let x = 10;
    let mut total = 0;
    if true { let x = 2; total += x; println!("{x}"); }
    else { let x = 20; println!("{x}"); }
    while total < 4 { let x = total; println!("{x}"); total += 1; }
    for x in 0..3 { println!("{x}"); }
    println!("{} {}", x, total);
}
