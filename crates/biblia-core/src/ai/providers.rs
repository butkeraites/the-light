//! Provedores concretos de LLM (BYOK): Anthropic, OpenAI e Ollama.
//!
//! O Rust não tem SDK oficial da Anthropic, então usamos HTTP direto (reqwest
//! *blocking*). Os corpos de requisição e o parsing das respostas são funções
//! puras (testáveis sem rede); só `complete()` faz I/O. Os testes nunca chamam
//! a rede nem usam chaves reais — ver `MockLlmProvider`.

use std::time::Duration;

use serde_json::{json, Value};

use super::{AiError, LlmProvider, Result};

const HTTP_TIMEOUT: Duration = Duration::from_secs(120);
const DEFAULT_MAX_TOKENS: u32 = 8192;

/// Modelo padrão por provedor (quando não configurado).
pub fn default_model(provider: &str) -> &'static str {
    match provider {
        "anthropic" => "claude-opus-4-8",
        "openai" => "gpt-4o",
        "ollama" => "llama3",
        _ => "",
    }
}

/// Constrói o provedor ativo a partir do nome, chave e modelo opcionais.
///
/// `key` é exigida para anthropic/openai; ollama é local (chave opcional).
/// `mock` é sempre disponível (sem rede), útil para testes/demonstração.
pub fn build_provider(
    name: &str,
    key: Option<String>,
    model: Option<String>,
) -> Result<Box<dyn LlmProvider>> {
    let model = model
        .filter(|m| !m.trim().is_empty())
        .unwrap_or_else(|| default_model(name).to_string());
    match name {
        "mock" => Ok(Box::new(super::MockLlmProvider::default())),
        "anthropic" => {
            let key = key.ok_or_else(|| AiError::NoKey("anthropic".into()))?;
            Ok(Box::new(AnthropicProvider { key, model }))
        }
        "openai" => {
            let key = key.ok_or_else(|| AiError::NoKey("openai".into()))?;
            Ok(Box::new(OpenAiProvider { key, model }))
        }
        "ollama" => {
            let host = std::env::var("BIBLIA_OLLAMA_HOST")
                .unwrap_or_else(|_| "http://localhost:11434".to_string());
            Ok(Box::new(OllamaProvider { host, model }))
        }
        other => Err(AiError::UnknownProvider(other.to_string())),
    }
}

/// Estimativa grosseira de custo em USD (entrada+saída), se o modelo for conhecido.
///
/// Preços por milhão de tokens (jun/2026); `None` = desconhecido/local.
pub fn estimate_cost_usd(model: &str, input_tokens: usize, output_tokens: usize) -> Option<f64> {
    // (entrada $/Mtok, saída $/Mtok)
    let (pin, pout) = match model {
        "claude-opus-4-8" | "claude-opus-4-7" | "claude-opus-4-6" => (5.0, 25.0),
        "claude-sonnet-4-6" => (3.0, 15.0),
        "claude-haiku-4-5" => (1.0, 5.0),
        "gpt-4o" => (2.5, 10.0),
        m if m.starts_with("llama") => return Some(0.0), // local
        _ => return None,
    };
    let cost = (input_tokens as f64 * pin + output_tokens as f64 * pout) / 1_000_000.0;
    Some(cost)
}

fn blocking_client() -> Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .timeout(HTTP_TIMEOUT)
        .build()
        .map_err(|e| AiError::Http(e.to_string()))
}

// ---------------------------------------------------------------------------
// Anthropic
// ---------------------------------------------------------------------------

/// Provedor Anthropic (API de Mensagens, HTTP direto).
pub struct AnthropicProvider {
    key: String,
    model: String,
}

fn anthropic_body(model: &str, system: &str, user: &str, max_tokens: u32) -> Value {
    json!({
        "model": model,
        "max_tokens": max_tokens,
        "system": system,
        // Pensamento adaptativo: melhora o raciocínio exegético (Opus 4.8).
        "thinking": { "type": "adaptive" },
        "messages": [ { "role": "user", "content": user } ],
    })
}

