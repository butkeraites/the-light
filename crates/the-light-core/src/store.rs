//! Abertura, migração e validação do banco SQLite embarcado.
//!
//! O banco guarda apenas o **texto bíblico** (read-only após import). Dados do
//! usuário (notas, marcações, planos) vivem em arquivos — ver `SPEC.md` §5.3.
//!
//! As migrações ficam em `migrations/` e são embutidas no binário com
//! `include_str!`; a versão aplicada é rastreada por `PRAGMA user_version`.

use rusqlite::Connection;
use std::path::{Path, PathBuf};

/// Migrações SQL em ordem. O índice `i` corresponde a `user_version = i + 1`.
const MIGRATIONS: &[&str] = &[
    include_str!("../migrations/v1_initial.sql"),
    include_str!("../migrations/v2_scholarly.sql"),
];

/// Versão de esquema mais recente conhecida por este binário.
pub const SCHEMA_VERSION: i64 = MIGRATIONS.len() as i64;

/// Erros da camada de armazenamento.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// Erro propagado do SQLite.
    #[error("erro de SQLite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// Erro de I/O ao criar diretórios/arquivos.
    #[error("erro de I/O: {0}")]
    Io(#[from] std::io::Error),
    /// A build do SQLite não tem FTS5 — a busca full-text não funcionaria.
    #[error("FTS5 não está disponível nesta build do SQLite (recompile rusqlite com `bundled`)")]
    Fts5Unavailable,
    /// Não foi possível determinar o diretório de dados do usuário.
    #[error("não foi possível determinar o diretório de dados do usuário")]
    NoDataDir,
}

/// Resultado da camada de armazenamento.
pub type Result<T> = std::result::Result<T, StoreError>;

/// Banco de texto bíblico aberto e migrado.
pub struct Store {
    conn: Connection,
}

impl Store {
    /// Abre (ou cria) o banco no caminho dado, garante FTS5 e aplica migrações.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let conn = Connection::open(path)?;
        Self::from_connection(conn)
    }

    /// Abre um banco em memória (usado em testes).
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        Self::from_connection(conn)
    }

    /// Abre o banco no caminho padrão do usuário (XDG/SO), criando-o se preciso.
    pub fn open_default() -> Result<Self> {
        Self::open(Self::default_db_path()?)
    }

    /// Caminho padrão do arquivo de banco (`<data_dir>/biblia.sqlite`).
    ///
    /// Linux: `~/.local/share/light/`; macOS: `~/Library/Application Support/light/`.
    pub fn default_db_path() -> Result<PathBuf> {
        let dirs = directories::ProjectDirs::from("", "", "light").ok_or(StoreError::NoDataDir)?;
        Ok(dirs.data_dir().join("biblia.sqlite"))
    }

    fn from_connection(conn: Connection) -> Result<Self> {
        conn.pragma_update(None, "foreign_keys", true)?;
        ensure_fts5(&conn)?;
        migrate(&conn)?;
        Ok(Store { conn })
    }

    /// Acesso de leitura à conexão subjacente.
    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    /// Acesso mutável à conexão (para transações de import).
    pub fn conn_mut(&mut self) -> &mut Connection {
        &mut self.conn
    }

    /// Versão de esquema atualmente aplicada (`PRAGMA user_version`).
    pub fn schema_version(&self) -> Result<i64> {
        schema_version(&self.conn)
    }

    /// Consome o `Store` devolvendo a conexão.
    pub fn into_connection(self) -> Connection {
        self.conn
    }
}

/// Verifica se a build do SQLite suporta FTS5 criando uma tabela virtual
/// temporária de teste.
fn ensure_fts5(conn: &Connection) -> Result<()> {
    let probe = conn.execute_batch(
        "CREATE VIRTUAL TABLE IF NOT EXISTS temp.__light_fts_probe USING fts5(x);\
         DROP TABLE temp.__light_fts_probe;",
    );
    match probe {
        Ok(()) => Ok(()),
        Err(_) => Err(StoreError::Fts5Unavailable),
    }
}

/// Lê `PRAGMA user_version`.
fn schema_version(conn: &Connection) -> Result<i64> {
    let v = conn.query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))?;
    Ok(v)
}

