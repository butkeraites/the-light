//! Subcomando `search` — busca full-text com destaque e ranqueamento.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Args;

use biblia_core::model::TranslationId;
use biblia_core::reference::{book_number, format_reference};
use biblia_core::search::SearchOptions;
use biblia_core::source::{BibleSource, EmbeddedSource};
use biblia_core::store::Store;

use crate::render::highlight_brackets;

/// Argumentos do subcomando `search`.
#[derive(Args)]
pub struct SearchArgs {
    /// Termo(s) a buscar. Múltiplas palavras combinam com AND.
    pub query: String,

    /// Versão onde buscar (slug), ex.: `alm1911`.
    #[arg(short, long, default_value = "kjv")]
    pub version: String,

    /// Restringe a um livro (nome/abreviação PT ou EN), ex.: `Romanos`.
    #[arg(short, long)]
    pub book: Option<String>,

    /// Máximo de resultados.
    #[arg(short, long, default_value_t = biblia_core::search::DEFAULT_LIMIT)]
    pub limit: usize,

    /// Caminho do banco (padrão: diretório de dados do usuário).
    #[arg(long)]
    pub db: Option<PathBuf>,
}

const EXIT_OK: u8 = 0;
const EXIT_NOT_FOUND: u8 = 1;
const EXIT_USAGE: u8 = 2;

/// Executa o comando `search`.
pub fn run(args: SearchArgs) -> ExitCode {
    let store = match open_store(args.db.as_deref()) {
        Ok(s) => s,
        Err(code) => return code,
    };
    let src = EmbeddedSource::new(&store);

    let translations = match src.translations() {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Erro ao ler o banco: {e}");
            return ExitCode::from(EXIT_NOT_FOUND);
        }
    };
    let tid = TranslationId::new(args.version.clone());
    let Some(meta) = translations.iter().find(|t| t.id == tid) else {
        if translations.is_empty() {
            eprintln!(
                "Nenhuma versão importada. Gere o banco com:\n  \
                 cargo run -p xtask -- import --version kjv,alm1911"
            );
        } else {
            eprintln!(
                "Versão desconhecida: `{}`. Disponíveis: {}",
                args.version,
                translations
                    .iter()
                    .map(|t| t.id.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        return ExitCode::from(EXIT_USAGE);
    };

    // Filtro de livro opcional.
    let book = match &args.book {
        None => None,
        Some(name) => match book_number(name) {
            Some(n) => Some(n),
            None => {
                eprintln!("Livro desconhecido: `{name}`.");
                return ExitCode::from(EXIT_USAGE);
            }
        },
    };

    let opts = SearchOptions {
        translation: tid,
        book,
        limit: args.limit.max(1),
    };

    let hits = match src.search(&args.query, &opts) {
        Ok(h) => h,
        Err(e) => {
            eprintln!("Erro na busca: {e}");
            return ExitCode::from(EXIT_NOT_FOUND);
        }
    };

    if hits.is_empty() {
        println!(
            "Nenhum resultado para \"{}\" em {}.",
            args.query, meta.abbrev
        );
        return ExitCode::from(EXIT_NOT_FOUND);
    }

    let scope = match &args.book {
        Some(b) => format!(" (livro: {b})"),
        None => String::new(),
    };
    println!(
        "{} resultado(s) para \"{}\" em {} ({}){}:",
        hits.len(),
        args.query,
        meta.abbrev,
        meta.name,
        scope
    );
    println!();

    // Alinha as referências numa coluna.
    let refs: Vec<String> = hits
        .iter()
        .map(|h| format_reference(&h.reference, meta.language))
        .collect();
    let ref_w = refs.iter().map(|r| r.chars().count()).max().unwrap_or(0);

    for (hit, reference) in hits.iter().zip(&refs) {
        println!(
            "  {:<ref_w$}   {}",
            reference,
            highlight_brackets(&hit.highlighted)
        );
    }

    ExitCode::from(EXIT_OK)
}

fn open_store(db: Option<&Path>) -> Result<Store, ExitCode> {
    let res = match db {
        Some(p) => Store::open(p),
        None => Store::open_default(),
    };
    res.map_err(|e| {
        eprintln!("Erro ao abrir o banco: {e}");
        ExitCode::from(EXIT_NOT_FOUND)
    })
}
