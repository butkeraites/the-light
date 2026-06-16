//! Binário `biblia` — interface de linha de comando.
//!
//! Subcomandos são adicionados tarefa a tarefa. Hoje: `read`.

mod config;
mod read;
mod render;
mod search;

use clap::{Parser, Subcommand};
use std::process::ExitCode;

/// Leitor de Bíblia hackeável para terminal.
#[derive(Parser)]
#[command(name = "biblia", version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Lê uma passagem, ex.: `biblia read "John 3:16" --version kjv`.
    Read(read::ReadArgs),
    /// Busca full-text, ex.: `biblia search "graça" --version alm1911`.
    Search(search::SearchArgs),
    /// Lê/edita as preferências (`config.toml`).
    Config(config::ConfigArgs),
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Read(args) => read::run(args),
        Command::Search(args) => search::run(args),
        Command::Config(args) => config::run(args),
    }
}
