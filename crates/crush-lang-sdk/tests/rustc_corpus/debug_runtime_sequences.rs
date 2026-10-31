fn main() {
    let c = 'é'; let s = "a\n";
    let chars = [c, '\0', '\'', '🦀'];
    let words = vec![s, "b\"", "\0"];
    let nested = [chars.clone(), ['x', 'y', 'z', 'w']];
    let nums: Vec<u8> = vec![0, 127, 255];
    let empty: Vec<i64> = vec![];
    println!("{:?} {:?}", chars, words);
    println!("{nested:?}");
    println!("{:?} {:?}", nums, empty);
    println!("{:?}", chars[0]);
}
