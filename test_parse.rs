use std::collections::HashMap;

fn main() {
    let input = "port: 8080  # the port\n";
    let end = input.find(|c: char| c.is_whitespace() || c == ',' || c == '}' || c == ']' || c == '~').unwrap_or(input.len());
    println!("end = {}", end);
    println!("val = {:?}", &input[..end]);
}
