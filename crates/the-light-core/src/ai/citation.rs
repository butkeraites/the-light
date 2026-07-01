//! Aparato acadêmico: modelo de **citação**, formatação **SBL** (notas de
//! rodapé e bibliografia) e **rodapé de procedência** (separa o verificável do
//! gerado por IA).
//!
//! Invariante central: **o LLM nunca produz uma `Citation`**. As citações são
//! construídas a partir de dados do banco (léxico verificado) ou de URLs
//! realmente buscadas (pesquisa web, fase 4). O modelo apenas emite âncoras
//! `[V:H7225]`; o motor valida-as e troca por marcadores `[^chave]` de forma
//! determinística (ver [`rewrite_anchors`]).

use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::model::Lang;

use super::lexicon::VerifiedLexicon;
// `WebSource` (pesquisa web, Fase 4) puxa `chrono` via `research` → só `embedded`.
#[cfg(feature = "embedded")]
use super::research::WebSource;

/// Tipo de uma citação (define onde aparece e se é verificável).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CitationKind {
    /// Passagem bíblica (citada no texto; fora da bibliografia, padrão SBL).
    Scripture,
    /// Entrada de léxico por número de Strong (vira nota de rodapé).
    Lexicon,
    /// Obra de referência (léxico/dataset) — entra na bibliografia.
    Source,
    /// Fonte secundária recuperada da web (fase 4) — nota + bibliografia.
    Web,
}

impl CitationKind {
    /// Se a citação aponta para um fato verificável (banco/URL real).
    pub fn is_verifiable(self) -> bool {
        matches!(
            self,
            CitationKind::Scripture | CitationKind::Lexicon | CitationKind::Source
        )
    }

    /// Se entra na bibliografia (SBL exclui a Escritura).
    pub fn in_bibliography(self) -> bool {
        matches!(self, CitationKind::Source | CitationKind::Web)
    }
}

/// Uma citação estruturada (round-trippável p/ o sidecar `.citations.json`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Citation {
    /// Tipo.
    pub kind: CitationKind,
    /// Chave estável e **idêntica à âncora** que o modelo cita (ex.: "H7225",
    /// "W:1") — contrato com [`rewrite_anchors`].
    pub key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locus: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publisher: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub year: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Rótulo de licença para exibição (ex.: "CC BY 4.0").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    /// Atribuição verbatim exigida pela fonte.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attribution: Option<String>,
    /// Data de acesso (fontes web).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accessed: Option<String>,
    /// Trecho verbatim (fontes web — citado entre aspas, nunca parafraseado).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quote: Option<String>,
}

impl Citation {
    fn empty(kind: CitationKind, key: impl Into<String>) -> Self {
        Citation {
            kind,
            key: key.into(),
            author: None,
            title: None,
            locus: None,
            publisher: None,
            year: None,
            url: None,
            license: None,
            attribution: None,
            accessed: None,
            quote: None,
        }
    }
}

/// Nome da obra de referência por testamento (léxico breve da STEP).
fn lexicon_title(testament: &str) -> &'static str {
    match testament {
        "NT" => "Translators Brief lexicon of Extended Strongs for Greek (TBESG)",
        _ => "Translators Brief lexicon of Extended Strongs for Hebrew (TBESH)",
    }
}

/// Coletor de citações: dedup por chave, preservando a ordem de inserção. O
/// modelo nunca o alimenta — só dados do banco/URLs reais.
#[derive(Debug, Default)]
pub struct CitationCollector {
    by_key: BTreeMap<String, Citation>,
    order: Vec<String>,
}

impl CitationCollector {
    /// Coletor vazio.
    pub fn new() -> Self {
        Self::default()
    }

    fn push(&mut self, c: Citation) {
        if !self.by_key.contains_key(&c.key) {
            self.order.push(c.key.clone());
        }
        self.by_key.entry(c.key.clone()).or_insert(c);
    }

