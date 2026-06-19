//! Conversas de IA persistentes (uma sessão = um arquivo JSON em `sessions/`).
//!
//! Espelha o padrão de [`super::plans::PlanStore`]: arquivos abertos e
//! versionáveis, gravados de forma atômica ([`crate::util::atomic_write`]).
//! Os IDs e timestamps são gerados pelo chamador (a TUI), mantendo a store
//! testável com valores fixos.

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::Result;
use crate::ai::{ChatRole, StudyMode};
use crate::model::Lang;

/// Um turno de uma conversa de IA.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    /// Papel (usuário/assistente).
    pub role: ChatRole,
    /// Conteúdo (texto limpo do que foi digitado/respondido).
    pub content: String,
    /// Momento do turno (UTC).
    pub at: DateTime<Utc>,
}

/// Uma conversa de IA salva, retomável.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    /// Identificador estável (também o nome do arquivo).
    pub id: String,
    /// Título legível (derivado da primeira pergunta).
    pub title: String,
    /// Rótulo da passagem onde a conversa nasceu (ex.: "Romanos 3").
    pub anchor_label: String,
    /// Contexto RAG (capítulo numerado + refs) capturado no início.
    pub context: String,
    /// Idioma da conversa (para o prompt de sistema).
    pub lang: Lang,
    /// Provedor usado (ex.: "anthropic").
    pub provider: String,
    /// Modelo usado.
    pub model: String,
    /// Quando foi criada (UTC).
    pub created_at: DateTime<Utc>,
    /// Quando foi atualizada pela última vez (UTC).
    pub updated_at: DateTime<Utc>,
    /// Turnos da conversa, em ordem.
    #[serde(default)]
    pub messages: Vec<Message>,
    /// Modo do estudo, quando a sessão é um **estudo** (não uma conversa livre).
    /// `None` = conversa de IA comum. Distingue os dois no navegador (`s`).
    #[serde(default)]
    pub study_mode: Option<StudyMode>,
    /// Lente denominacional do estudo (slug), quando aplicável.
    #[serde(default)]
    pub study_lens: Option<String>,
}

impl Session {
    /// Gera um id estável baseado no instante atual (UTC, microssegundos).
    pub fn generate_id() -> String {
        Utc::now().format("%Y%m%dT%H%M%S%6fZ").to_string()
    }

    /// Inicia uma conversa vazia, carimbando criação/atualização com o agora.
    #[allow(clippy::too_many_arguments)]
    pub fn start(
        id: String,
        title: String,
        anchor_label: String,
        context: String,
        lang: Lang,
        provider: String,
        model: String,
    ) -> Self {
        let now = Utc::now();
        Session {
            id,
            title,
            anchor_label,
            context,
            lang,
            provider,
            model,
            created_at: now,
            updated_at: now,
            messages: Vec::new(),
            study_mode: None,
            study_lens: None,
        }
    }

    /// Anexa um turno (carimbando o agora) e atualiza `updated_at`.
    pub fn push(&mut self, role: ChatRole, content: String) {
        let now = Utc::now();
        self.messages.push(Message {
            role,
            content,
            at: now,
        });
        self.updated_at = now;
    }
}

/// Persistência das conversas (`sessions/<id>.json`).
pub struct SessionStore {
    dir: PathBuf,
}

impl SessionStore {
    /// Cria um store ligado ao diretório dado.
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        SessionStore { dir: dir.into() }
    }

    /// Store no diretório padrão (`sessions/`).
    pub fn open_default() -> Result<Self> {
        Ok(SessionStore::new(super::sessions_dir()?))
    }

    /// Caminho do arquivo de uma sessão pelo id.
    pub fn path_for(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.json"))
    }

    /// Lê uma sessão pelo id; `None` se não existir.
    pub fn get(&self, id: &str) -> Result<Option<Session>> {
        match std::fs::read_to_string(self.path_for(id)) {
            Ok(s) => Ok(Some(serde_json::from_str(&s)?)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Grava uma sessão (sobrescreve se já existir), de forma atômica.
    pub fn put(&self, session: &Session) -> Result<()> {
        let json = serde_json::to_string_pretty(session)?;
        crate::util::atomic_write(&self.path_for(&session.id), json.as_bytes())?;
        Ok(())
    }

    /// Remove uma sessão pelo id. Devolve `true` se havia.
    pub fn delete(&self, id: &str) -> Result<bool> {
        match std::fs::remove_file(self.path_for(id)) {
            Ok(()) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e.into()),
        }
    }

    /// Lista todas as sessões, da mais recente para a mais antiga
    /// (`updated_at` desc). Arquivos inválidos/corrompidos são ignorados.
    pub fn list(&self) -> Result<Vec<Session>> {
        let mut out: Vec<Session> = Vec::new();
        let entries = match std::fs::read_dir(&self.dir) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
            Err(e) => return Err(e.into()),
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            if let Ok(s) = std::fs::read_to_string(&path) {
                if let Ok(session) = serde_json::from_str::<Session>(&s) {
                    out.push(session);
                }
            }
        }
        out.sort_by_key(|s| std::cmp::Reverse(s.updated_at));
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ts(secs: i64) -> DateTime<Utc> {
        DateTime::<Utc>::from_timestamp(secs, 0).unwrap()
    }

    fn session(id: &str, title: &str, updated: i64) -> Session {
        Session {
            id: id.to_string(),
            title: title.to_string(),
            anchor_label: "Romans 3".to_string(),
            context: "Romans 3:\n23 For all have sinned".to_string(),
            lang: Lang::En,
            provider: "mock".to_string(),
            model: "mock-1".to_string(),
            created_at: ts(updated),
            updated_at: ts(updated),
            messages: vec![Message {
                role: ChatRole::User,
                content: "o que significa?".to_string(),
                at: ts(updated),
            }],
            study_mode: None,
            study_lens: None,
        }
    }

    #[test]
    fn put_get_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let store = SessionStore::new(dir.path());
        let s = session("20260616T120000Z", "primeira", 1000);
        store.put(&s).unwrap();
        let got = store.get("20260616T120000Z").unwrap().unwrap();
        assert_eq!(got, s);
        assert!(store.get("inexistente").unwrap().is_none());
    }

    #[test]
    fn list_sorted_by_updated_desc() {
        let dir = tempfile::tempdir().unwrap();
        let store = SessionStore::new(dir.path());
        store.put(&session("a", "antiga", 1000)).unwrap();
        store.put(&session("b", "nova", 2000)).unwrap();
        let list = store.list().unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].id, "b"); // mais recente primeiro
        assert_eq!(list[1].id, "a");
    }

    #[test]
    fn delete_reports_existence() {
        let dir = tempfile::tempdir().unwrap();
        let store = SessionStore::new(dir.path());
        store.put(&session("x", "t", 1)).unwrap();
        assert!(store.delete("x").unwrap());
        assert!(!store.delete("x").unwrap());
        assert!(store.get("x").unwrap().is_none());
    }

    #[test]
    fn list_missing_dir_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        let store = SessionStore::new(dir.path().join("nope"));
        assert!(store.list().unwrap().is_empty());
    }
}
