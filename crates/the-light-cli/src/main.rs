//! Binário `light` — interface de linha de comando.
//!
//! Leitor de Bíblia hackeável para terminal: leitura/busca offline de versões
//! livres, estudo pessoal (marcações, notas, xrefs, planos), TUI, e estudo por
//! IA opt-in (BYOK). Cada subcomando vive no seu módulo.

mod ai_common;
mod ask;
mod config;
mod export;
mod highlight;
mod md;
mod note;
mod plan;
mod read;
mod render;
mod search;
mod sources;
mod study;
mod theme;
mod tui;
mod xref;

use clap::{Parser, Subcommand};
use std::process::ExitCode;

/// Leitor de Bíblia hackeável para terminal.
#[derive(Parser)]
#[command(
    name = "light",
    version,
    about = "Leitor de Bíblia hackeável para terminal (CLI + TUI), bilíngue PT/EN.",
    long_about = "The Light — leitor de Bíblia para o terminal.\n\n\
        Offline-first: leitura e busca de versões livres funcionam sem internet \
        e sem IA. O estudo por IA é opt-in e BYOK (você usa sua própria chave); \
        a chave nunca sai da máquina exceto para o provedor escolhido. Sem \
        telemetria. Versões protegidas (ARA/NVI/ESV/…) nunca são embarcadas — \
        só via conector com a sua credencial.",
    after_help = "EXEMPLOS:\n  \
        light read \"John 3:16\" --version kjv,alm1911\n  \
        light search \"graça\" --book Romanos\n  \
        light note add \"Jo 3.16\" \"Versículo **central**.\"\n  \
        light plan start annual --year 2026\n  \
        light tui\n  \
        light study \"Ef 2.8-9\" --lens presbiteriana   # requer provedor + chave\n\n\
        Mais detalhes em cada subcomando: `light <comando> --help`.\n\
        Documentação: https://github.com/butkeraites/the-light"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Lê uma passagem, ex.: `light read "John 3:16" --version kjv`.
    Read(read::ReadArgs),
    /// Busca full-text, ex.: `light search "graça" --version alm1911`.
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
    /// Abre a interface de terminal (TUI).
    Tui(tui::TuiArgs),
    /// Planos de leitura (cronológico/anual/temático) com progresso.
    Plan(plan::PlanArgs),
    /// Estudo exegético por lente denominacional (IA, BYOK).
    Study(study::StudyArgs),
    /// Pergunta livre ancorada numa referência (IA, BYOK).
    Ask(ask::AskArgs),
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
        Command::Tui(args) => tui::run(args),
        Command::Plan(args) => plan::run(args),
        Command::Study(args) => study::run(args),
        Command::Ask(args) => ask::run(args),
    }
}
