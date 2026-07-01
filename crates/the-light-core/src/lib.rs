//! `the-light-core` — lógica pura da The Light, sem I/O de terminal.
//!
//! Camadas planejadas (ver `SPEC.md` §5 e `IMPLEMENTATION_PLAN.md` §1):
//! - `model`: tipos de domínio (`Reference`, `Verse`, `Passage`, `Translation`...).
//! - `reference`: parser de referências bíblicas PT/EN.
//! - `store`: abertura/migração do banco SQLite embarcado.
//! - `source`, `search`, `userdata`, `xref`, `ai`, `config`: fases posteriores.
//!
//! Os módulos são introduzidos tarefa a tarefa (ver `PROGRESS.md`).
//! Crates de interface (`the-light-cli`, `the-light-tui`) dependem deste núcleo.

// Núcleo PURO — sempre disponível (compila para wasm32 sem store/rede):
pub mod model;
pub mod reference;

// Camada PESADA — só com a feature `embedded` (store SQLite, rede, persistência).
// Sem `embedded`, estes módulos (e suas deps rusqlite/reqwest/directories/…) não
// são compilados, permitindo o build wasm da fronteira que usa só `reference`.
//
// `ai` é a exceção: sua **superfície pura** (prompt/RAG/`ask`/citação) também
// compila sob a feature fina `ai-pure` (sem reqwest/rusqlite), para a IA no web
// (ADR-0024/D2). As partes pesadas do `ai` seguem gateadas por `embedded`, item a
// item, dentro do módulo. Os demais módulos pesados continuam só sob `embedded`.
#[cfg(any(feature = "embedded", feature = "ai-pure"))]
pub mod ai;
#[cfg(feature = "embedded")]
pub mod config;
#[cfg(feature = "embedded")]
pub mod export;
#[cfg(feature = "embedded")]
pub mod scholarly;
#[cfg(feature = "embedded")]
pub mod search;
#[cfg(feature = "embedded")]
pub mod source;
#[cfg(feature = "embedded")]
pub mod store;
#[cfg(feature = "embedded")]
pub mod userdata;
#[cfg(feature = "embedded")]
pub mod util;
#[cfg(feature = "embedded")]
pub mod xref;

/// Versão do crate, exposta para `--version` da CLI.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
