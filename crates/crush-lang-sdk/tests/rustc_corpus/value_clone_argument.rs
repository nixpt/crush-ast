fn grow(mut v: Vec<i64>) -> usize { v.push(9); v.len() }
fn fresh() -> Vec<i64> { vec![7, 8] }
fn main() {
    let a = vec![1, 2, 3];
    println!("{} {} {}", grow(a.clone()), a.len(), grow(fresh()));
}
