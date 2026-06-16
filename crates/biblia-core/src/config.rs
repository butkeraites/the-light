//! Configuração do usuário em `config.toml` (paths XDG/SO).
//!
//! Guarda preferências: versões padrão, idioma, tema e tamanho de fonte.
//! O caminho pode ser sobrescrito pela variável de ambiente `BIBLIA_CONFIG`
//! (útil para testes e cenários avançados).

use std::path::{Path, PathBuf};
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::model::Lang;

/// Chaves configuráveis (para `config set/get/list`).
pub const KEYS: &[&str] = &["versions", "language", "theme", "font-size", "provider"];

/// Preferências do usuário.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Versões padrão usadas pelo `read` quando `--version` é omitido.
    pub versions: Vec<String>,
    /// Idioma padrão de exibição (mensagens/formatação de referência).
    pub language: Lang,
    /// Nome do tema de cores.
    pub theme: String,
    /// Tamanho de fonte (onde o terminal/TUI permitir).
    pub font_size: Option<u16>,
    /// Provedor de IA ativo (`anthropic`/`openai`/`ollama`); vazio = nenhum.
    /// Não é segredo; as chaves ficam fora do `config.toml` (ver `ai::KeyStore`).
    pub provider: String,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            versions: vec!["kjv".to_string()],
            language: Lang::Pt,
            theme: "auto".to_string(),
            font_size: None,
            provider: String::new(),
        }
    }
}

