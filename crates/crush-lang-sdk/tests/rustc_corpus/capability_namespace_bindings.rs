fn main() {
    let mut array = vec![1];
    array.push(2);
    println!("{} {}", array.len(), array.pop().unwrap());
    let str = "ABC";
    println!("{}", str.to_lowercase());
    let math = 2i64;
    println!("{}", math.pow(3));
}
