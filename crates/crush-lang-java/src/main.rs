use anyhow::Result;
use clap::Parser as ClapParser;
use crush_walker_core::run_walker_binary;

#[derive(ClapParser)]
#[command(name = "crush_lang_java")]
struct Cli {
    input: String,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // CRUSH-37 Commit 1 (skeleton): JavaWalker is registered with
    // an `unreachable!()` Language stub (per the Sub-Commit 2
    // macro test pattern). Real Java parsing requires the
    // `tree-sitter-java` workspace dep, which is the CRUSH-37
    // Commit 2 follow-up. The binary compiles + loads but the
    // parse() call will panic at runtime until the real grammar
    // binding is added.
    run_walker_binary(
        crush_lang_java::JavaWalker { file_name: cli.input.clone() },
        "java",
        &[".java"],
        &cli.input,
    )
}
