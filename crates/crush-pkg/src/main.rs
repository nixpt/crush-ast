#![allow(dead_code)]

use std::path::PathBuf;

use clap::{Parser, Subcommand};

mod builder;
mod bundle;
mod ecap;
mod manifest;
mod merkle;
mod packer;
mod runners;
mod signer;

use builder::PackageBuilder;
use manifest::Manifest;
use packer::{pack, unpack};
use runners::{get_runner_for_payload, ExecutionResult};
use signer::{generate_keys, sign_package, verify_package};

fn find_manifest() -> anyhow::Result<PathBuf> {
    let cwd = std::env::current_dir()?;
    let mut dir = cwd.as_path();
    loop {
        for name in ["capsule.toml", "Capsule.toml", "crush.toml", "Crush.toml"] {
            let candidate = dir.join(name);
            if candidate.exists() {
                return Ok(candidate);
            }
        }
        if let Some(parent) = dir.parent() {
            dir = parent;
        } else {
            anyhow::bail!(
                "no capsule/crush.toml found in {} or any parent directory",
                cwd.display()
            );
        }
    }
}

fn load_manifest() -> anyhow::Result<(Manifest, PathBuf)> {
    let path = find_manifest()?;
    let manifest = Manifest::from_file(&path)?;
    let root = path.parent().unwrap().to_path_buf();
    Ok((manifest, root))
}

#[derive(Parser)]
#[command(name = "crush-pkg")]
#[command(about = "Crush Package Manager — build, run, and manage Crush programs")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Create a new Crush package
    New {
        /// Package name
        name: String,
        /// Output directory (defaults to ./<name>)
        #[arg(short, long)]
        dir: Option<PathBuf>,
    },
    /// Build the current package
    Build,
    /// Run the current package
    Run {
        /// Arguments passed to the program's main function
        #[arg(last = true)]
        args: Vec<String>,
    },
    /// Type-check without emitting bytecode
    Check,
    /// Pack source into a .crush-pack archive
    Pack {
        /// Output path (defaults to <name>.crush-pack)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Unpack a .crush-pack archive
    Unpack {
        /// Path to .crush-pack archive
        pack: PathBuf,
        /// Output directory (defaults to ./<name>)
        #[arg(short, long)]
        dir: Option<PathBuf>,
    },
    /// Generate Ed25519 signing keys
    GenerateKeys {
        /// Output directory for key files
        #[arg(default_value = "./keys")]
        dir: PathBuf,
    },
    /// Sign a .cap file with a private key
    Sign {
        /// Path to the .cap file
        package: PathBuf,
        /// Path to private key (64-byte keypair)
        key: PathBuf,
    },
    /// Verify a .cap file's signature
    Verify {
        /// Path to the .cap file
        package: PathBuf,
        /// Path to public key (32 bytes)
        key: PathBuf,
    },
    /// Show package metadata
    Show,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::New { name, dir } => {
            let target = dir.unwrap_or_else(|| PathBuf::from(&name));
            if target.exists() {
                anyhow::bail!("directory {} already exists", target.display());
            }
            let manifest = manifest::scaffold_package(&target, &name)?;
            println!(
                "created new Crush package at {}",
                target.join("capsule.toml").display()
            );
            println!(
                "  name:    {}",
                manifest.capsule.name
            );
            println!(
                "  entry:   {}",
                manifest.capsule.entry
            );
            println!(
                "  version: {}",
                manifest.capsule.version
            );
        }
        Commands::Build => {
            let (manifest, root) = load_manifest()?;
            println!("building {} v{}", manifest.capsule.name, manifest.capsule.version);
            let builder = PackageBuilder::new(manifest, root);
            let output = builder.build()?;
            builder.write_output(&output)?;
            println!(
                "done: {} function(s), {} byte(s)",
                output.functions.len(),
                output.program.code.len()
            );
        }
        Commands::Run { args } => {
            let (manifest, root) = load_manifest()?;
            let payload = root.join(&manifest.capsule.entry);
            if !payload.exists() {
                anyhow::bail!("entry file not found: {}", payload.display());
            }
            let runner = get_runner_for_payload(&payload, &manifest);
            println!("running {} v{} ({})", manifest.capsule.name, manifest.capsule.version, manifest.capsule.language);
            let result = runner.run(&manifest, &payload, &args)?;
            match result {
                ExecutionResult::Vm => {},
                ExecutionResult::Process(mut child) => {
                    let status = child.wait()?;
                    if !status.success() {
                        std::process::exit(status.code().unwrap_or(1));
                    }
                }
                ExecutionResult::None => {},
            }
        }
        Commands::Pack { output } => {
            let (manifest, root) = load_manifest()?;
            let output = output.unwrap_or_else(|| {
                PathBuf::from(format!("{}.crush-pack", manifest.capsule.name))
            });
            pack(&root, &output)?;
        }
        Commands::Unpack { pack, dir } => {
            let name = pack.file_stem().and_then(|s| s.to_str()).unwrap_or("package");
            let dir = dir.unwrap_or_else(|| PathBuf::from(name));
            unpack(&pack, &dir)?;
        }
        Commands::GenerateKeys { dir } => {
            generate_keys(&dir)?;
        }
        Commands::Sign { package, key } => {
            sign_package(&package, &key)?;
        }
        Commands::Verify { package, key } => {
            verify_package(&package, &key)?;
        }
        Commands::Check => {
            let (manifest, root) = load_manifest()?;
            println!("checking {} v{}", manifest.capsule.name, manifest.capsule.version);
            let builder = PackageBuilder::new(manifest, root);
            builder.check()?;
        }
        Commands::Show => {
            let (manifest, root) = load_manifest()?;
            println!("{}", manifest::Manifest::to_toml_string(&manifest)?);
            println!("  (at {})", root.join("capsule.toml").display());
        }
    }

    Ok(())
}
