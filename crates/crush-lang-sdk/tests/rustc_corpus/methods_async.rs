async fn deferred(x: f64) -> f64 { println!("deferred"); x.sqrt() }
fn main() { let _future = deferred(4.0); }
