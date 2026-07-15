use anyhow::Result;
use clap::Parser as ClapParser;
use crush_walker_core::run_walker_binary;

#[derive(ClapParser)]
#[command(name = "crush_lang_go")]
struct Cli {
    input: String,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    run_walker_binary(
        crush_lang_go::GoWalker { file_name: cli.input.clone() },
        "go",
        &[".go"],
        &cli.input,
    )
}
