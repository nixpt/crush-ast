fn char_id(c: char) -> char { c }
fn text(s: &str) -> String { format!("{}!", s) }
fn integer(n: u8) -> u8 { n }
fn unit() { println!("unit"); }
fn choose(flag: bool) -> char { if flag { 'x' } else { 'é' } }
fn main() {
    let s = "é\"\\\n\t\0";
    let c = 'é'; let copy = c;
    let yes = true; let zero = -0.0f64;
    let n: u8 = 255;
    println!("{s:?} {c:?} {copy:?} {yes:?} {zero:?} {n:?}");
    println!("{:?} {:?} {:?}", char_id(c), text("hello"), integer(n));
    let u = ();
    println!("{:?} {:?}", u, unit());
    println!("{:?}", choose(false));
    let shadow = 'é';
    if true { let shadow = "a"; println!("{shadow:?}"); }
    println!("{shadow:?}");
    let empty = format!("");
    println!("{:?}", empty);
}
