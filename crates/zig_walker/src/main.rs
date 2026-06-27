use anyhow::Result;
use clap::Parser as ClapParser;
use walker_core::run_walker_binary;

#[derive(ClapParser)]
#[command(name = "zig_walker")]
struct Cli {
    input: String,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    run_walker_binary(
        zig_walker::ZigWalker { file_name: cli.input.clone() },
        "zig",
        &[".zig"],
        &cli.input,
    )
}
