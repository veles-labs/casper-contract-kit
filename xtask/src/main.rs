use std::fs;
use std::path::PathBuf;
use std::process::exit;

use clap::{Parser, Subcommand};
use xshell::{cmd, Shell};

#[derive(Parser)]
#[command(name = "xtask")]
#[command(about = "Workspace task runner", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Build a smart contract example by package name.
    BuildExample { package: String },
    /// Build all smart contract examples under ./examples.
    BuildExamples,
}

fn main() -> xshell::Result<()> {
    let cli = Cli::parse();
    let sh = Shell::new()?;
    let _dir = sh.push_dir(workspace_root());

    match cli.command {
        Commands::BuildExample { package } => {
            cmd!(
                sh,
                "cargo build --target wasm32v1-none -p {package} --release"
            )
            .run()?;
        }
        Commands::BuildExamples => {
            let manifests = example_manifests();
            if manifests.is_empty() {
                eprintln!("No examples found under ./examples.");
                exit(2);
            }
            for manifest in manifests {
                cmd!(
                    sh,
                    "cargo build --target wasm32v1-none --release --manifest-path {manifest}"
                )
                .run()?;
            }
        }
    }

    Ok(())
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask should live in a workspace subdirectory")
        .to_path_buf()
}

fn example_manifests() -> Vec<PathBuf> {
    let examples_dir = workspace_root().join("examples");
    if !examples_dir.is_dir() {
        eprintln!("Missing examples directory: {}", examples_dir.display());
        exit(2);
    }

    let mut manifests = Vec::new();
    for entry in fs::read_dir(&examples_dir).unwrap_or_else(|err| {
        eprintln!("Failed to read examples directory: {err}");
        exit(1);
    }) {
        let entry = entry.unwrap_or_else(|err| {
            eprintln!("Failed to read examples directory entry: {err}");
            exit(1);
        });
        let path = entry.path();
        if path.is_dir() {
            let manifest = path.join("Cargo.toml");
            if manifest.is_file() {
                manifests.push(manifest);
            }
        }
    }

    manifests.sort();
    manifests
}
