//! Binário `biblia` — interface de linha de comando.
//!
//! Subcomandos são adicionados tarefa a tarefa. Hoje: `read`.

mod config;
mod export;
mod highlight;
mod md;
mod note;
mod read;
mod render;
mod search;
mod theme;
mod xref;

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
    /// Marca versículos com cor/etiqueta.
    Highlight(highlight::HighlightArgs),
    /// Notas em Markdown por versículo/intervalo.
    Note(note::NoteArgs),
    /// Referências cruzadas de um versículo.
    Xref(xref::XrefArgs),
    /// Exporta notas/estudos (Markdown ou PDF via pandoc).
    Export(export::ExportArgs),
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Read(args) => read::run(args),
        Command::Search(args) => search::run(args),
        Command::Config(args) => config::run(args),
        Command::Highlight(args) => highlight::run(args),
        Command::Note(args) => note::run(args),
        Command::Xref(args) => xref::run(args),
        Command::Export(args) => export::run(args),
    }
}
