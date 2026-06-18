//! Pesquisa web **opt-in** (desligada por padrão): busca fontes secundárias
//! **reais** para citar. NÓS buscamos as URLs/trechos e os entregamos ao modelo
//! — o modelo NUNCA inventa uma URL; só cita `[W:n]`, e o trecho é renderizado
//! **verbatim** na nota (ver `ai::citation`).
//!
//! Privacidade: cada consulta é explícita (flag `--research`) e a aplicação
//! mostra exatamente o que sai da máquina; o backend `mock` não usa rede.

use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{AiError, Result};

/// Tempo máximo por consulta (bem menor que o timeout do LLM, 120s).
const RESEARCH_TIMEOUT: Duration = Duration::from_secs(15);
const USER_AGENT: &str =
    "TheLight/1.0 (terminal Bible study; +https://github.com/butkeraites/the-light)";

/// Backends de pesquisa suportados.
pub const RESEARCH_BACKENDS: &[&str] = &["wikipedia", "tavily", "mock"];

/// Uma fonte secundária recuperada da web (real, citável).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebSource {
    /// Título da fonte.
    pub title: String,
    /// URL canônica.
    pub url: String,
    /// Trecho recuperado (citado verbatim na nota; nunca parafraseado).
    pub snippet: String,
    /// Domínio/origem (ex.: "en.wikipedia.org").
    pub site: String,
    /// Momento da recuperação (UTC).
    pub fetched_at: DateTime<Utc>,
}

/// Interface de um provedor de pesquisa web.
pub trait ResearchProvider {
    /// Nome do backend (ex.: "wikipedia").
    fn name(&self) -> &str;
    /// Busca `query`, devolvendo até `limit` fontes.
    fn search(&self, query: &str, limit: usize) -> Result<Vec<WebSource>>;
}

fn blocking_client() -> Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .timeout(RESEARCH_TIMEOUT)
        .user_agent(USER_AGENT)
        .build()
        .map_err(|e| AiError::Http(e.to_string()))
}

