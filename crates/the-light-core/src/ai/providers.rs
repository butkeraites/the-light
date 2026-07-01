//! Provedores concretos de LLM (BYOK): Anthropic, OpenAI, Ollama e Gemini.
//!
//! O Rust não tem SDK oficial desses provedores, então usamos HTTP direto
//! (reqwest *blocking*). Os corpos de requisição e o parsing das respostas são
//! funções puras (testáveis sem rede); só `complete()` faz I/O. Os testes nunca
//! chamam a rede nem usam chaves reais — ver `MockLlmProvider`.

// Sob `ai-pure` (SEM `embedded`), as helpers puras de corpo/parse (`*_body`,
// `*_extract`, `messages_json`, `parse_api_response`, `api_error_msg`) ainda não
// têm chamador: o transporte no wasm é via `fetch` (F2.7b), que as consumirá.
// Silenciamos o `dead_code` esperado SÓ nesse caminho; sob `embedded` elas são
// exercitadas pelos `impl LlmProvider` + testes, então `-D warnings` segue valendo.
#![cfg_attr(not(feature = "embedded"), allow(dead_code))]

#[cfg(feature = "embedded")]
use std::time::Duration;

use serde_json::{json, Value};

use super::{AiError, ChatMessage, ChatRole, Result};
// O trait e a fábrica só existem no caminho de rede (`embedded`); a superfície
// pura (bodies/extract/custo/modelo default) não os referencia.
#[cfg(feature = "embedded")]
use super::LlmProvider;

#[cfg(feature = "embedded")]
const HTTP_TIMEOUT: Duration = Duration::from_secs(120);
#[cfg(feature = "embedded")]
const DEFAULT_MAX_TOKENS: u32 = 8192;

/// Converte o histórico de conversa no array `[{role, content}]` das APIs.
fn messages_json(messages: &[ChatMessage]) -> Vec<Value> {
    messages
        .iter()
        .map(|m| json!({ "role": m.role.as_str(), "content": m.content }))
        .collect()
}

/// Modelo padrão por provedor (quando não configurado).
pub fn default_model(provider: &str) -> &'static str {
    match provider {
        "anthropic" => "claude-opus-4-8",
        "openai" => "gpt-4o",
        "ollama" => "llama3",
        // `gemini-2.0-flash` foi retirado (3/mar/2026); `2.5-flash` é o atual.
        "gemini" => "gemini-2.5-flash",
        _ => "",
    }
}

/// Constrói o provedor ativo a partir do nome, chave e modelo opcionais.
///
/// `key` é exigida para anthropic/openai; ollama é local (chave opcional).
/// `mock` é sempre disponível (sem rede), útil para testes/demonstração.
///
/// Cria provedores de **rede** (reqwest) — só no caminho `embedded`. No web
/// (`ai-pure`/wasm) o transporte é feito via `fetch` (F2.7b); o custo/modelo e a
/// montagem/parse do corpo vêm das funções puras deste módulo.
#[cfg(feature = "embedded")]
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
            let host = std::env::var("LIGHT_OLLAMA_HOST")
                .unwrap_or_else(|_| "http://localhost:11434".to_string());
            Ok(Box::new(OllamaProvider { host, model }))
        }
        "gemini" => {
            let key = key.ok_or_else(|| AiError::NoKey("gemini".into()))?;
            Ok(Box::new(GeminiProvider { key, model }))
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
        // Gemini 2.0 Flash (Google AI): preço público estável (histórico — o modelo
        // foi retirado 3/mar/2026, mas o arm segue por compatibilidade). Os demais
        // modelos gemini (incl. o novo default `2.5-flash`) variam → não inventamos
        // preço (`None`), coerente com a política anti-alucinação de custo.
        "gemini-2.0-flash" | "gemini-2.0-flash-001" => (0.10, 0.40),
        m if m.starts_with("gemini") => return None,
        m if m.starts_with("llama") => return Some(0.0), // local
        _ => return None,
    };
    let cost = (input_tokens as f64 * pin + output_tokens as f64 * pout) / 1_000_000.0;
    Some(cost)
}

#[cfg(feature = "embedded")]
fn blocking_client() -> Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .timeout(HTTP_TIMEOUT)
        .build()
        .map_err(|e| AiError::Http(e.to_string()))
}

/// Envia a requisição e lê o corpo verificando o **status HTTP antes** de exigir
/// JSON válido — assim erros de API (não-2xx) viram um `AiError::Http` legível
/// (mensagem da API ou corpo bruto), nunca um erro genérico de parsing.
#[cfg(feature = "embedded")]
fn send_json(req: reqwest::blocking::RequestBuilder) -> Result<Value> {
    let resp = req.send().map_err(|e| AiError::Http(e.to_string()))?;
    let status = resp.status();
    let text = resp.text().map_err(|e| AiError::Http(e.to_string()))?;
    parse_api_response(status.is_success(), status.as_str(), &text)
}

