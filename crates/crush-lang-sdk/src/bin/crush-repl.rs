use clap::Parser;
use crush_lang_sdk::MessageFormat;
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

    /// Format for diagnostic output on errors: `text` (default, themed
    /// per-line errors via `[crush-codepoint-N]` badges) or `json`
    /// (NDJSON records for editor / IDE / LSP bridge integration).
    /// Mirrors `crushc` / `crush-run` / `crush-compile`.
    #[arg(long = "message-format", value_name = "FORMAT", default_value = "text")]
    message_format: MessageFormat,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    crush_lang_sdk::theme::init_styling();

    let config = ReplConfig {
        quotas: Quotas {
            max_steps: args.max_steps,
            max_stack: args.max_stack,
            max_output: args.max_output,
            max_call_depth: args.max_call_depth,
            ..Default::default()
        },
        stdlib: args.stdlib,
        message_format: args.message_format,
    };

    // Errors during REPL eval are dispatched *inside* `run()` against
    // `config.message_format` (per-line, both stdin-line parse failures
    // and runtime eval failures) so the binary's `main` only returns
    // I/O errors from the underlying read loop / pipe. Those would only
    // surface under unusual conditions (broken stdin pipe, signal
    // handler) and aren't part of the JSON-mode observable surface.
    run(config)
}