/// Percent-encoda um valor de query (RFC 3986 unreserved).
fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 2);
    for b in s.bytes() {
        let c = b as char;
        if c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_' | '~') {
            out.push(c);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Remove tags HTML simples de um trecho (Wikipedia devolve `<span>` de realce).
fn strip_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if in_tag => {}
            _ => out.push(c),
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

// ----------------------------------------------------------------------------
// Mock (testes/demos, sem rede).
// ----------------------------------------------------------------------------

/// Provedor falso: devolve fontes pré-definidas (sem rede).
pub struct MockResearchProvider {
    results: Vec<WebSource>,
}

impl MockResearchProvider {
    /// Cria um mock com fontes específicas.
    pub fn new(results: Vec<WebSource>) -> Self {
        MockResearchProvider { results }
    }

    /// Mock canônico com duas fontes (timestamp fixo, determinístico).
    pub fn canned() -> Self {
        let at = DateTime::<Utc>::from_timestamp(1_750_000_000, 0).unwrap();
        MockResearchProvider::new(vec![
            WebSource {
                title: "Grace (Christianity)".to_string(),
                url: "https://example.org/grace".to_string(),
                snippet: "Grace in Christianity is the free and unmerited favor of God."
                    .to_string(),
                site: "example.org".to_string(),
                fetched_at: at,
            },
            WebSource {
                title: "Sola gratia".to_string(),
                url: "https://example.org/sola-gratia".to_string(),
                snippet: "Sola gratia is one of the five solae of the Reformation.".to_string(),
                site: "example.org".to_string(),
                fetched_at: at,
            },
        ])
    }
}

impl ResearchProvider for MockResearchProvider {
    fn name(&self) -> &str {
        "mock"
    }
    fn search(&self, _query: &str, limit: usize) -> Result<Vec<WebSource>> {
        Ok(self.results.iter().take(limit).cloned().collect())
    }
}

// ----------------------------------------------------------------------------
// Wikipedia (keyless, real).
// ----------------------------------------------------------------------------

/// Provedor Wikipedia (API de busca pública, sem chave). Fonte secundária real
/// e atribuível — útil como padrão sem credenciais.
pub struct WikipediaProvider {
    lang: String,
}

impl WikipediaProvider {
    /// Provedor para uma edição de idioma da Wikipedia (ex.: "en", "pt").
    pub fn new(lang: impl Into<String>) -> Self {
        WikipediaProvider { lang: lang.into() }
    }
}

impl ResearchProvider for WikipediaProvider {
    fn name(&self) -> &str {
        "wikipedia"
    }

    fn search(&self, query: &str, limit: usize) -> Result<Vec<WebSource>> {
        let host = format!("{}.wikipedia.org", self.lang);
        let client = blocking_client()?;
        let url = format!(
            "https://{host}/w/api.php?action=query&list=search&srsearch={}&srlimit={}&format=json",
            urlencode(query),
            limit.clamp(1, 10)
        );
        let resp = client
            .get(&url)
            .send()
            .map_err(|e| AiError::Http(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(AiError::BadResponse(format!("HTTP {}", resp.status())));
        }
        let body: Value = resp
            .json()
            .map_err(|e| AiError::BadResponse(e.to_string()))?;
        let at = Utc::now();
        let mut out = Vec::new();
        if let Some(items) = body["query"]["search"].as_array() {
            for it in items {
                let title = it["title"].as_str().unwrap_or("").to_string();
                if title.is_empty() {
                    continue;
                }
                let snippet = strip_html(it["snippet"].as_str().unwrap_or(""));
                let url = format!("https://{host}/wiki/{}", title.replace(' ', "_"));
                out.push(WebSource {
                    title,
                    url,
                    snippet,
                    site: host.clone(),
                    fetched_at: at,
                });
            }
        }
        Ok(out)
    }
}

// ----------------------------------------------------------------------------
// Tavily (BYOK).
// ----------------------------------------------------------------------------

/// Provedor Tavily (busca otimizada p/ LLMs; requer chave em `research.tavily`).
pub struct TavilyProvider {
    key: String,
}

impl TavilyProvider {
    /// Provedor com a chave do usuário.
    pub fn new(key: impl Into<String>) -> Self {
        TavilyProvider { key: key.into() }
    }
}

impl ResearchProvider for TavilyProvider {
    fn name(&self) -> &str {
        "tavily"
    }

    fn search(&self, query: &str, limit: usize) -> Result<Vec<WebSource>> {
        let client = blocking_client()?;
        let body = serde_json::json!({
            "api_key": self.key,
            "query": query,
            "max_results": limit.clamp(1, 10),
            "search_depth": "basic",
        });
        let resp = client
            .post("https://api.tavily.com/search")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .map_err(|e| AiError::Http(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(AiError::BadResponse(format!(
                "Tavily HTTP {}",
                resp.status()
            )));
        }
        let v: Value = resp
            .json()
            .map_err(|e| AiError::BadResponse(e.to_string()))?;
        let at = Utc::now();
        let mut out = Vec::new();
        if let Some(items) = v["results"].as_array() {
            for it in items {
                let url = it["url"].as_str().unwrap_or("").to_string();
                if url.is_empty() {
                    continue;
                }
                let site = url
                    .split("://")
                    .nth(1)
                    .and_then(|s| s.split('/').next())
                    .unwrap_or("")
                    .to_string();
                out.push(WebSource {
                    title: it["title"].as_str().unwrap_or("(sem título)").to_string(),
                    url,
                    snippet: it["content"].as_str().unwrap_or("").to_string(),
                    site,
                    fetched_at: at,
                });
            }
        }
        Ok(out)
    }
}

/// Constrói um provedor de pesquisa a partir do nome do backend e da chave
/// opcional (do `KeyStore`, sob `research.<backend>`).
pub fn build_research_provider(
    backend: &str,
    key: Option<String>,
    lang: &str,
) -> Result<Box<dyn ResearchProvider>> {
    match backend.trim().to_ascii_lowercase().as_str() {
        "mock" => Ok(Box::new(MockResearchProvider::canned())),
        "wikipedia" | "wiki" => Ok(Box::new(WikipediaProvider::new(lang.to_string()))),
        "tavily" => {
            let key = key.ok_or_else(|| AiError::NoKey("research.tavily".to_string()))?;
            Ok(Box::new(TavilyProvider::new(key)))
        }
        other => Err(AiError::UnknownProvider(other.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_returns_canned_and_respects_limit() {
        let p = MockResearchProvider::canned();
        assert_eq!(p.name(), "mock");
        assert_eq!(p.search("graça", 5).unwrap().len(), 2);
        assert_eq!(p.search("graça", 1).unwrap().len(), 1);
    }

    #[test]
    fn strip_html_removes_tags() {
        assert_eq!(
            strip_html("Grace is <span class=\"x\">free</span> favor"),
            "Grace is free favor"
        );
    }

    #[test]
    fn build_provider_resolves_and_gates_keys() {
        assert!(build_research_provider("mock", None, "en").is_ok());
        assert!(build_research_provider("wikipedia", None, "pt").is_ok());
        // Tavily exige chave.
        assert!(matches!(
            build_research_provider("tavily", None, "en"),
            Err(AiError::NoKey(_))
        ));
        assert!(build_research_provider("tavily", Some("k".into()), "en").is_ok());
        assert!(matches!(
            build_research_provider("bing", None, "en"),
            Err(AiError::UnknownProvider(_))
        ));
    }

    #[test]
    fn websource_json_roundtrips() {
        let p = MockResearchProvider::canned();
        let src = &p.search("x", 1).unwrap()[0];
        let json = serde_json::to_string(src).unwrap();
        let back: WebSource = serde_json::from_str(&json).unwrap();
        assert_eq!(&back, src);
    }
}
