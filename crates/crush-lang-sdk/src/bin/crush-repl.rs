use clap::Parser;
use crush_lang_sdk::repl::{ReplConfig, run};
use crush_vm::Quotas;

#[derive(Parser)]
#[command(name = "crush-repl")]
#[command(about = "Interactive REPL for the Crush language")]
struct Args {
    /// Enable standard library capabilities (str.*, math.*, conv.*, ...)
    #[arg(long)]
    stdlib: bool,

    /// Maximum instruction steps.
    #[arg(long, default_value = "100000")]
    max_steps: usize,

    /// Maximum stack depth.
    #[arg(long, default_value = "1024")]
    max_stack: usize,

    /// Maximum output bytes.
    #[arg(long, default_value = "65536")]
    max_output: usize,

    /// Maximum call depth.
    #[arg(long, default_value = "64")]
    max_call_depth: usize,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let config = ReplConfig {
        quotas: Quotas {
            max_steps: args.max_steps,
            max_stack: args.max_stack,
            max_output: args.max_output,
            max_call_depth: args.max_call_depth,
            ..Default::default()
        },
        stdlib: args.stdlib,
    };

    run(config)
}
