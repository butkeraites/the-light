//! Utilitários compartilhados pelos comandos de IA (`study`, `ask`).

use std::path::Path;
use std::process::ExitCode;

use the_light_core::ai::{build_provider, KeyStore, LlmProvider};
use the_light_core::config::Config;
use the_light_core::model::{Passage, Reference, TranslationId};
use the_light_core::store::Store;

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
            eprintln!("{}", the_light_core::ai::AiError::NoProvider);
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

/// Passagem resolvida + se a fonte é embarcável (versão livre) ou protegida.
pub struct ResolvedPassage {
    /// A passagem resolvida.
    pub passage: Passage,
    /// `false` quando veio de um conector (versão protegida).
    pub embeddable: bool,
}

/// Resolve a passagem na versão pedida (CLI > config > kjv), consultando
/// versões locais e conectores protegidos (ver [`crate::sources::resolve`]).
/// Devolve também a embarcabilidade da fonte, para que o chamador avise/gate
/// versões protegidas (texto efêmero, nunca persistido).
pub fn resolve_passage(
    store: &Store,
    config: &Config,
    reference: &Reference,
    cli_version: Option<&str>,
) -> Result<ResolvedPassage, ExitCode> {
    let version = cli_version
        .map(str::to_string)
        .or_else(|| config.versions.first().cloned())
        .unwrap_or_else(|| "kjv".to_string());
    let source = crate::sources::resolve(store, config, &version).map_err(|m| {
        eprintln!("{m}");
        ExitCode::from(EXIT_NOT_FOUND)
    })?;
    let embeddable = source.is_embeddable();
    let tid = TranslationId::new(&version);
    match source.passage(reference, &tid) {
        Ok(p) if !p.verses.is_empty() => Ok(ResolvedPassage {
            passage: p,
            embeddable,
        }),
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
