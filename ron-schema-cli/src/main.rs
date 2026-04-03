use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "ron-schema", version, about = "Validate RON files against schemas")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Validate RON files against a schema
    Validate {
        /// Path to the .ronschema file
        #[arg(long)]
        schema: PathBuf,

        /// Path to a .ron file or directory of .ron files
        target: PathBuf,
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Validate { schema, target } => {
            todo!("Implementation goes here")
        }
    }
}