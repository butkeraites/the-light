//! Subcomando `note` — notas em Markdown por versículo/intervalo.

use std::path::Path;
use std::process::ExitCode;

use clap::{Args, Subcommand};

use biblia_core::config::Config;
use biblia_core::model::{Lang, Reference};
use biblia_core::reference::{format_reference, parse_reference};
use biblia_core::userdata::NoteStore;

use crate::md::render_markdown;
use crate::theme::Style;

/// Argumentos do subcomando `note`.
#[derive(Args)]
pub struct NoteArgs {
    #[command(subcommand)]
    action: NoteAction,
}

#[derive(Subcommand)]
enum NoteAction {
    /// Cria/atualiza uma nota. Sem texto inline, abre o `$EDITOR`.
    Add {
        /// Referência (PT/EN).
        reference: String,
        /// Texto da nota (Markdown). Se omitido, abre o editor.
        text: Option<String>,
    },
    /// Edita a nota no `$EDITOR` (cria se não existir).
    Edit {
        /// Referência da nota.
        reference: String,
    },
    /// Exibe a nota renderizada.
    Show {
        /// Referência da nota.
        reference: String,
    },
    /// Lista todas as notas.
    List,
    /// Remove a nota de uma referência.
    Remove {
        /// Referência da nota.
        reference: String,
    },
}

const EXIT_OK: u8 = 0;
const EXIT_NOT_FOUND: u8 = 1;
const EXIT_USAGE: u8 = 2;

/// Executa o comando `note`.
pub fn run(args: NoteArgs) -> ExitCode {
    match args.action {
        NoteAction::Add { reference, text } => add(&reference, text),
        NoteAction::Edit { reference } => edit(&reference),
        NoteAction::Show { reference } => show(&reference),
        NoteAction::List => list(),
        NoteAction::Remove { reference } => remove(&reference),
    }
}

fn parse(reference: &str) -> std::result::Result<Reference, ExitCode> {
    parse_reference(reference).map_err(|e| {
        eprintln!("Referência inválida: {e}");
        ExitCode::from(EXIT_USAGE)
    })
}

fn store() -> std::result::Result<NoteStore, ExitCode> {
    NoteStore::open_default().map_err(|e| {
        eprintln!("Erro ao acessar notas: {e}");
        ExitCode::from(EXIT_NOT_FOUND)
    })
}

fn display_lang() -> Lang {
    Config::load().unwrap_or_default().language
}

fn add(reference: &str, text: Option<String>) -> ExitCode {
    let reference = match parse(reference) {
        Ok(r) => r,
        Err(c) => return c,
    };
    let store = match store() {
        Ok(s) => s,
        Err(c) => return c,
    };

    if let Some(text) = text {
        let body = if text.ends_with('\n') {
            text
        } else {
            format!("{text}\n")
        };
        if let Err(e) = store.put(&reference, &body) {
            eprintln!("Erro ao gravar nota: {e}");
            return ExitCode::from(EXIT_NOT_FOUND);
        }
        println!(
            "Nota salva: {}",
            format_reference(&reference, display_lang())
        );
        ExitCode::from(EXIT_OK)
    } else {
        edit_in_editor(&store, &reference)
    }
}

fn edit(reference: &str) -> ExitCode {
    let reference = match parse(reference) {
        Ok(r) => r,
        Err(c) => return c,
    };
    let store = match store() {
        Ok(s) => s,
        Err(c) => return c,
    };
    edit_in_editor(&store, &reference)
}

