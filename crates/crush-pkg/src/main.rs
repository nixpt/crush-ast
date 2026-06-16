use std::path::PathBuf;

use clap::{Parser, Subcommand};

mod builder;
mod manifest;
mod packer;

use builder::PackageBuilder;
use manifest::Manifest;
use packer::{pack, unpack};

fn find_manifest() -> anyhow::Result<PathBuf> {
    let cwd = std::env::current_dir()?;
    let mut dir = cwd.as_path();
    loop {
        let candidate = dir.join("crush.toml");
        if candidate.exists() {
            return Ok(candidate);
        }
        if let Some(parent) = dir.parent() {
            dir = parent;
        } else {
            anyhow::bail!(
                "no crush.toml found in {} or any parent directory",
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
                target.join("crush.toml").display()
            );
            println!(
                "  name:    {}",
                manifest.package.name
            );
            println!(
                "  entry:   {}",
                manifest.package.entry
            );
            println!(
                "  version: {}",
                manifest.package.version
            );
        }
        Commands::Build => {
            let (manifest, root) = load_manifest()?;
            println!("building {} v{}", manifest.package.name, manifest.package.version);
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
            println!("running {} v{}", manifest.package.name, manifest.package.version);
            let builder = PackageBuilder::new(manifest, root);
            let result = builder.run(&args)?;
            if !result.output.is_empty() {
                print!("{}", result.output);
            }
        }
        Commands::Pack { output } => {
            let (manifest, root) = load_manifest()?;
            let output = output.unwrap_or_else(|| {
                PathBuf::from(format!("{}.crush-pack", manifest.package.name))
            });
            pack(&root, &output)?;
        }
        Commands::Unpack { pack, dir } => {
            let name = pack.file_stem().and_then(|s| s.to_str()).unwrap_or("package");
            let dir = dir.unwrap_or_else(|| PathBuf::from(name));
            unpack(&pack, &dir)?;
        }
        Commands::Check => {
            let (manifest, root) = load_manifest()?;
            println!("checking {} v{}", manifest.package.name, manifest.package.version);
            let builder = PackageBuilder::new(manifest, root);
            builder.check()?;
        }
        Commands::Show => {
            let (manifest, root) = load_manifest()?;
            let path = root.join("crush.toml");
            println!("{}", manifest::Manifest::to_toml_string(&manifest)?);
            println!("  (at {})", path.display());
        }
    }

    Ok(())
}
