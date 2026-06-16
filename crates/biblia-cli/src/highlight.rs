//! Subcomando `highlight` — marca versículos/intervalos com cor e etiqueta.

use std::process::ExitCode;

use clap::{Args, Subcommand};

use biblia_core::config::Config;
use biblia_core::reference::{format_reference, parse_reference};
use biblia_core::userdata::{Highlight, HighlightStore};

/// Cores aceitas (mapeadas para ANSI na exibição).
pub const COLORS: &[&str] = &["yellow", "green", "blue", "red", "cyan", "magenta"];

/// Argumentos do subcomando `highlight`.
#[derive(Args)]
pub struct HighlightArgs {
    #[command(subcommand)]
    action: HighlightAction,
}

#[derive(Subcommand)]
enum HighlightAction {
    /// Marca uma referência: `highlight add "Jo 3.16" --color yellow --tag salvação`.
    Add {
        /// Referência (PT/EN), ex.: "Jo 3.16", "Sl 23".
        reference: String,
        /// Cor da marcação.
        #[arg(short, long, default_value = "yellow")]
        color: String,
        /// Etiqueta opcional.
        #[arg(short, long)]
        tag: Option<String>,
    },
    /// Lista as marcações.
    List,
    /// Remove a marcação de uma referência.
    Remove {
        /// Referência a desmarcar.
        reference: String,
    },
}

const EXIT_OK: u8 = 0;
const EXIT_NOT_FOUND: u8 = 1;
const EXIT_USAGE: u8 = 2;

/// Executa o comando `highlight`.
pub fn run(args: HighlightArgs) -> ExitCode {
    match args.action {
        HighlightAction::Add {
            reference,
            color,
            tag,
        } => add(&reference, &color, tag),
        HighlightAction::List => list(),
        HighlightAction::Remove { reference } => remove(&reference),
    }
}

fn load_store() -> std::result::Result<HighlightStore, ExitCode> {
    HighlightStore::load_default().map_err(|e| {
        eprintln!("Erro ao ler marcações: {e}");
        ExitCode::from(EXIT_NOT_FOUND)
    })
}

fn display_lang() -> biblia_core::model::Lang {
    Config::load().unwrap_or_default().language
}

fn add(reference: &str, color: &str, tag: Option<String>) -> ExitCode {
    let reference = match parse_reference(reference) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Referência inválida: {e}");
            return ExitCode::from(EXIT_USAGE);
        }
    };
    let color = color.to_ascii_lowercase();
    if !COLORS.contains(&color.as_str()) {
        eprintln!("Cor desconhecida: `{color}`. Use: {}", COLORS.join(", "));
        return ExitCode::from(EXIT_USAGE);
    }

    let mut store = match load_store() {
        Ok(s) => s,
        Err(code) => return code,
    };
    store.add(Highlight {
        reference,
        color: color.clone(),
        tag: tag.clone(),
    });
    if let Err(e) = store.save() {
        eprintln!("Erro ao gravar marcações: {e}");
        return ExitCode::from(EXIT_NOT_FOUND);
    }
    let tag_s = tag.map(|t| format!(" [{t}]")).unwrap_or_default();
    println!(
        "Marcado: {} ({color}){tag_s}",
        format_reference(&reference, display_lang())
    );
    ExitCode::from(EXIT_OK)
}

fn list() -> ExitCode {
    let store = match load_store() {
        Ok(s) => s,
        Err(code) => return code,
    };
    let lang = display_lang();
    if store.list().is_empty() {
        println!("Nenhuma marcação.");
        return ExitCode::from(EXIT_OK);
    }
    // Alinha as referências.
    let rows: Vec<(String, &Highlight)> = store
        .list()
        .iter()
        .map(|h| (format_reference(&h.reference, lang), h))
        .collect();
    let w = rows
        .iter()
        .map(|(r, _)| r.chars().count())
        .max()
        .unwrap_or(0);
    for (reference, h) in &rows {
        let tag = h
            .tag
            .as_deref()
            .map(|t| format!("  [{t}]"))
            .unwrap_or_default();
        println!("  {reference:<w$}   {}{tag}", h.color);
    }
    ExitCode::from(EXIT_OK)
}

fn remove(reference: &str) -> ExitCode {
    let parsed = match parse_reference(reference) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Referência inválida: {e}");
            return ExitCode::from(EXIT_USAGE);
        }
    };
    let mut store = match load_store() {
        Ok(s) => s,
        Err(code) => return code,
    };
    let n = store.remove(&parsed);
    if n == 0 {
        println!(
            "Nenhuma marcação em {}.",
            format_reference(&parsed, display_lang())
        );
        return ExitCode::from(EXIT_NOT_FOUND);
    }
    if let Err(e) = store.save() {
        eprintln!("Erro ao gravar marcações: {e}");
        return ExitCode::from(EXIT_NOT_FOUND);
    }
    println!("Removida(s) {n} marcação(ões).");
    ExitCode::from(EXIT_OK)
}
