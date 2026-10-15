fn main() {
    let x = 1;
    let x = x + 1;
    println!("{x}");
    { let x = x + 1; println!("{x}"); }
    println!("{x}");
    let __rust_binding_0 = 99;
    println!("{}", __rust_binding_0);
}
