//! Armazenamento de chaves de API (BYOK).
//!
//! As chaves vivem num `secrets.toml` separado do `config.toml`, com permissão
//! restrita (`0600` no Unix) e **fora do git** (ver `.gitignore`). O caminho
//! pode ser sobrescrito por `BIBLIA_SECRETS` (testes/cenários avançados).
//!
//! > Segurança: as chaves nunca são logadas nem impressas; `list_providers`
//! > devolve apenas os nomes, nunca os valores.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::{AiError, Result};

#[derive(Debug, Default, Serialize, Deserialize)]
struct Secrets {
    #[serde(default)]
    keys: BTreeMap<String, String>,
}

/// Cofre de chaves ligado a um arquivo `secrets.toml`.
pub struct KeyStore {
    path: PathBuf,
    secrets: Secrets,
}

impl KeyStore {
    /// Caminho do arquivo de segredos (`BIBLIA_SECRETS` tem prioridade).
    pub fn secrets_path() -> Result<PathBuf> {
        if let Some(p) = std::env::var_os("BIBLIA_SECRETS") {
            return Ok(PathBuf::from(p));
        }
        let dirs = directories::ProjectDirs::from("", "", "biblia").ok_or(AiError::NoConfigDir)?;
        Ok(dirs.config_dir().join("secrets.toml"))
    }

    /// Abre o cofre no caminho dado (ausência → vazio).
    pub fn open(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        let secrets = match std::fs::read_to_string(&path) {
            Ok(s) => toml::from_str(&s)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Secrets::default(),
            Err(e) => return Err(AiError::Io(e)),
        };
        Ok(KeyStore { path, secrets })
    }

    /// Abre o cofre no caminho padrão.
    pub fn open_default() -> Result<Self> {
        Self::open(Self::secrets_path()?)
    }

    /// Lê a chave de um provedor.
    pub fn get(&self, provider: &str) -> Option<&str> {
        self.secrets.keys.get(provider).map(String::as_str)
    }

    /// Define/atualiza a chave de um provedor e grava (atômico, `0600`).
    pub fn set(&mut self, provider: &str, key: &str) -> Result<()> {
        self.secrets
            .keys
            .insert(provider.trim().to_ascii_lowercase(), key.trim().to_string());
        self.save()
    }

    /// Remove a chave de um provedor. `true` se havia. Grava se removeu.
    pub fn remove(&mut self, provider: &str) -> Result<bool> {
        let removed = self
            .secrets
            .keys
            .remove(&provider.trim().to_ascii_lowercase())
            .is_some();
        if removed {
            self.save()?;
        }
        Ok(removed)
    }

    /// Nomes dos provedores com chave (NUNCA os valores).
    pub fn list_providers(&self) -> Vec<String> {
        self.secrets.keys.keys().cloned().collect()
    }

    fn save(&self) -> Result<()> {
        let toml = toml::to_string_pretty(&self.secrets)?;
        crate::util::atomic_write(&self.path, toml.as_bytes())?;
        // Garante permissão restrita no Unix (o tempfile já cria 0600, mas
        // reforçamos caso o arquivo de destino preexista com outra permissão).
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&self.path, std::fs::Permissions::from_mode(0o600))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_get_remove_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("secrets.toml");
        {
            let mut ks = KeyStore::open(&path).unwrap();
            assert!(ks.get("anthropic").is_none());
            ks.set("anthropic", "sk-ant-123").unwrap();
            ks.set("OpenAI", "sk-oai-456").unwrap(); // normaliza p/ minúsculas
        }
        let ks = KeyStore::open(&path).unwrap();
        assert_eq!(ks.get("anthropic"), Some("sk-ant-123"));
        assert_eq!(ks.get("openai"), Some("sk-oai-456"));
        assert_eq!(
            ks.list_providers(),
            vec!["anthropic".to_string(), "openai".to_string()]
        );

        let mut ks = KeyStore::open(&path).unwrap();
        assert!(ks.remove("anthropic").unwrap());
        assert!(!ks.remove("anthropic").unwrap());
        assert!(ks.get("anthropic").is_none());
    }

    #[cfg(unix)]
    #[test]
    fn secrets_file_is_0600() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("secrets.toml");
        let mut ks = KeyStore::open(&path).unwrap();
        ks.set("anthropic", "sk-x").unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "secrets deve ser 0600, é {mode:o}");
    }

    #[test]
    fn missing_file_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        let ks = KeyStore::open(dir.path().join("absent.toml")).unwrap();
        assert!(ks.list_providers().is_empty());
    }
}
