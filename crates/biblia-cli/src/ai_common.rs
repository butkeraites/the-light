//! Utilitários compartilhados pelos comandos de IA (`study`, `ask`).

use std::path::Path;
use std::process::ExitCode;

use biblia_core::ai::{build_provider, KeyStore, LlmProvider};
use biblia_core::config::Config;
use biblia_core::model::{Lang, Passage, Reference, TranslationId, VerseRange};
use biblia_core::reference::format_reference;
use biblia_core::source::{BibleSource, EmbeddedSource};
use biblia_core::store::Store;
use biblia_core::xref;

const EXIT_NOT_FOUND: u8 = 1;
const EXIT_USAGE: u8 = 2;

/// Abre o banco (caminho explícito ou diretório de dados do usuário).
pub fn open_store(db: Option<&Path>) -> Result<Store, ExitCode> {
    let res = match db {
        Some(p) => Store::open(p),
        None => Store::open_default(),
    };
    res.map_err(|e| {
        eprintln!("Erro ao abrir o banco: {e}");
        ExitCode::from(EXIT_NOT_FOUND)
    })
}

/// Resolve o provedor ativo (CLI > config), buscando a chave no cofre.
///
/// Mensagens de erro amigáveis quando falta provedor/chave (saída 2).
pub fn resolve_provider(
    cli_provider: Option<String>,
    cli_model: Option<String>,
    config: &Config,
) -> Result<Box<dyn LlmProvider>, ExitCode> {
    let name = cli_provider
        .filter(|s| !s.trim().is_empty())
        .or_else(|| (!config.provider.is_empty()).then(|| config.provider.clone()));
    let name = match name {
        Some(n) => n.trim().to_ascii_lowercase(),
        None => {
            eprintln!("{}", biblia_core::ai::AiError::NoProvider);
            return Err(ExitCode::from(EXIT_USAGE));
        }
    };
    // Provedores locais/falsos não precisam de chave.
    let key = if name == "mock" || name == "ollama" {
        None
    } else {
        KeyStore::open_default()
            .ok()
            .and_then(|ks| ks.get(&name).map(str::to_string))
    };
    build_provider(&name, key, cli_model).map_err(|e| {
        eprintln!("{e}");
        ExitCode::from(EXIT_USAGE)
    })
}

/// Resolve a passagem na versão pedida (CLI > config > kjv).
pub fn resolve_passage(
    src: &EmbeddedSource,
    reference: &Reference,
    cli_version: Option<&str>,
    config: &Config,
) -> Result<Passage, ExitCode> {
    let version = cli_version
        .map(str::to_string)
        .or_else(|| config.versions.first().cloned())
        .unwrap_or_else(|| "kjv".to_string());
    let tid = TranslationId::new(&version);
    match src.has_translation(&tid) {
        Ok(true) => {}
        _ => {
            eprintln!("Versão `{version}` não encontrada no banco.");
            return Err(ExitCode::from(EXIT_NOT_FOUND));
        }
    }
    match src.passage(reference, &tid) {
        Ok(p) if !p.verses.is_empty() => Ok(p),
        Ok(_) => {
            eprintln!("Passagem sem texto na versão `{version}`.");
            Err(ExitCode::from(EXIT_NOT_FOUND))
        }
        Err(e) => {
            eprintln!("Erro ao ler a passagem: {e}");
            Err(ExitCode::from(EXIT_NOT_FOUND))
        }
    }
}

/// Rótulos de referências cruzadas locais para o primeiro versículo (RAG leve).
pub fn xref_labels(store: &Store, reference: &Reference, lang: Lang, limit: usize) -> Vec<String> {
    let verse = match reference.verses {
        VerseRange::Single(v) => v,
        VerseRange::Range { start, .. } => start,
        VerseRange::WholeChapter => 1,
    };
    xref::for_verse(
        store.conn(),
        reference.book,
        reference.chapter,
        verse,
        xref::DEFAULT_MIN_VOTES,
        limit,
    )
    .map(|hits| {
        hits.iter()
            .map(|h| format_reference(&h.reference, lang))
            .collect()
    })
    .unwrap_or_default()
}
