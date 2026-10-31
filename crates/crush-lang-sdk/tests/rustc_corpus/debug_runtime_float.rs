fn scale(x: f64) -> f64 { x * 2.0 }
fn overflow(x: f64) -> f64 { x * x }
fn main() {
    let x = 1.5f64 * 4.0;
    let huge = overflow(1e308f64);
    let nan = huge - huge;
    println!("{x:?} {huge:?} {nan:?}");
    println!("{:?}", scale(-0.0));
}
