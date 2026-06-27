use anyhow::Result;
use clap::Parser as ClapParser;
use walker_core::run_walker_binary;

#[derive(ClapParser)]
#[command(name = "go_walker")]
struct Cli {
    input: String,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    run_walker_binary(
        go_walker::GoWalker { file_name: cli.input.clone() },
        "go",
        &[".go"],
        &cli.input,
    )
}
