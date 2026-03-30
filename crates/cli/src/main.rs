use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum OutputFormat {
    Json,
    Text,
}

#[derive(Debug, Parser)]
#[command(name = "rustygene", version, about = "RustyGene CLI")]
struct Cli {
    /// Database location
    #[arg(long, global = true, default_value = "~/.rustygene/rustygene.db")]
    db: PathBuf,

    /// Output format
    #[arg(long, global = true, value_enum, default_value_t = OutputFormat::Text)]
    format: OutputFormat,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Import,
    Export,
    Query,
    Show,
    ResearchLog,
    RebuildSnapshots,
}

fn main() {
    let _cli = Cli::parse();
}
