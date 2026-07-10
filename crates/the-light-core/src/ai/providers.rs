//! Provedores concretos de LLM (BYOK): Anthropic, OpenAI, Ollama e Gemini.
//!
//! O Rust não tem SDK oficial desses provedores, então usamos HTTP direto
//! (reqwest *blocking*). Os corpos de requisição e o parsing das respostas são
//! funções puras (testáveis sem rede); só `complete()` faz I/O. Os testes nunca
//! chamam a rede nem usam chaves reais — ver `MockLlmProvider`.

// A superfície pura de transporte (`*_body`/`*_chat_body`/`*_extract`/
// `*_stream_delta`/`messages_json`/`sse_data`/`with_stream`/`parse_api_response`/
// `api_error_msg` + URLs/headers) é `pub` e DATA-ONLY (ADR-0062): sob `embedded` os
// `impl LlmProvider` a consomem; sob `ai-pure` (wasm) o consumidor é a fronteira
// `the-light-app` via `fetch`. Só `stream_reader` (privado, usado apenas por
// `stream_response`/`embedded`) fica dead-code sob `ai-pure` — daí o `allow`
// restrito a esse caminho.
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
/// Teto de tokens de saída pedido a cada provedor. Superfície DATA-ONLY (ADR-0062):
/// o alvo web IMPORTA este valor em vez de hard-codar `8192`. Un-gated → disponível
/// sob `ai-pure` (wasm); é um `u32` puro, não arrasta reqwest.
pub const DEFAULT_MAX_TOKENS: u32 = 8192;

// --- Superfície DATA-ONLY de transporte (ADR-0062) -------------------------
// URLs e headers como DADO puro, un-gated: o alvo web monta `{url, headers, body}` e
// chama `*_extract`/`*_stream_delta`, tornando o `fetch` no TS um transporte burro. O
// caminho nativo (`embedded`) consome ESTAS MESMAS helpers → uma única fonte da verdade
// (nativo e web só podem CONCORDAR).

/// Endpoint Anthropic (Messages API).
pub const ANTHROPIC_URL: &str = "https://api.anthropic.com/v1/messages";
/// Endpoint OpenAI (chat completions).
pub const OPENAI_URL: &str = "https://api.openai.com/v1/chat/completions";
/// Host Ollama padrão (local) quando `LIGHT_OLLAMA_HOST` não está setado.
pub const OLLAMA_DEFAULT_HOST: &str = "http://localhost:11434";

/// URL do endpoint Ollama `/api/chat` a partir do `host` (remove `/` final).
pub fn ollama_url(host: &str) -> String {
    format!("{}/api/chat", host.trim_end_matches('/'))
}

/// URL Gemini: `:generateContent` (não-stream) ou `:streamGenerateContent?alt=sse`
/// (stream). O modelo vai na URL — não no corpo.
pub fn gemini_url(model: &str, stream: bool) -> String {
    let method = if stream {
        "streamGenerateContent?alt=sse"
    } else {
        "generateContent"
    };
    format!("https://generativelanguage.googleapis.com/v1beta/models/{model}:{method}")
}

/// Headers do request Anthropic. `browser=true` acrescenta o header de acesso-direto-
/// do-browser (CORS; só o alvo web precisa — ADR-0058); o nativo passa `false`,
/// mantendo o request byte-a-byte.
pub fn anthropic_headers(key: &str, browser: bool) -> Vec<(String, String)> {
    let mut h = vec![
        ("x-api-key".to_string(), key.to_string()),
        ("anthropic-version".to_string(), "2023-06-01".to_string()),
        ("content-type".to_string(), "application/json".to_string()),
    ];
    if browser {
        h.push((
            "anthropic-dangerous-direct-browser-access".to_string(),
            "true".to_string(),
        ));
    }
    h
}

/// Headers do request OpenAI (Bearer + content-type).
pub fn openai_headers(key: &str) -> Vec<(String, String)> {
    vec![
        ("authorization".to_string(), format!("Bearer {key}")),
        ("content-type".to_string(), "application/json".to_string()),
    ]
}

/// Headers do request Ollama (local, sem chave).
pub fn ollama_headers() -> Vec<(String, String)> {
    vec![("content-type".to_string(), "application/json".to_string())]
}

