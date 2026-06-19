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

pub mod ai;
pub mod config;
pub mod export;
pub mod model;
pub mod reference;
pub mod scholarly;
pub mod search;
pub mod source;
pub mod store;
pub mod userdata;
pub mod util;
pub mod xref;

/// Versão do crate, exposta para `--version` da CLI.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
