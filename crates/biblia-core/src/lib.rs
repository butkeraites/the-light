//! `biblia-core` — lógica pura da Bíblia CLI, sem I/O de terminal.
//!
//! Camadas planejadas (ver `SPEC.md` §5 e `IMPLEMENTATION_PLAN.md` §1):
//! - `model`: tipos de domínio (`Reference`, `Verse`, `Passage`, `Translation`...).
//! - `reference`: parser de referências bíblicas PT/EN.
//! - `store`: abertura/migração do banco SQLite embarcado.
//! - `source`, `search`, `userdata`, `xref`, `ai`, `config`: fases posteriores.
//!
//! Os módulos são introduzidos tarefa a tarefa (ver `PROGRESS.md`).
//! Crates de interface (`biblia-cli`, `biblia-tui`) dependem deste núcleo.

pub mod model;
pub mod reference;
pub mod store;

/// Versão do crate, exposta para `--version` da CLI.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