    /// Adiciona as citações derivadas do léxico verificado: uma nota por Strong
    /// citável + as obras de referência (bibliografia) usadas.
    pub fn from_verified_lexicon(&mut self, vl: &VerifiedLexicon) {
        let attribution = vl.sources.first().cloned();
        let mut bib_done: HashSet<&'static str> = HashSet::new();
        for e in &vl.entries {
            let title = lexicon_title(&e.testament);
            let lemma = e.lemma.clone().unwrap_or_default();
            let mut c = Citation::empty(CitationKind::Lexicon, e.strongs.clone());
            c.author = Some("STEP Bible".to_string());
            c.title = Some(title.to_string());
            c.locus = Some(if lemma.is_empty() {
                e.strongs.clone()
            } else {
                format!("{lemma} ({})", e.strongs)
            });
            c.license = Some("CC BY 4.0".to_string());
            c.attribution = attribution.clone();
            self.push(c);

            // Obra de referência (bibliografia) — uma por léxico realmente usado.
            if bib_done.insert(title) {
                let mut src = Citation::empty(CitationKind::Source, format!("src:{title}"));
                src.author = Some("STEP Bible".to_string());
                src.title = Some(title.to_string());
                src.publisher = Some("Tyndale House, Cambridge".to_string());
                src.license = Some("CC BY 4.0".to_string());
                src.attribution = attribution.clone();
                self.push(src);
            }
        }
    }

    /// Adiciona citações de fontes web (chave `W1`, `W2`, … = âncora `[W:n]`).
    /// O trecho é guardado verbatim para a nota; nunca parafraseado.
    ///
    /// Depende de `WebSource` (Fase 4, `chrono`) → só no caminho `embedded`.
    #[cfg(feature = "embedded")]
    pub fn from_web_results(&mut self, sources: &[WebSource]) {
        for (i, ws) in sources.iter().enumerate() {
            let mut c = Citation::empty(CitationKind::Web, format!("W{}", i + 1));
            c.title = Some(ws.title.clone());
            c.url = Some(ws.url.clone());
            c.quote = Some(ws.snippet.clone());
            c.accessed = Some(ws.fetched_at.format("%Y-%m-%d").to_string());
            c.attribution = Some(ws.site.clone());
            self.push(c);
        }
    }

    /// Devolve as citações na ordem de inserção.
    pub fn into_vec(self) -> Vec<Citation> {
        self.order
            .into_iter()
            .filter_map(|k| self.by_key.get(&k).cloned())
            .collect()
    }
}

/// Serializa citações para o sidecar `.citations.json`.
pub fn to_json(cites: &[Citation]) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(cites)
}

/// Lê citações de um sidecar `.citations.json`.
pub fn from_json(s: &str) -> Result<Vec<Citation>, serde_json::Error> {
    serde_json::from_str(s)
}

/// Strong base (mesma normalização do léxico): "H7225G" → "H7225".
fn base_strong(s: &str) -> String {
    s.trim()
        .trim_end_matches(|c: char| c.is_ascii_alphabetic())
        .to_string()
}

/// `true` se o token tem a forma de um Strong (`H`/`G` + dígitos + letras).
fn is_strong(tok: &str) -> bool {
    let mut chars = tok.chars();
    if !matches!(chars.next(), Some('H') | Some('G')) {
        return false;
    }
    let mut saw_digit = false;
    for c in chars {
        if c.is_ascii_digit() {
            saw_digit = true;
        } else if !c.is_ascii_alphabetic() {
            return false;
        }
    }
    saw_digit
}

/// Troca âncoras `[V:Strong]` (léxico) e `[W:n]` (fonte web) por marcadores de
/// nota `[^chave]` quando a chave é válida (consta de `valid`); âncoras inválidas
/// são **removidas**. Determinístico — nunca confia no modelo para posicionar a
/// citação, só para emitir a âncora. Seguro para UTF-8 (opera por fatias).
pub fn rewrite_anchors(text: &str, valid: &HashSet<String>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    loop {
        let v = rest.find("[V:");
        let w = rest.find("[W:");
        let (pos, is_web) = match (v, w) {
            (Some(a), Some(b)) if a <= b => (a, false),
            (Some(_), Some(b)) => (b, true),
            (Some(a), None) => (a, false),
            (None, Some(b)) => (b, true),
            (None, None) => break,
        };
        out.push_str(&rest[..pos]);
        let after = &rest[pos + 3..]; // "[V:"/"[W:" são ASCII → fatia válida
        if let Some(close) = after.find(']') {
            let tok = &after[..close];
            let key = if is_web {
                if !tok.is_empty() && tok.chars().all(|c| c.is_ascii_digit()) {
                    Some(format!("W{tok}"))
                } else {
                    None
                }
            } else if is_strong(tok) {
                Some(base_strong(tok))
            } else {
                None
            };
            if let Some(k) = key {
                if valid.contains(&k) {
                    out.push_str(&format!("[^{k}]"));
                }
                // âncora inválida: removida (nada é escrito)
                rest = &after[close + 1..];
                continue;
            }
        }
        // Não é uma âncora bem-formada: mantém o prefixo literal e segue.
        out.push_str(if is_web { "[W:" } else { "[V:" });
        rest = after;
    }
    out.push_str(rest);
    out
}

