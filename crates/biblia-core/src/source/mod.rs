//! Camada de fontes de texto bíblico (trait [`BibleSource`]).
//!
//! Abstrai a origem do texto para isolar a fronteira legal (ver `SPEC.md` §5.2):
//! versões livres vêm de [`embedded::EmbeddedSource`] (SQLite local); versões
//! protegidas virão de conectores opt-in em fases posteriores.

pub mod apibible;
pub mod embedded;
pub mod esv;
mod http;

pub use apibible::ApiBibleSource;
pub use embedded::EmbeddedSource;
pub use esv::EsvApiSource;

use crate::model::{Passage, Reference, SearchHit, Translation, TranslationId};
use crate::search::SearchOptions;

/// Erros da camada de fontes.
#[derive(Debug, thiserror::Error)]
pub enum SourceError {
    /// Erro propagado da abertura/migração do banco.
    #[error(transparent)]
    Store(#[from] crate::store::StoreError),
    /// Erro propagado do SQLite.
    #[error("erro de SQLite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// A tradução pedida não está disponível nesta fonte.
    #[error("versão desconhecida: {0}")]
    UnknownTranslation(String),
    /// Erro de rede/HTTP num conector.
    #[error("erro de rede: {0}")]
    Http(String),
    /// Operação não suportada por esta fonte (ex.: busca em conector).
    #[error("operação não suportada por esta fonte: {0}")]
    Unsupported(String),
}

/// Resultado da camada de fontes.
pub type Result<T> = std::result::Result<T, SourceError>;

/// Origem de texto bíblico (local embarcado ou conector remoto).
pub trait BibleSource {
    /// Lista as traduções disponíveis nesta fonte.
    fn translations(&self) -> Result<Vec<Translation>>;

    /// `true` se a tradução `t` está disponível nesta fonte.
    fn has_translation(&self, t: &TranslationId) -> Result<bool>;

    /// Resolve uma passagem na tradução `t`. Pode retornar uma [`Passage`] vazia
    /// se a referência for válida mas não existir texto (ex.: capítulo fora do
    /// alcance do livro).
    fn passage(&self, r: &Reference, t: &TranslationId) -> Result<Passage>;

    /// Busca full-text por relevância, conforme [`SearchOptions`].
    fn search(&self, query: &str, opts: &SearchOptions) -> Result<Vec<SearchHit>>;

    /// `true` se o texto desta fonte pode ser embarcado/redistribuído.
    fn is_embeddable(&self) -> bool;
}