/// Aplica as migrações pendentes, em ordem, dentro de uma transação cada,
/// atualizando `user_version` ao final de cada uma.
fn migrate(conn: &Connection) -> Result<()> {
    let current = schema_version(conn)?;
    for (i, sql) in MIGRATIONS.iter().enumerate() {
        let target = (i + 1) as i64;
        if target <= current {
            continue;
        }
        conn.execute_batch("BEGIN")?;
        match conn.execute_batch(sql) {
            Ok(()) => {
                // `user_version` não aceita parâmetro vinculado; é seguro pois
                // `target` é um inteiro derivado do índice da migração.
                conn.execute_batch(&format!("PRAGMA user_version = {target};"))?;
                conn.execute_batch("COMMIT")?;
            }
            Err(e) => {
                let _ = conn.execute_batch("ROLLBACK");
                return Err(StoreError::Sqlite(e));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table_names(conn: &Connection) -> Vec<String> {
        let mut stmt = conn
            .prepare("SELECT name FROM sqlite_master WHERE type IN ('table','index') ORDER BY name")
            .unwrap();
        let rows = stmt
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .collect::<std::result::Result<Vec<_>, _>>()
            .unwrap();
        rows
    }

    #[test]
    fn fresh_db_is_migrated_to_latest() {
        let store = Store::open_in_memory().unwrap();
        assert_eq!(store.schema_version().unwrap(), SCHEMA_VERSION);
        assert_eq!(SCHEMA_VERSION, 2);
    }

    #[test]
    fn migration_creates_all_tables() {
        let store = Store::open_in_memory().unwrap();
        let names = table_names(store.conn());
        for expected in [
            // v1 — texto bíblico
            "translations",
            "books",
            "verses",
            "verses_fts",
            "cross_references",
            "idx_verses_lookup",
            "idx_xref_from",
            // v2 — dados acadêmicos (vazios até `import-scholarly`)
            "scholarly_sources",
            "original_tokens",
            "lexicon",
            "morph_legend",
            "versification_map",
            "idx_tokens_strongs",
            "idx_lexicon_strongs",
        ] {
            assert!(
                names.iter().any(|n| n == expected),
                "faltou `{expected}` em {names:?}"
            );
        }
    }

    #[test]
    fn scholarly_tables_are_empty_on_a_base_db() {
        // O DB base (sem `import-scholarly`) tem o esquema v2 com tabelas vazias —
        // este é o caso offline normal, não um erro. O grounding deve tolerá-lo.
        let store = Store::open_in_memory().unwrap();
        let n: i64 = store
            .conn()
            .query_row("SELECT count(*) FROM original_tokens", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn fts5_is_available_and_usable() {
        let store = Store::open_in_memory().unwrap();
        // Inserir e buscar no índice FTS5 confirma que a tabela virtual funciona.
        let conn = store.conn();
        conn.execute(
            "INSERT INTO verses_fts(text, translation_id, verse_id) VALUES (?1, ?2, ?3)",
            rusqlite::params!["No princípio criou Deus os céus e a terra", "alm1911", 1i64],
        )
        .unwrap();
        // 'remove_diacritics 2' → busca sem acento casa com texto acentuado.
        let hits: i64 = conn
            .query_row(
                "SELECT count(*) FROM verses_fts WHERE verses_fts MATCH 'ceus'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(hits, 1, "FTS5 deveria casar 'ceus' com 'céus'");
    }

    #[test]
    fn migration_is_idempotent() {
        let store = Store::open_in_memory().unwrap();
        // Rodar a migração de novo na mesma conexão não deve recriar tabelas.
        migrate(store.conn()).unwrap();
        assert_eq!(store.schema_version().unwrap(), SCHEMA_VERSION);
    }

    #[test]
    fn reopening_file_db_keeps_schema_version() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("biblia.sqlite");
        {
            let store = Store::open(&path).unwrap();
            assert_eq!(store.schema_version().unwrap(), SCHEMA_VERSION);
        }
        // Reabrir não deve reaplicar migrações (user_version já está no alvo).
        let store = Store::open(&path).unwrap();
        assert_eq!(store.schema_version().unwrap(), SCHEMA_VERSION);
        assert!(table_names(store.conn()).iter().any(|n| n == "verses"));
    }

    #[test]
    fn foreign_keys_enforced() {
        let store = Store::open_in_memory().unwrap();
        // Inserir um livro sem a tradução referenciada deve violar a FK.
        let res = store.conn().execute(
            "INSERT INTO books(translation_id, number, name, abbrev, testament) \
             VALUES ('inexistente', 1, 'Genesis', 'Gn', 'OT')",
            [],
        );
        assert!(res.is_err(), "FK deveria bloquear tradução inexistente");
    }
}
