//! Subcomando `export` — exporta notas/estudos para Markdown (ou PDF via pandoc).

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Args;

use biblia_core::config::Config;
use biblia_core::reference::format_reference;
use biblia_core::userdata::{self, NoteStore};

/// Argumentos do subcomando `export`.
#[derive(Args)]
pub struct ExportArgs {
    /// O que exportar: `notes` ou `study`.
    pub what: String,

    /// Formato de saída: `md` (padrão) ou `pdf` (requer `pandoc`).
    #[arg(short, long, default_value = "md")]
    pub format: String,

    /// Arquivo de saída. Para `md`, omitir imprime no stdout; `pdf` exige.
    #[arg(short, long)]
    pub output: Option<PathBuf>,
}

const EXIT_OK: u8 = 0;
const EXIT_NOT_FOUND: u8 = 1;
const EXIT_USAGE: u8 = 2;

/// Executa o comando `export`.
pub fn run(args: ExportArgs) -> ExitCode {
    let markdown = match args.what.as_str() {
        "notes" | "notas" => match notes_markdown() {
            Ok(m) => m,
            Err(code) => return code,
        },
        "study" | "studies" | "estudos" => match studies_markdown() {
            Ok(m) => m,
            Err(code) => return code,
        },
        other => {
            eprintln!("Não sei exportar `{other}` (use: notes, study).");
            return ExitCode::from(EXIT_USAGE);
        }
    };

    match args.format.to_ascii_lowercase().as_str() {
        "md" | "markdown" => write_md(&markdown, args.output.as_deref()),
        "pdf" => write_pdf(&markdown, args.output.as_deref()),
        other => {
            eprintln!("Formato desconhecido: `{other}` (use: md, pdf).");
            ExitCode::from(EXIT_USAGE)
        }
    }
}

/// Compila todas as notas num único Markdown.
fn notes_markdown() -> Result<String, ExitCode> {
    let store = NoteStore::open_default().map_err(|e| {
        eprintln!("Erro ao acessar notas: {e}");
        ExitCode::from(EXIT_NOT_FOUND)
    })?;
    let notes = store.list().map_err(|e| {
        eprintln!("Erro ao listar notas: {e}");
        ExitCode::from(EXIT_NOT_FOUND)
    })?;
    let lang = Config::load().unwrap_or_default().language;

    let mut out = String::from("# Notas\n\n");
    if notes.is_empty() {
        out.push_str("_Nenhuma nota._\n");
        return Ok(out);
    }
    for note in &notes {
        out.push_str(&format!(
            "## {}\n\n",
            format_reference(&note.reference, lang)
        ));
        out.push_str(note.body.trim_end());
        out.push_str("\n\n");
    }
    Ok(out)
}

/// Concatena os estudos salvos (`studies/*.md`).
fn studies_markdown() -> Result<String, ExitCode> {
    let dir = userdata::studies_dir().map_err(|e| {
        eprintln!("Erro ao localizar estudos: {e}");
        ExitCode::from(EXIT_NOT_FOUND)
    })?;
    let mut files: Vec<PathBuf> = match std::fs::read_dir(&dir) {
        Ok(rd) => rd
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("md"))
            .collect(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(e) => {
            eprintln!("Erro ao ler estudos: {e}");
            return Err(ExitCode::from(EXIT_NOT_FOUND));
        }
    };
    files.sort();

    let mut out = String::from("# Estudos\n\n");
    if files.is_empty() {
        out.push_str("_Nenhum estudo salvo._\n");
        return Ok(out);
    }
    let bodies: Vec<String> = files
        .iter()
        .filter_map(|p| std::fs::read_to_string(p).ok())
        .map(|b| b.trim_end().to_string())
        .collect();
    // Separador `---` **entre** estudos, sem sobra no final.
    out.push_str(&bodies.join("\n\n---\n\n"));
    out.push('\n');
    Ok(out)
}

fn write_md(markdown: &str, output: Option<&Path>) -> ExitCode {
    match output {
        Some(path) => {
            if let Err(e) = biblia_core::util::atomic_write(path, markdown.as_bytes()) {
                eprintln!("Erro ao gravar {}: {e}", path.display());
                return ExitCode::from(EXIT_NOT_FOUND);
            }
            println!("Exportado para {}", path.display());
        }
        None => print!("{markdown}"),
    }
    ExitCode::from(EXIT_OK)
}

fn write_pdf(markdown: &str, output: Option<&Path>) -> ExitCode {
    let Some(output) = output else {
        eprintln!("`--format pdf` exige `--output <arquivo.pdf>`.");
        return ExitCode::from(EXIT_USAGE);
    };
    // Grava um Markdown temporário e converte com pandoc.
    let tmp = match tempfile::Builder::new().suffix(".md").tempfile() {
        Ok(mut f) => {
            use std::io::Write;
            if let Err(e) = f.write_all(markdown.as_bytes()) {
                eprintln!("Erro ao preparar Markdown: {e}");
                return ExitCode::from(EXIT_NOT_FOUND);
            }
            f
        }
        Err(e) => {
            eprintln!("Erro ao criar arquivo temporário: {e}");
            return ExitCode::from(EXIT_NOT_FOUND);
        }
    };
    match std::process::Command::new("pandoc")
        .arg(tmp.path())
        .arg("-o")
        .arg(output)
        .status()
    {
        Ok(s) if s.success() => {
            println!("Exportado para {}", output.display());
            ExitCode::from(EXIT_OK)
        }
        Ok(_) => {
            eprintln!("pandoc falhou ao gerar o PDF.");
            ExitCode::from(EXIT_NOT_FOUND)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            eprintln!(
                "`pandoc` não encontrado. Exporte em Markdown e converta você mesmo:\n  \
                 biblia export notes --format md --output notas.md && pandoc notas.md -o notas.pdf"
            );
            ExitCode::from(EXIT_NOT_FOUND)
        }
        Err(e) => {
            eprintln!("Erro ao executar pandoc: {e}");
            ExitCode::from(EXIT_NOT_FOUND)
        }
    }
}
