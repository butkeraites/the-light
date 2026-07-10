//! Dados do usuário em arquivos abertos e versionáveis (ver `SPEC.md` §5.3).
//!
//! Layout (sob o diretório de dados do SO, ou `LIGHT_DATA_DIR`):
//! - `highlights.json` — marcações.
//! - `notes/` — uma nota Markdown por versículo/intervalo.
//! - `reading-plans/` — planos (fase 4).
//! - `studies/` — estudos de IA (fase 5).

// `plans` tem a GERAÇÃO PURA (wasm-safe); os demais (notas/marcações/sessões) são
// persistência em disco → `embedded`. `plans::PlanStore` (fs) também é `embedded`.
#[cfg(feature = "embedded")]
pub mod highlights;
#[cfg(feature = "embedded")]
pub mod notes;
// `note_slug` é PURO (ai-pure/wasm-safe): o formato do nome de arquivo da nota, fonte
// única p/ o nativo (fs) e o web (OPFS). ADR-0062.
pub mod note_slug;
pub mod plans;
#[cfg(feature = "embedded")]
pub mod sessions;

#[cfg(feature = "embedded")]
pub use highlights::{Highlight, HighlightStore};
#[cfg(feature = "embedded")]
pub use notes::{Note, NoteStore};
// Superfície PURA de planos (disponível em wasm sob `ai-pure`): tipos + geração.
pub use plans::{Plan, PlanProgress};
// Persistência do plano ativo (fs) — só `embedded`.
#[cfg(feature = "embedded")]
pub use plans::PlanStore;
#[cfg(feature = "embedded")]
pub use sessions::{Message, Session, SessionStore};

#[cfg(feature = "embedded")]
use std::path::PathBuf;

/// Erros da camada de dados do usuário.
#[derive(Debug, thiserror::Error)]
pub enum UserDataError {
    /// Erro de I/O.
    #[error("erro de I/O: {0}")]
    Io(#[from] std::io::Error),
    /// JSON inválido.
    #[error("JSON inválido: {0}")]
    Json(#[from] serde_json::Error),
    /// Não foi possível determinar o diretório de dados do usuário.
    #[error("não foi possível determinar o diretório de dados do usuário")]
    NoDataDir,
}

/// Resultado da camada de dados do usuário.
pub type Result<T> = std::result::Result<T, UserDataError>;

/// Diretório base de dados do usuário (`LIGHT_DATA_DIR` tem prioridade).
///
/// Linux: `~/.local/share/light/`; macOS: `~/Library/Application Support/light/`.
#[cfg(feature = "embedded")]
pub fn data_dir() -> Result<PathBuf> {
    if let Some(p) = std::env::var_os("LIGHT_DATA_DIR") {
        return Ok(PathBuf::from(p));
    }
    let dirs = directories::ProjectDirs::from("", "", "light").ok_or(UserDataError::NoDataDir)?;
    Ok(dirs.data_dir().to_path_buf())
}

/// Caminho do arquivo de marcações (`highlights.json`).
#[cfg(feature = "embedded")]
pub fn highlights_path() -> Result<PathBuf> {
    Ok(data_dir()?.join("highlights.json"))
}

/// Diretório das notas (`notes/`).
#[cfg(feature = "embedded")]
pub fn notes_dir() -> Result<PathBuf> {
    Ok(data_dir()?.join("notes"))
}

/// Diretório dos estudos de IA (`studies/`, fase 5).
#[cfg(feature = "embedded")]
pub fn studies_dir() -> Result<PathBuf> {
    Ok(data_dir()?.join("studies"))
}

/// Diretório dos planos de leitura (`reading-plans/`).
#[cfg(feature = "embedded")]
pub fn reading_plans_dir() -> Result<PathBuf> {
    Ok(data_dir()?.join("reading-plans"))
}

/// Diretório das conversas de IA (`sessions/`).
#[cfg(feature = "embedded")]
pub fn sessions_dir() -> Result<PathBuf> {
    Ok(data_dir()?.join("sessions"))
}

/// Diretório da pesquisa web opt-in (`research/`): cache + `log.jsonl`.
#[cfg(feature = "embedded")]
pub fn research_dir() -> Result<PathBuf> {
    Ok(data_dir()?.join("research"))
}