/// Erros de configuração.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// Erro de I/O ao ler/gravar o arquivo.
    #[error("erro de I/O: {0}")]
    Io(#[from] std::io::Error),
    /// Falha ao parsear o TOML.
    #[error("config.toml inválido: {0}")]
    Parse(#[from] toml::de::Error),
    /// Falha ao serializar o TOML.
    #[error("erro ao serializar config: {0}")]
    Serialize(#[from] toml::ser::Error),
    /// Chave de configuração desconhecida.
    #[error("chave desconhecida: {0:?} (válidas: versions, language, theme, font-size)")]
    UnknownKey(String),
    /// Valor inválido para a chave.
    #[error("valor inválido para {key}: {value:?}")]
    BadValue {
        /// Chave afetada.
        key: String,
        /// Valor rejeitado.
        value: String,
    },
    /// Não foi possível determinar o diretório de configuração.
    #[error("não foi possível determinar o diretório de configuração")]
    NoConfigDir,
}

/// Resultado das operações de configuração.
pub type Result<T> = std::result::Result<T, ConfigError>;

fn normalize_key(key: &str) -> String {
    key.trim().to_ascii_lowercase().replace('_', "-")
}

impl Config {
    /// Caminho do `config.toml` (env `BIBLIA_CONFIG` tem prioridade).
    pub fn config_path() -> Result<PathBuf> {
        if let Some(p) = std::env::var_os("BIBLIA_CONFIG") {
            return Ok(PathBuf::from(p));
        }
        let dirs =
            directories::ProjectDirs::from("", "", "biblia").ok_or(ConfigError::NoConfigDir)?;
        Ok(dirs.config_dir().join("config.toml"))
    }

    /// Carrega do caminho padrão; ausência de arquivo devolve os padrões.
    pub fn load() -> Result<Config> {
        Self::load_from(&Self::config_path()?)
    }

    /// Carrega de um caminho específico; ausência de arquivo devolve os padrões.
    pub fn load_from(path: &Path) -> Result<Config> {
        match std::fs::read_to_string(path) {
            Ok(s) => Ok(toml::from_str(&s)?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
            Err(e) => Err(e.into()),
        }
    }

    /// Grava no caminho padrão (criando diretórios).
    pub fn save(&self) -> Result<()> {
        self.save_to(&Self::config_path()?)
    }

    /// Grava num caminho específico (criando diretórios), de forma **atômica**
    /// (ver [`crate::util::atomic_write`]).
    pub fn save_to(&self, path: &Path) -> Result<()> {
        let data = toml::to_string_pretty(self)?;
        crate::util::atomic_write(path, data.as_bytes())?;
        Ok(())
    }

    /// Define uma chave a partir de texto (validando o valor).
    pub fn set(&mut self, key: &str, value: &str) -> Result<()> {
        match normalize_key(key).as_str() {
            "versions" => {
                let vs: Vec<String> = value
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                if vs.is_empty() {
                    return Err(ConfigError::BadValue {
                        key: "versions".to_string(),
                        value: value.to_string(),
                    });
                }
                self.versions = vs;
            }
            "language" => {
                self.language = Lang::from_str(value).map_err(|_| ConfigError::BadValue {
                    key: "language".to_string(),
                    value: value.to_string(),
                })?;
            }
            "theme" => {
                let t = value.trim();
                if t.is_empty() {
                    return Err(ConfigError::BadValue {
                        key: "theme".to_string(),
                        value: value.to_string(),
                    });
                }
                self.theme = t.to_string();
            }
            "font-size" => {
                let v = value.trim();
                if v.is_empty() || v.eq_ignore_ascii_case("none") {
                    self.font_size = None;
                } else {
                    self.font_size = Some(v.parse().map_err(|_| ConfigError::BadValue {
                        key: "font-size".to_string(),
                        value: value.to_string(),
                    })?);
                }
            }
            "provider" => self.provider = value.trim().to_ascii_lowercase(),
            _ => return Err(ConfigError::UnknownKey(key.to_string())),
        }
        Ok(())
    }

    /// Lê uma chave como texto. `None` se a chave é desconhecida.
    pub fn get(&self, key: &str) -> Option<String> {
        match normalize_key(key).as_str() {
            "versions" => Some(self.versions.join(",")),
            "language" => Some(self.language.code().to_string()),
            "theme" => Some(self.theme.clone()),
            "font-size" => Some(
                self.font_size
                    .map(|n| n.to_string())
                    .unwrap_or_else(|| "none".to_string()),
            ),
            "provider" => Some(self.provider.clone()),
            _ => None,
        }
    }

    /// Todas as chaves e valores, em ordem estável (para `config list`).
    pub fn entries(&self) -> Vec<(String, String)> {
        KEYS.iter()
            .map(|k| ((*k).to_string(), self.get(k).unwrap_or_default()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_sensible() {
        let c = Config::default();
        assert_eq!(c.versions, vec!["kjv".to_string()]);
        assert_eq!(c.language, Lang::Pt);
        assert_eq!(c.theme, "auto");
        assert_eq!(c.font_size, None);
    }

    #[test]
    fn set_and_get_roundtrip_each_key() {
        let mut c = Config::default();
        c.set("versions", "kjv, alm1911 ,").unwrap();
        assert_eq!(c.versions, vec!["kjv".to_string(), "alm1911".to_string()]);
        assert_eq!(c.get("versions").as_deref(), Some("kjv,alm1911"));

        c.set("language", "en").unwrap();
        assert_eq!(c.get("language").as_deref(), Some("en"));

        c.set("theme", "dark").unwrap();
        assert_eq!(c.get("theme").as_deref(), Some("dark"));

        c.set("font-size", "16").unwrap();
        assert_eq!(c.get("font-size").as_deref(), Some("16"));
        c.set("font_size", "none").unwrap(); // aceita underscore e "none"
        assert_eq!(c.get("font-size").as_deref(), Some("none"));
    }

    #[test]
    fn set_rejects_unknown_key_and_bad_value() {
        let mut c = Config::default();
        assert!(matches!(
            c.set("nope", "x"),
            Err(ConfigError::UnknownKey(_))
        ));
        assert!(matches!(
            c.set("language", "klingon"),
            Err(ConfigError::BadValue { .. })
        ));
        assert!(matches!(
            c.set("font-size", "big"),
            Err(ConfigError::BadValue { .. })
        ));
        assert!(matches!(
            c.set("versions", " , "),
            Err(ConfigError::BadValue { .. })
        ));
    }

    #[test]
    fn file_roundtrip_persists() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub").join("config.toml");
        let mut c = Config::default();
        c.set("versions", "alm1911,kjv").unwrap();
        c.set("theme", "solarized").unwrap();
        c.save_to(&path).unwrap();

        let loaded = Config::load_from(&path).unwrap();
        assert_eq!(loaded, c);
    }

    #[test]
    fn missing_file_yields_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("inexistente.toml");
        assert_eq!(Config::load_from(&path).unwrap(), Config::default());
    }

    #[test]
    fn partial_toml_fills_missing_with_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "theme = \"dark\"\n").unwrap();
        let c = Config::load_from(&path).unwrap();
        assert_eq!(c.theme, "dark");
        // Campos ausentes mantêm o padrão.
        assert_eq!(c.versions, vec!["kjv".to_string()]);
        assert_eq!(c.language, Lang::Pt);
    }
}
