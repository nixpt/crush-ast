fn main() {
    let __crush_assign__ = || 7;
    let print = || 8;
    let println = || 9;
    let len = || 10;
    println!("{} {} {}", print(), println(), len());
    let mut n = 1;
    n += 1;
    println!("{} {}", __crush_assign__(), n);
}