/// Headers do request Gemini (chave em `x-goog-api-key`, nunca na URL/log).
pub fn gemini_headers(key: &str) -> Vec<(String, String)> {
    vec![
        ("x-goog-api-key".to_string(), key.to_string()),
        ("content-type".to_string(), "application/json".to_string()),
    ]
}

/// Converte o histórico de conversa no array `[{role, content}]` das APIs.
pub fn messages_json(messages: &[ChatMessage]) -> Vec<Value> {
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
                .unwrap_or_else(|_| OLLAMA_DEFAULT_HOST.to_string());
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

/// Aplica os headers DATA-ONLY (`[(nome, valor)]`) ao request reqwest. Único ponto
/// nativo que consome `*_headers`, garantindo que web e nativo montem o MESMO header.
#[cfg(feature = "embedded")]
fn apply_headers(
    req: reqwest::blocking::RequestBuilder,
    headers: &[(String, String)],
) -> reqwest::blocking::RequestBuilder {
    headers
        .iter()
        .fold(req, |req, (k, v)| req.header(k.as_str(), v.as_str()))
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
pub fn parse_api_response(success: bool, status: &str, body: &str) -> Result<Value> {
    if !success {
        let msg = serde_json::from_str::<Value>(body)
            .map(|v| api_error_msg(&v))
            .unwrap_or_else(|_| body.trim().to_string());
        return Err(AiError::Http(format!("HTTP {status}: {msg}")));
    }
    serde_json::from_str(body).map_err(|e| AiError::BadResponse(e.to_string()))
}

// ---------------------------------------------------------------------------
// Streaming (SSE / NDJSON) — leitura por linha e parsers de delta por provedor.
//
// O `stream_reader` é PURO (std::io::BufRead) → testável com um `Cursor` de
// fixture, sem rede. Só `stream_response` faz o I/O `reqwest` (embedded). Os
// parsers de delta são PUROS (serde_json) e un-gated (como os `*_body`/`*_extract`);
// sob `ai-pure` ficam dead-code (coberto pelo `allow` do topo do arquivo).
// ---------------------------------------------------------------------------

/// Payload de uma linha SSE `data: …` (com espaço opcional após o `:`). `None`
/// para linhas que NÃO são `data:` (`event:`/`id:`/comentários `:`/linhas vazias).
pub fn sse_data(line: &str) -> Option<&str> {
    line.strip_prefix("data:").map(str::trim)
}

/// Acrescenta `"stream": true` a um corpo JSON de objeto, REUSANDO o `*_body` do
/// não-streaming (zero-drift do payload; só o flag muda). Substitui o valor se a
/// chave já existir (caso do `ollama_body`, que tem `"stream": false`).
pub fn with_stream(mut body: Value) -> Value {
    if let Some(obj) = body.as_object_mut() {
        obj.insert("stream".to_string(), Value::Bool(true));
    }
    body
}

/// Lê `reader` LINHA a LINHA (SSE `text/event-stream` OU NDJSON), aplicando
/// `parse_line` a cada linha; para cada delta não-vazio chama `on_token` e ACUMULA
/// na String de retorno (== a resposta de `complete`). `parse_line` devolve
/// `Ok(None)` para linhas a ignorar (`event:`/vazias/`[DONE]`/parciais/pings) e
/// `Err(..)` para recusa/bloqueio do modelo. Puro: testável com um `Cursor`.
fn stream_reader<R: std::io::BufRead>(
    reader: R,
    on_token: &mut dyn FnMut(&str),
    mut parse_line: impl FnMut(&str) -> Result<Option<String>>,
) -> Result<String> {
    let mut full = String::new();
    for line in reader.lines() {
        let line = line.map_err(|e| AiError::Http(e.to_string()))?;
        if let Some(delta) = parse_line(&line)? {
            if !delta.is_empty() {
                full.push_str(&delta);
                on_token(&delta);
            }
        }
    }
    if full.trim().is_empty() {
        return Err(AiError::BadResponse("resposta de texto vazia".into()));
    }
    Ok(full)
}

/// Abre a resposta de streaming: verifica o **status HTTP** (erro → `AiError::Http`
/// com a mensagem da API, como `send_json`) e delega a leitura por linha ao
/// `stream_reader`. A chave NUNCA é logada aqui (só viajou nos headers do request).
#[cfg(feature = "embedded")]
fn stream_response(
    resp: reqwest::blocking::Response,
    on_token: &mut dyn FnMut(&str),
    parse_line: impl FnMut(&str) -> Result<Option<String>>,
) -> Result<String> {
    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().unwrap_or_default();
        let msg = serde_json::from_str::<Value>(&text)
            .map(|v| api_error_msg(&v))
            .unwrap_or_else(|_| text.trim().to_string());
        return Err(AiError::Http(format!("HTTP {}: {msg}", status.as_str())));
    }
    stream_reader(std::io::BufReader::new(resp), on_token, parse_line)
}

// ---- Parsers de delta por provedor (puros; espelham os `*_extract`) ----

/// Anthropic (SSE): emite o texto de `content_block_delta` com
/// `delta.type == "text_delta"` (IGNORA `thinking_delta`, como `anthropic_extract`
/// ignora blocos não-`text`). `message_delta` com `stop_reason == "refusal"` → erro
/// (mesma política do não-streaming). Um evento `error` in-band (após o 200, ex.:
/// `overloaded_error`) vira `AiError::Http` — como o não-streaming faz com erros do
/// provedor — em vez de ser silenciosamente engolido (truncaria a resposta). Linha
/// não-JSON (ping) → `Ok(None)`.
pub fn anthropic_stream_delta(data: &str) -> Result<Option<String>> {
    let v: Value = match serde_json::from_str(data) {
        Ok(v) => v,
        Err(_) => return Ok(None),
    };
    match v.get("type").and_then(Value::as_str) {
        Some("content_block_delta") => {
            if v.pointer("/delta/type").and_then(Value::as_str) == Some("text_delta") {
                Ok(v.pointer("/delta/text")
                    .and_then(Value::as_str)
                    .map(str::to_string))
            } else {
                Ok(None)
            }
        }
        Some("message_delta") => {
            if v.pointer("/delta/stop_reason").and_then(Value::as_str) == Some("refusal") {
                Err(AiError::BadResponse(
                    "o modelo recusou a solicitação (refusal)".into(),
                ))
            } else {
                Ok(None)
            }
        }
        Some("error") => Err(AiError::Http(api_error_msg(&v))),
        _ => Ok(None),
    }
}

/// OpenAI (SSE): `choices[0].delta.content`; sentinela `[DONE]` e linhas
/// não-JSON → `Ok(None)`. Um frame `{"error":{…}}` in-band (após o 200) vira
/// `AiError::Http` (não é engolido).
pub fn openai_stream_delta(data: &str) -> Result<Option<String>> {
    if data == "[DONE]" {
        return Ok(None);
    }
    let v: Value = match serde_json::from_str(data) {
        Ok(v) => v,
        Err(_) => return Ok(None),
    };
    if v.get("error").is_some() {
        return Err(AiError::Http(api_error_msg(&v)));
    }
    Ok(v.pointer("/choices/0/delta/content")
        .and_then(Value::as_str)
        .map(str::to_string))
}

/// Gemini (SSE, `:streamGenerateContent?alt=sse`): concat de
/// `candidates[0].content.parts[*].text`. `promptFeedback.blockReason` → erro
/// (mesma política de `gemini_extract`). Um objeto `error` in-band (após o 200) vira
/// `AiError::Http` (não é engolido).
pub fn gemini_stream_delta(data: &str) -> Result<Option<String>> {
    let v: Value = match serde_json::from_str(data) {
        Ok(v) => v,
        Err(_) => return Ok(None),
    };
    if v.get("error").is_some() {
        return Err(AiError::Http(api_error_msg(&v)));
    }
    if let Some(reason) = v
        .pointer("/promptFeedback/blockReason")
        .and_then(Value::as_str)
    {
        return Err(AiError::BadResponse(format!(
            "resposta bloqueada pelo provedor: {reason}"
        )));
    }
    let parts = match v
        .pointer("/candidates/0/content/parts")
        .and_then(Value::as_array)
    {
        Some(p) => p,
        None => return Ok(None),
    };
    let text: String = parts
        .iter()
        .filter_map(|p| p.get("text").and_then(Value::as_str))
        .collect();
    Ok(if text.is_empty() { None } else { Some(text) })
}

/// Ollama (NDJSON, SEM prefixo `data:`): a linha INTEIRA é um objeto JSON;
/// emite `message.content`. Linha em branco/parcial → `Ok(None)`. Uma linha
/// `{"error":"…"}` in-band vira `AiError::Http` (não é engolida).
pub fn ollama_stream_delta(line: &str) -> Result<Option<String>> {
    let v: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(_) => return Ok(None),
    };
    if v.get("error").is_some() {
        return Err(AiError::Http(api_error_msg(&v)));
    }
    Ok(v.pointer("/message/content")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string))
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

