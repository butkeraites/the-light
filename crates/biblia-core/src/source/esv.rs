//! Conector [ESV API](https://api.esv.org) (opt-in, texto protegido).
//!
//! `GET /v3/passage/text/?q=<ref>` com cabeçalho `Authorization: Token <key>`;
//! o texto vem em `passages[0]`. Mantemos `include-short-copyright=true` para
//! preservar a atribuição "(ESV)" exigida pelos termos da Crossway; o texto é
//! efêmero (uso pessoal), nunca embarcado nem cacheado em massa.

use serde_json::Value;

use crate::model::{Passage, Reference, SearchHit, Translation, TranslationId, Verse, VerseRange};
use crate::reference::book_info;
use crate::search::SearchOptions;

use super::http;
use super::{BibleSource, Result, SourceError};

const BASE: &str = "https://api.esv.org/v3/passage/text/";

/// Monta o parâmetro `q` em inglês: `"John 3:16"`, `"John 3:16-18"`, `"John 3"`.
pub fn esv_query(reference: &Reference) -> Option<String> {
    let name = book_info(reference.book)?.name_en;
    let c = reference.chapter;
    Some(match reference.verses {
        VerseRange::Single(v) => format!("{name} {c}:{v}"),
        VerseRange::Range { start, end } => format!("{name} {c}:{start}-{end}"),
        VerseRange::WholeChapter => format!("{name} {c}"),
    })
}

/// Parâmetros de query para texto limpo (mantendo a atribuição "(ESV)").
fn text_query(q: &str) -> [(&'static str, &str); 6] {
    [
        ("q", q),
        ("include-passage-references", "false"),
        ("include-verse-numbers", "false"),
        ("include-footnotes", "false"),
        ("include-headings", "false"),
        ("include-short-copyright", "true"),
    ]
}

/// Extrai o texto de `passages[0]`.
pub fn parse_passages(v: &Value) -> Result<String> {
    let p = v
        .pointer("/passages/0")
        .and_then(Value::as_str)
        .ok_or_else(|| SourceError::Http("resposta sem `passages[0]`".into()))?;
    let t = p.trim().to_string();
    if t.is_empty() {
        return Err(SourceError::Http("passagem vazia".into()));
    }
    Ok(t)
}

/// Fonte ESV API (uma única tradução protegida).
pub struct EsvApiSource {
    translation: Translation,
    key: String,
}

impl EsvApiSource {
    /// Cria o conector. `translation.id` é o slug usado em `--version`.
    pub fn new(translation: Translation, key: impl Into<String>) -> Self {
        EsvApiSource {
            translation,
            key: key.into(),
        }
    }
}

impl BibleSource for EsvApiSource {
    fn translations(&self) -> Result<Vec<Translation>> {
        Ok(vec![self.translation.clone()])
    }

    fn has_translation(&self, t: &TranslationId) -> Result<bool> {
        Ok(*t == self.translation.id)
    }

    fn passage(&self, r: &Reference, t: &TranslationId) -> Result<Passage> {
        if *t != self.translation.id {
            return Err(SourceError::UnknownTranslation(t.to_string()));
        }
        let q = esv_query(r)
            .ok_or_else(|| SourceError::UnknownTranslation(format!("livro {}", r.book)))?;
        let auth = format!("Token {}", self.key);
        let json = http::get_json(BASE, &[("authorization", &auth)], &text_query(&q))?;
        let content = parse_passages(&json)?;
        let start = r.verses.start().unwrap_or(1);
        Ok(Passage {
            reference: *r,
            verses: vec![Verse {
                reference: Reference::single(r.book, r.chapter, start),
                text: content,
                translation: t.clone(),
            }],
        })
    }

    fn search(&self, _query: &str, _opts: &SearchOptions) -> Result<Vec<SearchHit>> {
        Err(SourceError::Unsupported(
            "busca full-text só em versões locais".into(),
        ))
    }

    fn is_embeddable(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Lang, License};

    fn translation() -> Translation {
        Translation {
            id: TranslationId::new("esv"),
            abbrev: "ESV".into(),
            name: "English Standard Version".into(),
            language: Lang::En,
            license: License::Proprietary,
            embeddable: false,
        }
    }

    #[test]
    fn queries_in_english() {
        assert_eq!(
            esv_query(&Reference::single(43, 3, 16)).unwrap(),
            "John 3:16"
        );
        let range = Reference {
            book: 43,
            chapter: 3,
            verses: VerseRange::Range { start: 16, end: 18 },
        };
        assert_eq!(esv_query(&range).unwrap(), "John 3:16-18");
        assert_eq!(
            esv_query(&Reference::whole_chapter(19, 23)).unwrap(),
            "Psalms 23"
        );
    }

    #[test]
    fn query_keeps_copyright_and_strips_extras() {
        let q = text_query("John 3:16");
        assert!(q.contains(&("q", "John 3:16")));
        assert!(q.contains(&("include-short-copyright", "true")));
        assert!(q.contains(&("include-headings", "false")));
    }

    #[test]
    fn parse_passages_reads_first() {
        let v = serde_json::json!({ "passages": ["  [16] For God so loved... (ESV)  "] });
        assert_eq!(
            parse_passages(&v).unwrap(),
            "[16] For God so loved... (ESV)"
        );
        assert!(parse_passages(&serde_json::json!({ "passages": [] })).is_err());
        assert!(parse_passages(&serde_json::json!({ "passages": [""] })).is_err());
    }

    #[test]
    fn metadata_and_unsupported_search() {
        let s = EsvApiSource::new(translation(), "key");
        assert!(s.has_translation(&TranslationId::new("esv")).unwrap());
        assert!(!s.is_embeddable());
        assert!(matches!(
            s.search("grace", &SearchOptions::new(TranslationId::new("esv"))),
            Err(SourceError::Unsupported(_))
        ));
    }
}
