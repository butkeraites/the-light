//! Binário `biblia` — interface de linha de comando.
//!
//! Subcomandos são adicionados tarefa a tarefa. Hoje: `read`.

mod read;

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
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Read(args) => read::run(args),
    }
}
