use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};
use rusqlite::Connection;
use rustygene_storage::{run_migrations, sqlite_impl::SqliteBackend};

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
    let cli = Cli::parse();

    match cli.command {
        Commands::RebuildSnapshots => {
            let db_path = resolve_db_path(&cli.db);
            if let Some(parent) = db_path.parent()
                && let Err(err) = std::fs::create_dir_all(parent)
            {
                eprintln!(
                    "failed to create database directory '{}': {}",
                    parent.display(),
                    err
                );
                std::process::exit(1);
            }
            let mut connection = match Connection::open(&db_path) {
                Ok(conn) => conn,
                Err(err) => {
                    eprintln!("failed to open database '{}': {}", db_path.display(), err);
                    std::process::exit(1);
                }
            };

            if let Err(err) = run_migrations(&mut connection) {
                eprintln!("failed to run migrations: {}", err);
                std::process::exit(1);
            }

            let backend = SqliteBackend::new(connection);
            match backend.rebuild_all_snapshots() {
                Ok(rebuilt_count) => match cli.format {
                    OutputFormat::Text => {
                        println!("rebuild-snapshots complete: {} entity snapshots rebuilt", rebuilt_count);
                    }
                    OutputFormat::Json => {
                        println!("{{\"rebuilt\":{}}}", rebuilt_count);
                    }
                },
                Err(err) => {
                    eprintln!("failed to rebuild snapshots: {}", err.message);
                    std::process::exit(1);
                }
            }
        }
        Commands::Import
        | Commands::Export
        | Commands::Query
        | Commands::Show
        | Commands::ResearchLog => {
            eprintln!("command not implemented yet");
            std::process::exit(2);
        }
    }
}

fn resolve_db_path(path: &PathBuf) -> PathBuf {
    let path_str = path.to_string_lossy();
    if path_str == "~" {
        return std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| path.clone());
    }
    if let Some(stripped) = path_str.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(stripped);
        }
    }
    path.clone()
}

#[cfg(test)]
mod tests {
    use super::resolve_db_path;
    use std::path::PathBuf;

    #[test]
    fn resolve_db_path_leaves_absolute_path_unchanged() {
        let input = PathBuf::from("/tmp/rustygene-test.db");
        assert_eq!(resolve_db_path(&input), input);
    }

    #[test]
    fn resolve_db_path_expands_home_prefix() {
        let home = std::env::var_os("HOME").expect("HOME must be set for test");
        let resolved = resolve_db_path(&PathBuf::from("~/.rustygene/test.db"));
        assert_eq!(resolved, PathBuf::from(home).join(".rustygene/test.db"));
    }
}
