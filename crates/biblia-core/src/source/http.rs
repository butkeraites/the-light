//! Cliente HTTP mínimo para conectores (GET → JSON), com verificação de status
//! **antes** de exigir JSON válido. Os testes nunca chamam a rede; a parte pura
//! ([`parse_http_json`]) é testada isoladamente.

use std::time::Duration;

use serde_json::Value;

use super::{Result, SourceError};

const TIMEOUT: Duration = Duration::from_secs(60);

/// Faz um GET com cabeçalhos e parâmetros de query, devolvendo o JSON.
pub(crate) fn get_json(
    url: &str,
    headers: &[(&str, &str)],
    query: &[(&str, &str)],
) -> Result<Value> {
    let client = reqwest::blocking::Client::builder()
        .timeout(TIMEOUT)
        .build()
        .map_err(|e| SourceError::Http(e.to_string()))?;
    let full = reqwest::Url::parse_with_params(url, query.iter().copied())
        .map_err(|e| SourceError::Http(format!("URL inválida: {e}")))?;
    let mut req = client.get(full);
    for (k, v) in headers {
        req = req.header(*k, *v);
    }
    let resp = req.send().map_err(|e| SourceError::Http(e.to_string()))?;
    let status = resp.status();
    let text = resp.text().map_err(|e| SourceError::Http(e.to_string()))?;
    parse_http_json(status.is_success(), status.as_str(), &text)
}

/// Núcleo puro: status-first. Em erro, extrai uma mensagem legível (ou o corpo
/// bruto truncado), nunca um erro genérico de parsing.
pub(crate) fn parse_http_json(success: bool, status: &str, body: &str) -> Result<Value> {
    if !success {
        let msg = serde_json::from_str::<Value>(body)
            .ok()
            .and_then(|v| {
                v.get("message")
                    .or_else(|| v.pointer("/error/message"))
                    .or_else(|| v.get("error"))
                    .or_else(|| v.get("detail"))
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .unwrap_or_else(|| body.trim().chars().take(200).collect());
        return Err(SourceError::Http(format!("HTTP {status}: {msg}")));
    }
    serde_json::from_str(body).map_err(|e| SourceError::Http(format!("resposta inválida: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_checked_before_json() {
        // 2xx + JSON válido → Ok.
        assert!(parse_http_json(true, "200", r#"{"a":1}"#).is_ok());
        // erro com mensagem JSON → mensagem da API.
        let e = parse_http_json(false, "401", r#"{"message":"chave inválida"}"#)
            .unwrap_err()
            .to_string();
        assert!(e.contains("401") && e.contains("chave inválida"), "{e}");
        // erro com corpo NÃO-JSON → usa o corpo bruto, não trava.
        let e = parse_http_json(false, "502", "Bad Gateway")
            .unwrap_err()
            .to_string();
        assert!(e.contains("502") && e.contains("Bad Gateway"), "{e}");
        // 2xx com corpo inválido → erro de resposta (não pânico).
        assert!(parse_http_json(true, "200", "not json").is_err());
    }
}