/// Abre o `$EDITOR` no arquivo da nota; descarta se ficar vazio.
fn edit_in_editor(store: &NoteStore, reference: &Reference) -> ExitCode {
    let path = store.path_for(reference);
    if let Some(parent) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            eprintln!("Erro ao criar diretório de notas: {e}");
            return ExitCode::from(EXIT_NOT_FOUND);
        }
    }
    match launch_editor(&path) {
        Ok(true) => {}
        Ok(false) => {
            eprintln!("Editor saiu com erro; nota não alterada.");
            return ExitCode::from(EXIT_NOT_FOUND);
        }
        Err(e) => {
            eprintln!("Não foi possível abrir o editor ($EDITOR/$VISUAL): {e}");
            return ExitCode::from(EXIT_NOT_FOUND);
        }
    }
    // Descarta nota vazia.
    match store.get(reference) {
        Ok(Some(note)) if note.body.trim().is_empty() => {
            let _ = store.delete(reference);
            println!("Nota vazia descartada.");
            ExitCode::from(EXIT_OK)
        }
        Ok(Some(_)) => {
            println!(
                "Nota salva: {}",
                format_reference(reference, display_lang())
            );
            ExitCode::from(EXIT_OK)
        }
        Ok(None) => {
            println!("Nenhuma nota criada.");
            ExitCode::from(EXIT_OK)
        }
        Err(e) => {
            eprintln!("Erro ao ler nota: {e}");
            ExitCode::from(EXIT_NOT_FOUND)
        }
    }
}

fn launch_editor(path: &Path) -> std::io::Result<bool> {
    let editor = std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .unwrap_or_else(|_| "vi".to_string());
    let mut parts = editor.split_whitespace();
    let prog = parts.next().unwrap_or("vi");
    let status = std::process::Command::new(prog)
        .args(parts)
        .arg(path)
        .status()?;
    Ok(status.success())
}

fn show(reference: &str) -> ExitCode {
    let reference = match parse(reference) {
        Ok(r) => r,
        Err(c) => return c,
    };
    let store = match store() {
        Ok(s) => s,
        Err(c) => return c,
    };
    match store.get(&reference) {
        Ok(Some(note)) => {
            let cfg = Config::load().unwrap_or_default();
            let style = Style::resolve(false, &cfg.theme);
            println!("{}", format_reference(&reference, cfg.language));
            print!("{}", render_markdown(&note.body, &style));
            ExitCode::from(EXIT_OK)
        }
        Ok(None) => {
            println!(
                "Sem nota para {}.",
                format_reference(&reference, display_lang())
            );
            ExitCode::from(EXIT_NOT_FOUND)
        }
        Err(e) => {
            eprintln!("Erro ao ler nota: {e}");
            ExitCode::from(EXIT_NOT_FOUND)
        }
    }
}

fn list() -> ExitCode {
    let store = match store() {
        Ok(s) => s,
        Err(c) => return c,
    };
    let notes = match store.list() {
        Ok(n) => n,
        Err(e) => {
            eprintln!("Erro ao listar notas: {e}");
            return ExitCode::from(EXIT_NOT_FOUND);
        }
    };
    if notes.is_empty() {
        println!("Nenhuma nota.");
        return ExitCode::from(EXIT_OK);
    }
    let lang = display_lang();
    let refs: Vec<String> = notes
        .iter()
        .map(|n| format_reference(&n.reference, lang))
        .collect();
    let w = refs.iter().map(|r| r.chars().count()).max().unwrap_or(0);
    for (note, reference) in notes.iter().zip(&refs) {
        let snippet = first_line(&note.body);
        println!("  {reference:<w$}   {snippet}");
    }
    ExitCode::from(EXIT_OK)
}

/// Primeira linha não-vazia da nota, sem marcação de título, truncada.
fn first_line(body: &str) -> String {
    let line = body
        .lines()
        .map(|l| l.trim_start_matches('#').trim())
        .find(|l| !l.is_empty())
        .unwrap_or("");
    let max = 60;
    if line.chars().count() > max {
        let s: String = line.chars().take(max - 1).collect();
        format!("{s}…")
    } else {
        line.to_string()
    }
}

fn remove(reference: &str) -> ExitCode {
    let reference = match parse(reference) {
        Ok(r) => r,
        Err(c) => return c,
    };
    let store = match store() {
        Ok(s) => s,
        Err(c) => return c,
    };
    match store.delete(&reference) {
        Ok(true) => {
            println!(
                "Nota removida: {}",
                format_reference(&reference, display_lang())
            );
            ExitCode::from(EXIT_OK)
        }
        Ok(false) => {
            println!(
                "Sem nota para {}.",
                format_reference(&reference, display_lang())
            );
            ExitCode::from(EXIT_NOT_FOUND)
        }
        Err(e) => {
            eprintln!("Erro ao remover nota: {e}");
            ExitCode::from(EXIT_NOT_FOUND)
        }
    }
}
