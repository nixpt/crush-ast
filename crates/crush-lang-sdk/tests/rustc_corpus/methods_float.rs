fn power(x: f64) -> f64 { x.powi(-3) }
fn root(x: f64) -> f64 { x.powf(1.5) }
fn main() {
    println!("{} {}", power(2.5), root(2.5));
    println!("{} {} {} {}", (-2.5f64).abs(), 2.25f64.sqrt(), (-0.5f64).ceil(), (-0.5f64).round());
    println!("{} {} {}", (-1.25f64).floor(), (-0.0f64).abs(), 0.0f64.powi(-1));
    println!("{} {}", (-1.0f64).sqrt(), (-0.0f64).powi(3));
    let huge = 1e308f64 * 1e308f64;
    let nan = huge - huge;
    println!("{} {}", huge.sqrt(), nan.abs());
}
