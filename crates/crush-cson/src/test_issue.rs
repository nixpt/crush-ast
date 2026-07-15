use crate::parser::CsonParser;
fn main() {
    let mut parser = CsonParser::new("port: 8080  # the port\n@module { purpose: \"parse a, b, c\" }\nmsg: \"he said \\\"hi\\\"\"");
    println!("{:#?}", parser.parse());
}
