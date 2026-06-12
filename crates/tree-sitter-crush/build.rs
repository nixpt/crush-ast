fn main() {
    println!("cargo:rerun-if-changed=src/parser.c");
    println!("cargo:rerun-if-changed=src/grammar.json");
    let src_dir = std::path::Path::new("src");
    let mut config = cc::Build::new();
    config.include(src_dir);
    config.file(src_dir.join("parser.c"));
    config.compile("tree-sitter-crush");
}