/// Nota de rodapé no estilo SBL para uma citação.
pub fn sbl_footnote(c: &Citation, _lang: Lang) -> String {
    match c.kind {
        CitationKind::Lexicon | CitationKind::Source => {
            let author = c.author.as_deref().unwrap_or("");
            let title = c.title.as_deref().unwrap_or("");
            let mut s = format!("{author}, *{title}*");
            if let Some(locus) = &c.locus {
                s.push_str(&format!(", s.v. “{locus}”"));
            }
            s.push('.');
            s
        }
        CitationKind::Web => {
            let title = c.title.as_deref().unwrap_or("");
            let mut s = String::new();
            // Trecho verbatim primeiro (P1-2: o leitor vê o texto buscado, não a
            // paráfrase do modelo).
            if let Some(q) = &c.quote {
                s.push_str(&format!("“{q}” "));
            }
            s.push_str(&format!("— *{title}*"));
            if let Some(url) = &c.url {
                s.push_str(&format!(", {url}"));
            }
            if let Some(acc) = &c.accessed {
                s.push_str(&format!(" (acesso em {acc})"));
            }
            s.push('.');
            s
        }
        CitationKind::Scripture => c.key.clone(),
    }
}

/// Entrada de bibliografia no estilo SBL (autor invertido).
pub fn sbl_bibliography_entry(c: &Citation) -> String {
    let author = c.author.as_deref().unwrap_or("");
    let title = c.title.as_deref().unwrap_or("");
    let mut s = if author.is_empty() {
        format!("*{title}*.")
    } else {
        format!("{author}. *{title}*.")
    };
    if let Some(pub_) = &c.publisher {
        s.push_str(&format!(" {pub_}."));
    }
    if let Some(year) = &c.year {
        s.push_str(&format!(" {year}."));
    }
    if let Some(url) = &c.url {
        s.push_str(&format!(" {url}."));
    }
    if let Some(acc) = &c.accessed {
        s.push_str(&format!(" Acesso em {acc}."));
    }
    if let Some(lic) = &c.license {
        s.push_str(&format!(" {lic}."));
    }
    s
}

