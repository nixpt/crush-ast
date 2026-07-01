use std::fs;
use std::env;
use crush_frontend::parser::cson::parse_cson;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <path-to-.cson-file>", args[0]);
        std::process::exit(1);
    }

    let file_path = &args[1];
    let content = match fs::read_to_string(file_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to read file '{}': {}", file_path, e);
            std::process::exit(1);
        }
    };

    println!("Parsing CSON file: {}", file_path);
    match parse_cson(&content) {
        Ok(ast) => {
            let json_ast = serde_json::to_string_pretty(&ast).unwrap();
            println!("AST Output:\n{}", json_ast);
        }
        Err(e) => {
            eprintln!("Failed to parse CSON: {:?}", e);
            std::process::exit(1);
        }
    }
}
