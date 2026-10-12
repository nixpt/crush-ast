fn name(x: i64) -> String { match x { 1 => "one".to_string(), 2 | 3 => "few".to_string(), _ => "many".to_string() } }
fn main() { let x = 2; let y = match x { 1 => 10, 2 => 20, _ => 0 }; println!("{}", y); println!("{} {} {}", name(1), name(3), name(9)); }