pub fn anthropic_body(model: &str, system: &str, user: &str, max_tokens: u32) -> Value {
    json!({
        "model": model,
        "max_tokens": max_tokens,
        "system": system,
        // Pensamento adaptativo: melhora o raciocínio exegético (Opus 4.8).
        "thinking": { "type": "adaptive" },
        "messages": [ { "role": "user", "content": user } ],
    })
}

pub fn anthropic_chat_body(
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
pub fn anthropic_extract(v: &Value) -> Result<String> {
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
    fn complete_stream(
        &self,
        system: &str,
        user: &str,
        on_token: &mut dyn FnMut(&str),
    ) -> Result<String> {
        let body = with_stream(anthropic_body(
            &self.model,
            system,
            user,
            DEFAULT_MAX_TOKENS,
        ));
        self.post_stream(body, on_token)
    }
}

#[cfg(feature = "embedded")]
impl AnthropicProvider {
    fn post(&self, body: Value) -> Result<String> {
        let req = apply_headers(
            blocking_client()?.post(ANTHROPIC_URL),
            &anthropic_headers(&self.key, false),
        )
        .json(&body);
        anthropic_extract(&send_json(req)?)
    }
    fn post_stream(&self, body: Value, on_token: &mut dyn FnMut(&str)) -> Result<String> {
        let req = apply_headers(
            blocking_client()?.post(ANTHROPIC_URL),
            &anthropic_headers(&self.key, false),
        )
        .json(&body);
        let resp = req.send().map_err(|e| AiError::Http(e.to_string()))?;
        stream_response(resp, on_token, |line| match sse_data(line) {
            Some(data) => anthropic_stream_delta(data),
            None => Ok(None),
        })
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

pub fn openai_body(model: &str, system: &str, user: &str, max_tokens: u32) -> Value {
    json!({
        "model": model,
        "max_tokens": max_tokens,
        "messages": [
            { "role": "system", "content": system },
            { "role": "user", "content": user },
        ],
    })
}

pub fn openai_chat_body(
    model: &str,
    system: &str,
    messages: &[ChatMessage],
    max_tokens: u32,
) -> Value {
    let mut msgs = vec![json!({ "role": "system", "content": system })];
    msgs.extend(messages_json(messages));
    json!({ "model": model, "max_tokens": max_tokens, "messages": msgs })
}

pub fn openai_extract(v: &Value) -> Result<String> {
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
    fn complete_stream(
        &self,
        system: &str,
        user: &str,
        on_token: &mut dyn FnMut(&str),
    ) -> Result<String> {
        let body = with_stream(openai_body(&self.model, system, user, DEFAULT_MAX_TOKENS));
        self.post_stream(body, on_token)
    }
}

#[cfg(feature = "embedded")]
impl OpenAiProvider {
    fn post(&self, body: Value) -> Result<String> {
        let req = apply_headers(
            blocking_client()?.post(OPENAI_URL),
            &openai_headers(&self.key),
        )
        .json(&body);
        openai_extract(&send_json(req)?)
    }
    fn post_stream(&self, body: Value, on_token: &mut dyn FnMut(&str)) -> Result<String> {
        let req = apply_headers(
            blocking_client()?.post(OPENAI_URL),
            &openai_headers(&self.key),
        )
        .json(&body);
        let resp = req.send().map_err(|e| AiError::Http(e.to_string()))?;
        stream_response(resp, on_token, |line| match sse_data(line) {
            Some(data) => openai_stream_delta(data),
            None => Ok(None),
        })
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

pub fn ollama_body(model: &str, system: &str, user: &str) -> Value {
    json!({
        "model": model,
        "stream": false,
        "messages": [
            { "role": "system", "content": system },
            { "role": "user", "content": user },
        ],
    })
}

pub fn ollama_chat_body(model: &str, system: &str, messages: &[ChatMessage]) -> Value {
    let mut msgs = vec![json!({ "role": "system", "content": system })];
    msgs.extend(messages_json(messages));
    json!({ "model": model, "stream": false, "messages": msgs })
}

pub fn ollama_extract(v: &Value) -> Result<String> {
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
    fn complete_stream(
        &self,
        system: &str,
        user: &str,
        on_token: &mut dyn FnMut(&str),
    ) -> Result<String> {
        // `ollama_body` traz `"stream": false`; `with_stream` sobrescreve p/ true.
        let body = with_stream(ollama_body(&self.model, system, user));
        self.post_stream(body, on_token)
    }
}

#[cfg(feature = "embedded")]
impl OllamaProvider {
    fn post(&self, body: Value) -> Result<String> {
        let req = apply_headers(
            blocking_client()?.post(ollama_url(&self.host)),
            &ollama_headers(),
        )
        .json(&body);
        ollama_extract(&send_json(req)?)
    }
    fn post_stream(&self, body: Value, on_token: &mut dyn FnMut(&str)) -> Result<String> {
        let req = apply_headers(
            blocking_client()?.post(ollama_url(&self.host)),
            &ollama_headers(),
        )
        .json(&body);
        let resp = req.send().map_err(|e| AiError::Http(e.to_string()))?;
        // NDJSON: a linha inteira É o JSON (sem `data:`).
        stream_response(resp, on_token, ollama_stream_delta)
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
pub fn gemini_body(model: &str, system: &str, user: &str, max_tokens: u32) -> Value {
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
pub fn gemini_chat_body(
    model: &str,
    system: &str,
    messages: &[ChatMessage],
    max_tokens: u32,
) -> Value {
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
pub fn gemini_extract(v: &Value) -> Result<String> {
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
    fn complete_stream(
        &self,
        system: &str,
        user: &str,
        on_token: &mut dyn FnMut(&str),
    ) -> Result<String> {
        // Corpo IDÊNTICO ao não-streaming; o modelo vai na URL, o streaming no endpoint.
        self.post_stream(
            gemini_body(&self.model, system, user, DEFAULT_MAX_TOKENS),
            on_token,
        )
    }
}

#[cfg(feature = "embedded")]
impl GeminiProvider {
    fn post(&self, body: Value) -> Result<String> {
        // O modelo vai na URL; a chave vai no header (nunca na URL/log).
        let req = apply_headers(
            blocking_client()?.post(gemini_url(&self.model, false)),
            &gemini_headers(&self.key),
        )
        .json(&body);
        gemini_extract(&send_json(req)?)
    }
    fn post_stream(&self, body: Value, on_token: &mut dyn FnMut(&str)) -> Result<String> {
        // Streaming SSE selecionado pelo endpoint `:streamGenerateContent?alt=sse`.
        let req = apply_headers(
            blocking_client()?.post(gemini_url(&self.model, true)),
            &gemini_headers(&self.key),
        )
        .json(&body);
        let resp = req.send().map_err(|e| AiError::Http(e.to_string()))?;
        stream_response(resp, on_token, |line| match sse_data(line) {
            Some(data) => gemini_stream_delta(data),
            None => Ok(None),
        })
    }
}

/// Extrai uma mensagem de erro legível de respostas de erro variadas.
pub fn api_error_msg(v: &Value) -> String {
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

    #[test]
    fn transport_data_surface_urls_and_headers() {
        // Contrato DATA-ONLY (ADR-0062): o alvo web IMPORTA estes valores. Este teste
        // é a fonte da verdade que o espelho TS deve reproduzir byte-a-byte.
        assert_eq!(DEFAULT_MAX_TOKENS, 8192);
        assert_eq!(ANTHROPIC_URL, "https://api.anthropic.com/v1/messages");
        assert_eq!(OPENAI_URL, "https://api.openai.com/v1/chat/completions");
        assert_eq!(OLLAMA_DEFAULT_HOST, "http://localhost:11434");
        assert_eq!(ollama_url("http://h:1/"), "http://h:1/api/chat");
        assert_eq!(
            gemini_url("gemini-2.5-flash", false),
            "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash:generateContent"
        );
        assert_eq!(
            gemini_url("gemini-2.5-flash", true),
            "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash:streamGenerateContent?alt=sse"
        );

        // Headers: o nativo (browser=false) NÃO manda o header de CORS; o web (true) sim.
        assert_eq!(
            anthropic_headers("K", false),
            vec![
                ("x-api-key".to_string(), "K".to_string()),
                ("anthropic-version".to_string(), "2023-06-01".to_string()),
                ("content-type".to_string(), "application/json".to_string()),
            ]
        );
        assert!(anthropic_headers("K", true)
            .iter()
            .any(|(k, v)| k == "anthropic-dangerous-direct-browser-access" && v == "true"));
        assert_eq!(
            openai_headers("K"),
            vec![
                ("authorization".to_string(), "Bearer K".to_string()),
                ("content-type".to_string(), "application/json".to_string()),
            ]
        );
        assert_eq!(
            ollama_headers(),
            vec![("content-type".to_string(), "application/json".to_string())]
        );
        assert_eq!(
            gemini_headers("K"),
            vec![
                ("x-goog-api-key".to_string(), "K".to_string()),
                ("content-type".to_string(), "application/json".to_string()),
            ]
        );
    }
}

#[cfg(test)]
mod stream_tests {
    use super::*;
    use std::io::Cursor;

    /// Roda o `stream_reader` sobre um corpo de fixture, devolvendo (deltas, full).
    fn run(body: &str, parse: impl FnMut(&str) -> Result<Option<String>>) -> (Vec<String>, String) {
        let mut deltas: Vec<String> = Vec::new();
        let full = {
            let mut on = |t: &str| deltas.push(t.to_string());
            stream_reader(Cursor::new(body.as_bytes()), &mut on, parse).unwrap()
        };
        (deltas, full)
    }

    #[test]
    fn anthropic_sse_text_deltas_concat_to_full() {
        // Inclui um `thinking_delta` (deve ser IGNORADO) e linhas `event:`/vazias.
        let body = concat!(
            "event: message_start\n",
            "data: {\"type\":\"message_start\"}\n",
            "\n",
            "event: content_block_delta\n",
            "data: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"Deus \"}}\n",
            "\n",
            "data: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"thinking_delta\",\"thinking\":\"...\"}}\n",
            "data: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"amou \"}}\n",
            "data: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"o mundo.\"}}\n",
            "event: message_stop\n",
            "data: {\"type\":\"message_stop\"}\n",
        );
        let (deltas, full) = run(body, |l| match sse_data(l) {
            Some(d) => anthropic_stream_delta(d),
            None => Ok(None),
        });
        assert_eq!(deltas, ["Deus ", "amou ", "o mundo."]);
        assert_eq!(full, "Deus amou o mundo.");
    }

    #[test]
    fn openai_sse_deltas_concat_and_done_ignored() {
        let body = concat!(
            "data: {\"choices\":[{\"delta\":{\"content\":\"Deus \"}}]}\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\"amou \"}}]}\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\"o mundo.\"}}]}\n",
            "data: [DONE]\n",
        );
        let (deltas, full) = run(body, |l| match sse_data(l) {
            Some(d) => openai_stream_delta(d),
            None => Ok(None),
        });
        assert_eq!(deltas, ["Deus ", "amou ", "o mundo."]);
        assert_eq!(full, "Deus amou o mundo.");
    }

    #[test]
    fn gemini_sse_parts_concat_to_full() {
        let body = concat!(
            "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"Deus \"}]}}]}\n",
            "\n",
            "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"amou \"}]}}]}\n",
            "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"o mundo.\"}]}}]}\n",
        );
        let (deltas, full) = run(body, |l| match sse_data(l) {
            Some(d) => gemini_stream_delta(d),
            None => Ok(None),
        });
        assert_eq!(deltas, ["Deus ", "amou ", "o mundo."]);
        assert_eq!(full, "Deus amou o mundo.");
    }

    #[test]
    fn ollama_ndjson_deltas_concat_to_full() {
        // NDJSON: 1 objeto por linha, SEM `data:`; a última traz done:true e content vazio.
        let body = concat!(
            "{\"message\":{\"role\":\"assistant\",\"content\":\"Deus \"},\"done\":false}\n",
            "{\"message\":{\"content\":\"amou \"},\"done\":false}\n",
            "{\"message\":{\"content\":\"o mundo.\"},\"done\":false}\n",
            "{\"message\":{\"content\":\"\"},\"done\":true}\n",
        );
        let (deltas, full) = run(body, ollama_stream_delta);
        assert_eq!(deltas, ["Deus ", "amou ", "o mundo."]);
        assert_eq!(full, "Deus amou o mundo.");
    }

    #[test]
    fn anthropic_refusal_in_stream_is_error() {
        let body = "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"refusal\"}}\n";
        let mut on = |_: &str| {};
        let err = stream_reader(Cursor::new(body.as_bytes()), &mut on, |l| {
            match sse_data(l) {
                Some(d) => anthropic_stream_delta(d),
                None => Ok(None),
            }
        });
        assert!(matches!(err, Err(AiError::BadResponse(_))));
    }

    #[test]
    fn gemini_block_reason_in_stream_is_error() {
        let body = "data: {\"promptFeedback\":{\"blockReason\":\"SAFETY\"}}\n";
        let mut on = |_: &str| {};
        let err = stream_reader(Cursor::new(body.as_bytes()), &mut on, |l| {
            match sse_data(l) {
                Some(d) => gemini_stream_delta(d),
                None => Ok(None),
            }
        });
        assert!(matches!(err, Err(AiError::BadResponse(_))));
    }

    #[test]
    fn with_stream_sets_flag_true() {
        // Reusa o corpo do não-streaming e liga o flag (ollama já vem com false).
        let b = with_stream(ollama_body("llama3", "sys", "user"));
        assert_eq!(b.pointer("/stream"), Some(&Value::Bool(true)));
    }

    #[test]
    fn anthropic_error_event_in_stream_aborts_not_truncates() {
        // Um `event: error` in-band (após deltas de texto) vira Err(Http), NÃO
        // Ok(parcial) — não trunca silenciosamente a resposta.
        let body = concat!(
            "data: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"Deus \"}}\n",
            "event: error\n",
            "data: {\"type\":\"error\",\"error\":{\"type\":\"overloaded_error\",\"message\":\"Overloaded\"}}\n",
        );
        let mut on = |_: &str| {};
        let err = stream_reader(Cursor::new(body.as_bytes()), &mut on, |l| {
            match sse_data(l) {
                Some(d) => anthropic_stream_delta(d),
                None => Ok(None),
            }
        });
        match err {
            Err(AiError::Http(m)) => assert!(m.contains("Overloaded"), "{m}"),
            other => panic!("esperava Err(Http), veio {other:?}"),
        }
    }

    #[test]
    fn openai_error_frame_in_stream_is_http_error() {
        let body = "data: {\"error\":{\"message\":\"rate limit\",\"type\":\"rate_limit_error\"}}\n";
        let mut on = |_: &str| {};
        let err = stream_reader(Cursor::new(body.as_bytes()), &mut on, |l| {
            match sse_data(l) {
                Some(d) => openai_stream_delta(d),
                None => Ok(None),
            }
        });
        match err {
            Err(AiError::Http(m)) => assert!(m.contains("rate limit"), "{m}"),
            other => panic!("esperava Err(Http), veio {other:?}"),
        }
    }

    #[test]
    fn gemini_error_frame_in_stream_is_http_error() {
        let body =
            "data: {\"error\":{\"code\":429,\"message\":\"RESOURCE_EXHAUSTED\",\"status\":\"RESOURCE_EXHAUSTED\"}}\n";
        let mut on = |_: &str| {};
        let err = stream_reader(Cursor::new(body.as_bytes()), &mut on, |l| {
            match sse_data(l) {
                Some(d) => gemini_stream_delta(d),
                None => Ok(None),
            }
        });
        match err {
            Err(AiError::Http(m)) => assert!(m.contains("RESOURCE_EXHAUSTED"), "{m}"),
            other => panic!("esperava Err(Http), veio {other:?}"),
        }
    }

    #[test]
    fn ollama_error_line_in_stream_is_http_error() {
        let body = concat!(
            "{\"message\":{\"content\":\"Deus \"},\"done\":false}\n",
            "{\"error\":\"model not found\"}\n",
        );
        let mut on = |_: &str| {};
        let err = stream_reader(Cursor::new(body.as_bytes()), &mut on, ollama_stream_delta);
        match err {
            Err(AiError::Http(m)) => assert!(m.contains("model not found"), "{m}"),
            other => panic!("esperava Err(Http), veio {other:?}"),
        }
    }
}
