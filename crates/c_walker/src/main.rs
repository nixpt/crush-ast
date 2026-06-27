use anyhow::Result;
use clap::Parser as ClapParser;
use walker_core::run_walker_binary;

#[derive(ClapParser)]
#[command(name = "c_walker")]
struct Cli {
    input: String,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    run_walker_binary(
        c_walker::CWalker { file_name: cli.input.clone() },
        "c",
        &[".c", ".h"],
        &cli.input,
    )
}