/// Núcleo puro de [`send_json`] (testável sem rede).
fn parse_api_response(success: bool, status: &str, body: &str) -> Result<Value> {
    if !success {
        let msg = serde_json::from_str::<Value>(body)
            .map(|v| api_error_msg(&v))
            .unwrap_or_else(|_| body.trim().to_string());
        return Err(AiError::Http(format!("HTTP {status}: {msg}")));
    }
    serde_json::from_str(body).map_err(|e| AiError::BadResponse(e.to_string()))
}

// ---------------------------------------------------------------------------
// Anthropic
// ---------------------------------------------------------------------------

/// Provedor Anthropic (API de Mensagens, HTTP direto).
#[cfg(feature = "embedded")]
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

fn anthropic_chat_body(
    model: &str,
    system: &str,
    messages: &[ChatMessage],
    max_tokens: u32,
) -> Value {
    json!({
        "model": model,
        "max_tokens": max_tokens,
        "system": system,
        "thinking": { "type": "adaptive" },
        "messages": messages_json(messages),
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

#[cfg(feature = "embedded")]
impl LlmProvider for AnthropicProvider {
    fn name(&self) -> &str {
        "anthropic"
    }
    fn model(&self) -> &str {
        &self.model
    }
    fn complete(&self, system: &str, user: &str) -> Result<String> {
        let body = anthropic_body(&self.model, system, user, DEFAULT_MAX_TOKENS);
        self.post(body)
    }
    fn chat(&self, system: &str, messages: &[ChatMessage]) -> Result<String> {
        let body = anthropic_chat_body(&self.model, system, messages, DEFAULT_MAX_TOKENS);
        self.post(body)
    }
}

#[cfg(feature = "embedded")]
impl AnthropicProvider {
    fn post(&self, body: Value) -> Result<String> {
        let req = blocking_client()?
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body);
        anthropic_extract(&send_json(req)?)
    }
}

// ---------------------------------------------------------------------------
// OpenAI
// ---------------------------------------------------------------------------

/// Provedor OpenAI (chat completions, HTTP direto).
#[cfg(feature = "embedded")]
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

fn openai_chat_body(model: &str, system: &str, messages: &[ChatMessage], max_tokens: u32) -> Value {
    let mut msgs = vec![json!({ "role": "system", "content": system })];
    msgs.extend(messages_json(messages));
    json!({ "model": model, "max_tokens": max_tokens, "messages": msgs })
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

#[cfg(feature = "embedded")]
impl LlmProvider for OpenAiProvider {
    fn name(&self) -> &str {
        "openai"
    }
    fn model(&self) -> &str {
        &self.model
    }
    fn complete(&self, system: &str, user: &str) -> Result<String> {
        self.post(openai_body(&self.model, system, user, DEFAULT_MAX_TOKENS))
    }
    fn chat(&self, system: &str, messages: &[ChatMessage]) -> Result<String> {
        self.post(openai_chat_body(
            &self.model,
            system,
            messages,
            DEFAULT_MAX_TOKENS,
        ))
    }
}

#[cfg(feature = "embedded")]
impl OpenAiProvider {
    fn post(&self, body: Value) -> Result<String> {
        let req = blocking_client()?
            .post("https://api.openai.com/v1/chat/completions")
            .header("authorization", format!("Bearer {}", self.key))
            .header("content-type", "application/json")
            .json(&body);
        openai_extract(&send_json(req)?)
    }
}

// ---------------------------------------------------------------------------
// Ollama (local)
// ---------------------------------------------------------------------------

/// Provedor Ollama local (sem chave). Host via `LIGHT_OLLAMA_HOST`.
#[cfg(feature = "embedded")]
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

fn ollama_chat_body(model: &str, system: &str, messages: &[ChatMessage]) -> Value {
    let mut msgs = vec![json!({ "role": "system", "content": system })];
    msgs.extend(messages_json(messages));
    json!({ "model": model, "stream": false, "messages": msgs })
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

#[cfg(feature = "embedded")]
impl LlmProvider for OllamaProvider {
    fn name(&self) -> &str {
        "ollama"
    }
    fn model(&self) -> &str {
        &self.model
    }
    fn complete(&self, system: &str, user: &str) -> Result<String> {
        self.post(ollama_body(&self.model, system, user))
    }
    fn chat(&self, system: &str, messages: &[ChatMessage]) -> Result<String> {
        self.post(ollama_chat_body(&self.model, system, messages))
    }
}

#[cfg(feature = "embedded")]
impl OllamaProvider {
    fn post(&self, body: Value) -> Result<String> {
        let url = format!("{}/api/chat", self.host.trim_end_matches('/'));
        let req = blocking_client()?
            .post(url)
            .header("content-type", "application/json")
            .json(&body);
        ollama_extract(&send_json(req)?)
    }
}

// ---------------------------------------------------------------------------
// Gemini (Google AI)
// ---------------------------------------------------------------------------

/// Provedor Google Gemini (API `generateContent`, HTTP direto).
///
/// O modelo vai na **URL** (`.../models/{model}:generateContent`), não no corpo;
/// a chave vai no header `x-goog-api-key` (nunca na URL nem em log).
#[cfg(feature = "embedded")]
pub struct GeminiProvider {
    key: String,
    model: String,
}

/// Corpo `generateContent` single-turn. `system` vira `system_instruction`; o
/// `model` não entra no corpo (vai na URL) — o parâmetro é mantido por simetria
/// com os demais `*_body` e para deixar a assinatura auto-documentada.
fn gemini_body(model: &str, system: &str, user: &str, max_tokens: u32) -> Value {
    let _ = model;
    json!({
        "contents": [
            { "role": "user", "parts": [ { "text": user } ] }
        ],
        "system_instruction": { "parts": [ { "text": system } ] },
        "generationConfig": { "maxOutputTokens": max_tokens },
    })
}

/// Corpo `generateContent` multi-turno. O papel do assistente na Gemini é
/// `"model"` (não `"assistant"`).
fn gemini_chat_body(model: &str, system: &str, messages: &[ChatMessage], max_tokens: u32) -> Value {
    let _ = model;
    let contents: Vec<Value> = messages
        .iter()
        .map(|m| {
            let role = match m.role {
                ChatRole::User => "user",
                ChatRole::Assistant => "model",
            };
            json!({ "role": role, "parts": [ { "text": m.content } ] })
        })
        .collect();
    json!({
        "contents": contents,
        "system_instruction": { "parts": [ { "text": system } ] },
        "generationConfig": { "maxOutputTokens": max_tokens },
    })
}

/// Extrai o texto de `candidates[0].content.parts[*].text` (concatena as partes).
///
/// Sem `candidates` a resposta pode ter sido **bloqueada** (segurança): tenta ler
/// `promptFeedback.blockReason` para uma mensagem clara, senão erro genérico.
fn gemini_extract(v: &Value) -> Result<String> {
    let candidates = v
        .get("candidates")
        .and_then(Value::as_array)
        .filter(|c| !c.is_empty())
        .ok_or_else(|| {
            match v
                .pointer("/promptFeedback/blockReason")
                .and_then(Value::as_str)
            {
                Some(reason) => {
                    AiError::BadResponse(format!("resposta bloqueada pelo provedor: {reason}"))
                }
                None => AiError::BadResponse("sem `candidates` na resposta".into()),
            }
        })?;
    let parts = candidates[0]
        .pointer("/content/parts")
        .and_then(Value::as_array)
        .ok_or_else(|| AiError::BadResponse("sem `content.parts` no candidate".into()))?;
    let text: String = parts
        .iter()
        .filter_map(|p| p.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("");
    if text.trim().is_empty() {
        return Err(AiError::BadResponse("resposta de texto vazia".into()));
    }
    Ok(text)
}

#[cfg(feature = "embedded")]
impl LlmProvider for GeminiProvider {
    fn name(&self) -> &str {
        "gemini"
    }
    fn model(&self) -> &str {
        &self.model
    }
    fn complete(&self, system: &str, user: &str) -> Result<String> {
        self.post(gemini_body(&self.model, system, user, DEFAULT_MAX_TOKENS))
    }
    fn chat(&self, system: &str, messages: &[ChatMessage]) -> Result<String> {
        self.post(gemini_chat_body(
            &self.model,
            system,
            messages,
            DEFAULT_MAX_TOKENS,
        ))
    }
}

#[cfg(feature = "embedded")]
impl GeminiProvider {
    fn post(&self, body: Value) -> Result<String> {
        // O modelo vai na URL; a chave vai no header (nunca na URL/log).
        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent",
            self.model
        );
        let req = blocking_client()?
            .post(url)
            .header("x-goog-api-key", &self.key)
            .header("content-type", "application/json")
            .json(&body);
        gemini_extract(&send_json(req)?)
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
        // Gemini 2.0 Flash (histórico) tem preço conhecido; os demais gemini
        // (incl. o novo default `2.5-flash`) → None (preço não inventado).
        let g = estimate_cost_usd("gemini-2.0-flash", 1_000_000, 1_000_000).unwrap();
        assert!((g - 0.50).abs() < 1e-6, "{g}");
        assert_eq!(estimate_cost_usd("gemini-2.5-flash", 1000, 1000), None);
        assert_eq!(estimate_cost_usd("gemini-1.5-pro", 1000, 1000), None);
    }

    #[test]
    fn gemini_registered_in_factory_and_default_model() {
        // Gemini exige chave (provedor de nuvem), como anthropic/openai.
        assert!(build_provider("gemini", None, None).is_err());
        let p = build_provider("gemini", Some("PLACEHOLDER".into()), None).unwrap();
        assert_eq!(p.name(), "gemini");
        // `gemini-2.0-flash` foi retirado (3/mar/2026); o default agora é `2.5-flash`.
        assert_eq!(p.model(), "gemini-2.5-flash");
        assert_eq!(default_model("gemini"), "gemini-2.5-flash");
        // Modelo customizado é respeitado.
        let p = build_provider(
            "gemini",
            Some("PLACEHOLDER".into()),
            Some("gemini-1.5-pro".into()),
        )
        .unwrap();
        assert_eq!(p.model(), "gemini-1.5-pro");
    }

    #[test]
    fn gemini_body_shape() {
        let b = gemini_body("gemini-2.0-flash", "sys", "usr", 4096);
        // O modelo NÃO entra no corpo (vai na URL).
        assert!(b.get("model").is_none());
        assert_eq!(b["contents"][0]["role"], "user");
        assert_eq!(b["contents"][0]["parts"][0]["text"], "usr");
        assert_eq!(b["system_instruction"]["parts"][0]["text"], "sys");
        assert_eq!(b["generationConfig"]["maxOutputTokens"], 4096);
    }

    #[test]
    fn gemini_chat_body_maps_assistant_to_model_role() {
        let msgs = vec![
            ChatMessage {
                role: ChatRole::User,
                content: "oi".into(),
            },
            ChatMessage {
                role: ChatRole::Assistant,
                content: "olá".into(),
            },
        ];
        let b = gemini_chat_body("gemini-2.0-flash", "sys", &msgs, 100);
        assert_eq!(b["contents"][0]["role"], "user");
        assert_eq!(b["contents"][1]["role"], "model");
        assert_eq!(b["contents"][1]["parts"][0]["text"], "olá");
        assert_eq!(b["system_instruction"]["parts"][0]["text"], "sys");
    }

    #[test]
    fn gemini_extract_concats_parts() {
        let v = json!({
            "candidates": [
                { "content": { "role": "model", "parts": [
                    { "text": "Parte 1. " },
                    { "text": "Parte 2." }
                ] } }
            ]
        });
        assert_eq!(gemini_extract(&v).unwrap(), "Parte 1. Parte 2.");
    }

    #[test]
    fn gemini_extract_errors_without_candidates() {
        // Sem `candidates` → erro claro.
        let v = json!({ "usageMetadata": {} });
        assert!(gemini_extract(&v).is_err());
        // `candidates` vazio também → erro.
        let empty = json!({ "candidates": [] });
        assert!(gemini_extract(&empty).is_err());
        // Bloqueio de segurança traz a razão na mensagem.
        let blocked = json!({ "promptFeedback": { "blockReason": "SAFETY" } });
        let msg = gemini_extract(&blocked).unwrap_err().to_string();
        assert!(msg.contains("SAFETY"), "{msg}");
    }

    #[test]
    fn parse_api_response_checks_status_before_json() {
        // 2xx com JSON válido → Ok.
        let ok = parse_api_response(true, "200", r#"{"a":1}"#).unwrap();
        assert_eq!(ok["a"], 1);
        // não-2xx com JSON de erro → mensagem da API (não erro de parsing).
        let e = parse_api_response(false, "401", r#"{"error":{"message":"invalid x-api-key"}}"#)
            .unwrap_err()
            .to_string();
        assert!(e.contains("401") && e.contains("invalid x-api-key"), "{e}");
        // não-2xx com corpo NÃO-JSON (ex.: HTML/texto) → usa o corpo bruto, não trava.
        let e = parse_api_response(false, "502", "Bad Gateway")
            .unwrap_err()
            .to_string();
        assert!(e.contains("502") && e.contains("Bad Gateway"), "{e}");
        // 2xx com corpo inválido → BadResponse (não Http).
        assert!(matches!(
            parse_api_response(true, "200", "not json"),
            Err(AiError::BadResponse(_))
        ));
    }

    #[test]
    fn api_error_msg_extracts_message() {
        let v = json!({ "error": { "message": "chave inválida" } });
        assert_eq!(api_error_msg(&v), "chave inválida");
        let v2 = json!({ "error": "string simples" });
        assert_eq!(api_error_msg(&v2), "string simples");
    }
}