/// Rodapé de procedência: separa **verificável** (banco/URLs) do **gerado por
/// IA**, tornando a confiabilidade inequívoca.
pub fn provenance_footer(
    citations: &[Citation],
    provider: &str,
    model: &str,
    _lang: Lang,
) -> String {
    let mut out = String::from("---\n\n**Procedência e verificação**\n\n");

    // Bloco verificável: fontes embarcadas com licença/atribuição.
    let mut verif: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for c in citations.iter().filter(|c| c.kind == CitationKind::Source) {
        let title = c.title.as_deref().unwrap_or("");
        let lic = c.license.as_deref().unwrap_or("");
        let attr = c.attribution.as_deref().unwrap_or("");
        let line = format!("{title} — {lic}. {attr}");
        if seen.insert(line.clone()) {
            verif.push(line);
        }
    }
    out.push_str("- **Verificável (acervo local):** texto bíblico da edição citada");
    if verif.is_empty() {
        out.push_str(".\n");
    } else {
        out.push_str("; dados léxicos de:\n");
        for v in &verif {
            out.push_str(&format!("  - {v}\n"));
        }
    }

    // Bloco web (fase 4) — só aparece se houver fontes recuperadas.
    let web: Vec<&Citation> = citations
        .iter()
        .filter(|c| c.kind == CitationKind::Web)
        .collect();
    if !web.is_empty() {
        out.push_str(&format!(
            "- **Recuperado da web:** {} fonte(s) secundária(s), citadas com trecho e data de acesso.\n",
            web.len()
        ));
    }

    out.push_str(&format!(
        "- **Gerado por IA:** a análise e a interpretação são geradas por {provider}/{model} \
         sob a lente escolhida e podem conter erros — confira sempre as fontes primárias.\n"
    ));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::lexicon::LexicalEntry;

    fn vl() -> VerifiedLexicon {
        VerifiedLexicon {
            entries: vec![
                LexicalEntry {
                    strongs: "H7225".into(),
                    lemma: Some("rēʾšît".into()),
                    translit: None,
                    gloss: Some("beginning".into()),
                    occurrences: 1,
                    testament: "OT".into(),
                },
                LexicalEntry {
                    strongs: "G0026".into(),
                    lemma: Some("agápē".into()),
                    translit: None,
                    gloss: Some("love".into()),
                    occurrences: 2,
                    testament: "NT".into(),
                },
            ],
            sources: vec!["Credit it to 'STEP Bible' (CC BY 4.0)".into()],
        }
    }

    #[test]
    fn collector_builds_footnotes_and_bibliography() {
        let mut c = CitationCollector::new();
        c.from_verified_lexicon(&vl());
        let cites = c.into_vec();
        // 2 notas (léxico) + 2 obras de referência (TBESH/TBESG).
        let notes = cites
            .iter()
            .filter(|c| c.kind == CitationKind::Lexicon)
            .count();
        let srcs = cites
            .iter()
            .filter(|c| c.kind == CitationKind::Source)
            .count();
        assert_eq!(notes, 2);
        assert_eq!(srcs, 2);
        let h = cites.iter().find(|c| c.key == "H7225").unwrap();
        assert_eq!(sbl_footnote(h, Lang::Pt), "STEP Bible, *Translators Brief lexicon of Extended Strongs for Hebrew (TBESH)*, s.v. “rēʾšît (H7225)”.");
    }

    #[test]
    fn rewrite_anchors_keeps_valid_strips_invalid() {
        let valid: HashSet<String> = ["H7225".to_string()].into_iter().collect();
        let got = rewrite_anchors("graça [V:H7225] e falso [V:G9999] fim", &valid);
        assert_eq!(got, "graça [^H7225] e falso  fim");
        // Texto sem âncora passa intacto.
        assert_eq!(rewrite_anchors("sem ancora", &valid), "sem ancora");
    }

    #[test]
    fn bibliography_entry_is_sbl_inverted() {
        let mut c = CitationCollector::new();
        c.from_verified_lexicon(&vl());
        let cites = c.into_vec();
        let src = cites
            .iter()
            .find(|c| c.kind == CitationKind::Source)
            .unwrap();
        let bib = sbl_bibliography_entry(src);
        assert!(bib.starts_with("STEP Bible. *"));
        assert!(bib.contains("Tyndale House, Cambridge."));
        assert!(bib.contains("CC BY 4.0."));
    }

    #[test]
    fn provenance_footer_separates_verifiable_from_ai() {
        let mut c = CitationCollector::new();
        c.from_verified_lexicon(&vl());
        let footer = provenance_footer(&c.into_vec(), "anthropic", "claude-opus-4-8", Lang::Pt);
        assert!(footer.contains("Verificável"));
        assert!(footer.contains("CC BY 4.0"));
        assert!(footer.contains("Gerado por IA"));
        assert!(footer.contains("anthropic/claude-opus-4-8"));
        // Sem web → bloco web ausente.
        assert!(!footer.contains("Recuperado da web"));
    }

    #[test]
    fn citation_json_roundtrips() {
        let mut c = Citation::empty(CitationKind::Web, "W:1");
        c.title = Some("Algum comentário".into());
        c.url = Some("https://example.org".into());
        c.accessed = Some("2026-06-18".into());
        let json = serde_json::to_string(&c).unwrap();
        let back: Citation = serde_json::from_str(&json).unwrap();
        assert_eq!(back, c);
    }
}