/// Extrai o texto dos blocos `type == "text"` (ignora blocos de pensamento).
fn anthropic_extract(v: &Value) -> Result<String> {
    if v.get("stop_reason").and_then(Value::as_str) == Some("refusal") {
        return Err(AiError::BadResponse(
            "o modelo recusou a solicitação (refusal)".into(),
        ));
    }
    let blocks = v
        .get("content")
        .and_then(Value::as_array)
        .ok_or_else(|| AiError::BadResponse("sem `content` na resposta".into()))?;
    let text: String = blocks
        .iter()
        .filter(|b| b.get("type").and_then(Value::as_str) == Some("text"))
        .filter_map(|b| b.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("");
    if text.trim().is_empty() {
        return Err(AiError::BadResponse("resposta de texto vazia".into()));
    }
    Ok(text)
}

impl LlmProvider for AnthropicProvider {
    fn name(&self) -> &str {
        "anthropic"
    }
    fn model(&self) -> &str {
        &self.model
    }
    fn complete(&self, system: &str, user: &str) -> Result<String> {
        let body = anthropic_body(&self.model, system, user, DEFAULT_MAX_TOKENS);
        let resp = blocking_client()?
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .map_err(|e| AiError::Http(e.to_string()))?;
        let status = resp.status();
        let v: Value = resp.json().map_err(|e| AiError::Http(e.to_string()))?;
        if !status.is_success() {
            return Err(AiError::Http(format!(
                "HTTP {status}: {}",
                api_error_msg(&v)
            )));
        }
        anthropic_extract(&v)
    }
}

// ---------------------------------------------------------------------------
// OpenAI
// ---------------------------------------------------------------------------

/// Provedor OpenAI (chat completions, HTTP direto).
pub struct OpenAiProvider {
    key: String,
    model: String,
}

fn openai_body(model: &str, system: &str, user: &str, max_tokens: u32) -> Value {
    json!({
        "model": model,
        "max_tokens": max_tokens,
        "messages": [
            { "role": "system", "content": system },
            { "role": "user", "content": user },
        ],
    })
}

fn openai_extract(v: &Value) -> Result<String> {
    let text = v
        .pointer("/choices/0/message/content")
        .and_then(Value::as_str)
        .ok_or_else(|| AiError::BadResponse("sem `choices[0].message.content`".into()))?;
    if text.trim().is_empty() {
        return Err(AiError::BadResponse("resposta de texto vazia".into()));
    }
    Ok(text.to_string())
}

impl LlmProvider for OpenAiProvider {
    fn name(&self) -> &str {
        "openai"
    }
    fn model(&self) -> &str {
        &self.model
    }
    fn complete(&self, system: &str, user: &str) -> Result<String> {
        let body = openai_body(&self.model, system, user, DEFAULT_MAX_TOKENS);
        let resp = blocking_client()?
            .post("https://api.openai.com/v1/chat/completions")
            .header("authorization", format!("Bearer {}", self.key))
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .map_err(|e| AiError::Http(e.to_string()))?;
        let status = resp.status();
        let v: Value = resp.json().map_err(|e| AiError::Http(e.to_string()))?;
        if !status.is_success() {
            return Err(AiError::Http(format!(
                "HTTP {status}: {}",
                api_error_msg(&v)
            )));
        }
        openai_extract(&v)
    }
}

// ---------------------------------------------------------------------------
// Ollama (local)
// ---------------------------------------------------------------------------

/// Provedor Ollama local (sem chave). Host via `BIBLIA_OLLAMA_HOST`.
pub struct OllamaProvider {
    host: String,
    model: String,
}

fn ollama_body(model: &str, system: &str, user: &str) -> Value {
    json!({
        "model": model,
        "stream": false,
        "messages": [
            { "role": "system", "content": system },
            { "role": "user", "content": user },
        ],
    })
}

fn ollama_extract(v: &Value) -> Result<String> {
    let text = v
        .pointer("/message/content")
        .and_then(Value::as_str)
        .ok_or_else(|| AiError::BadResponse("sem `message.content`".into()))?;
    if text.trim().is_empty() {
        return Err(AiError::BadResponse("resposta de texto vazia".into()));
    }
    Ok(text.to_string())
}

impl LlmProvider for OllamaProvider {
    fn name(&self) -> &str {
        "ollama"
    }
    fn model(&self) -> &str {
        &self.model
    }
    fn complete(&self, system: &str, user: &str) -> Result<String> {
        let url = format!("{}/api/chat", self.host.trim_end_matches('/'));
        let body = ollama_body(&self.model, system, user);
        let resp = blocking_client()?
            .post(url)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .map_err(|e| AiError::Http(e.to_string()))?;
        let status = resp.status();
        let v: Value = resp.json().map_err(|e| AiError::Http(e.to_string()))?;
        if !status.is_success() {
            return Err(AiError::Http(format!(
                "HTTP {status}: {}",
                api_error_msg(&v)
            )));
        }
        ollama_extract(&v)
    }
}

/// Extrai uma mensagem de erro legível de respostas de erro variadas.
fn api_error_msg(v: &Value) -> String {
    v.pointer("/error/message")
        .or_else(|| v.get("error"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| v.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn factory_requires_key_for_remote_providers() {
        assert!(build_provider("anthropic", None, None).is_err());
        assert!(build_provider("openai", None, None).is_err());
        // ollama é local: sem chave funciona.
        let p = build_provider("ollama", None, None).unwrap();
        assert_eq!(p.name(), "ollama");
        assert_eq!(p.model(), "llama3");
        // mock sempre disponível.
        assert_eq!(build_provider("mock", None, None).unwrap().name(), "mock");
        assert!(build_provider("skynet", Some("k".into()), None).is_err());
    }

    #[test]
    fn factory_uses_default_and_custom_model() {
        let p = build_provider("anthropic", Some("sk".into()), None).unwrap();
        assert_eq!(p.model(), "claude-opus-4-8");
        let p = build_provider(
            "anthropic",
            Some("sk".into()),
            Some("claude-haiku-4-5".into()),
        )
        .unwrap();
        assert_eq!(p.model(), "claude-haiku-4-5");
    }

    #[test]
    fn anthropic_body_shape() {
        let b = anthropic_body("claude-opus-4-8", "sys", "usr", 4096);
        assert_eq!(b["model"], "claude-opus-4-8");
        assert_eq!(b["system"], "sys");
        assert_eq!(b["max_tokens"], 4096);
        assert_eq!(b["thinking"]["type"], "adaptive");
        assert_eq!(b["messages"][0]["role"], "user");
        assert_eq!(b["messages"][0]["content"], "usr");
    }

    #[test]
    fn anthropic_extract_concats_text_blocks_only() {
        let v = json!({
            "stop_reason": "end_turn",
            "content": [
                { "type": "thinking", "thinking": "" },
                { "type": "text", "text": "Parte 1. " },
                { "type": "text", "text": "Parte 2." }
            ]
        });
        assert_eq!(anthropic_extract(&v).unwrap(), "Parte 1. Parte 2.");
    }

    #[test]
    fn anthropic_extract_handles_refusal() {
        let v = json!({ "stop_reason": "refusal", "content": [] });
        assert!(anthropic_extract(&v).is_err());
    }

    #[test]
    fn openai_and_ollama_extract() {
        let o = json!({ "choices": [ { "message": { "content": "Resposta OpenAI" } } ] });
        assert_eq!(openai_extract(&o).unwrap(), "Resposta OpenAI");
        let l = json!({ "message": { "content": "Resposta Ollama" } });
        assert_eq!(ollama_extract(&l).unwrap(), "Resposta Ollama");
    }

    #[test]
    fn body_builders_for_openai_and_ollama() {
        let o = openai_body("gpt-4o", "s", "u", 100);
        assert_eq!(o["messages"][0]["role"], "system");
        assert_eq!(o["messages"][1]["content"], "u");
        let l = ollama_body("llama3", "s", "u");
        assert_eq!(l["stream"], false);
        assert_eq!(l["model"], "llama3");
    }

    #[test]
    fn cost_estimate_known_and_unknown() {
        // 1M entrada + 1M saída em opus 4.8 = 5 + 25 = 30 USD.
        let c = estimate_cost_usd("claude-opus-4-8", 1_000_000, 1_000_000).unwrap();
        assert!((c - 30.0).abs() < 1e-6);
        assert_eq!(estimate_cost_usd("llama3", 1000, 1000), Some(0.0));
        assert_eq!(estimate_cost_usd("modelo-desconhecido", 1, 1), None);
    }

    #[test]
    fn api_error_msg_extracts_message() {
        let v = json!({ "error": { "message": "chave inválida" } });
        assert_eq!(api_error_msg(&v), "chave inválida");
        let v2 = json!({ "error": "string simples" });
        assert_eq!(api_error_msg(&v2), "string simples");
    }
}
