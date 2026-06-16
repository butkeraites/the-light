//! Subcomando `xref` — lista versículos relacionados (referências cruzadas).

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Args;

use the_light_core::config::Config;
use the_light_core::model::{Reference, TranslationId, VerseRange};
use the_light_core::reference::{format_reference, parse_reference};
use the_light_core::source::{BibleSource, EmbeddedSource};
use the_light_core::store::Store;
use the_light_core::xref;

use crate::theme::Style;

/// Argumentos do subcomando `xref`.
#[derive(Args)]
pub struct XrefArgs {
    /// Versículo de origem (PT/EN), ex.: "Rm 3.23".
    pub reference: String,

    /// Versão usada para mostrar o texto dos versículos relacionados.
    #[arg(short, long)]
    pub version: Option<String>,

    /// Limiar de votos; use negativo para incluir referências disputadas.
    #[arg(long, allow_hyphen_values = true, default_value_t = xref::DEFAULT_MIN_VOTES)]
    pub min_votes: i64,

    /// Máximo de resultados.
    #[arg(short, long, default_value_t = xref::DEFAULT_LIMIT)]
    pub limit: usize,

    /// Caminho do banco (padrão: diretório de dados do usuário).
    #[arg(long)]
    pub db: Option<PathBuf>,

    /// Saída sem cor.
    #[arg(long)]
    pub plain: bool,
}

const EXIT_OK: u8 = 0;
const EXIT_NOT_FOUND: u8 = 1;
const EXIT_USAGE: u8 = 2;

/// Executa o comando `xref`.
pub fn run(args: XrefArgs) -> ExitCode {
    let reference = match parse_reference(&args.reference) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Referência inválida: {e}");
            return ExitCode::from(EXIT_USAGE);
        }
    };
    // Referências cruzadas são por versículo.
    let verse = match reference.verses {
        VerseRange::Single(v) => v,
        VerseRange::Range { start, .. } => start,
        VerseRange::WholeChapter => {
            eprintln!("Informe um versículo específico, ex.: \"Rm 3.23\".");
            return ExitCode::from(EXIT_USAGE);
        }
    };

    let store = match open_store(args.db.as_deref()) {
        Ok(s) => s,
        Err(code) => return code,
    };

    let hits = match xref::for_verse(
        store.conn(),
        reference.book,
        reference.chapter,
        verse,
        args.min_votes,
        args.limit,
    ) {
        Ok(h) => h,
        Err(e) => {
            eprintln!("Erro ao consultar referências cruzadas: {e}");
            return ExitCode::from(EXIT_NOT_FOUND);
        }
    };

    let config = Config::load().unwrap_or_default();
    let style = Style::resolve(args.plain, &config.theme);
    let lang = config.language;

    let origin = Reference::single(reference.book, reference.chapter, verse);
    if hits.is_empty() {
        println!(
            "Nenhuma referência cruzada para {} (min-votes {}).",
            format_reference(&origin, lang),
            args.min_votes
        );
        return ExitCode::from(EXIT_NOT_FOUND);
    }

    // Versão para mostrar o texto (CLI > config > kjv); pode não existir.
    let src = EmbeddedSource::new(&store);
    let version = args
        .version
        .clone()
        .or_else(|| config.versions.first().cloned());
    let active = version.and_then(|v| {
        let tid = TranslationId::new(v);
        match src.has_translation(&tid) {
            Ok(true) => Some(tid),
            _ => None,
        }
    });

    println!(
        "{} referência(s) cruzada(s) para {}:",
        hits.len(),
        format_reference(&origin, lang)
    );
    println!();

    let refs: Vec<String> = hits
        .iter()
        .map(|h| format_reference(&h.reference, lang))
        .collect();
    let w = refs.iter().map(|r| r.chars().count()).max().unwrap_or(0);

    for (hit, reference) in hits.iter().zip(&refs) {
        let text = active
            .as_ref()
            .and_then(|tid| verse_snippet(&src, &hit.reference, tid));
        let votes = style.dim(&format!("({})", hit.votes));
        match text {
            Some(t) => println!("  {reference:<w$}  {votes}  {t}"),
            None => println!("  {reference:<w$}  {votes}"),
        }
    }

    // Atribuição obrigatória CC-BY dos dados de referências cruzadas.
    println!();
    println!("{}", style.dim(XREF_ATTRIBUTION));

    ExitCode::from(EXIT_OK)
}

/// Atribuição CC-BY exigida para os dados de referências cruzadas (OpenBible.info).
pub const XREF_ATTRIBUTION: &str = "Referências cruzadas cortesia de OpenBible.info (CC-BY).";

/// Texto do primeiro versículo do destino (com `…` se for intervalo).
fn verse_snippet(
    src: &EmbeddedSource,
    reference: &Reference,
    tid: &TranslationId,
) -> Option<String> {
    let passage = src.passage(reference, tid).ok()?;
    let first = passage.verses.first()?;
    let more = if passage.verses.len() > 1 { " …" } else { "" };
    Some(format!("{}{more}", first.text))
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
