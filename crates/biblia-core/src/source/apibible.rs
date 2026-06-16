//! Conector [API.Bible](https://scripture.api.bible) (opt-in, versões protegidas).
//!
//! Busca a passagem **ao vivo** com a chave do usuário; nunca embarca nem
//! cacheia o texto em massa (a `Passage` devolvida é efêmera, para exibição).
//! `GET /v1/bibles/{bibleId}/passages/{passageId}` com cabeçalho `api-key`; o
//! texto vem em `data.content`.

use serde_json::Value;

use crate::model::{Passage, Reference, SearchHit, Translation, TranslationId, Verse, VerseRange};
use crate::search::SearchOptions;

use super::http;
use super::{BibleSource, Result, SourceError};

const BASE: &str = "https://api.scripture.api.bible/v1";

/// Código USFM (Paratext) por número canônico do livro (`1..=66`).
const USFM: [&str; 66] = [
    "GEN", "EXO", "LEV", "NUM", "DEU", "JOS", "JDG", "RUT", "1SA", "2SA", "1KI", "2KI", "1CH",
    "2CH", "EZR", "NEH", "EST", "JOB", "PSA", "PRO", "ECC", "SNG", "ISA", "JER", "LAM", "EZK",
    "DAN", "HOS", "JOL", "AMO", "OBA", "JON", "MIC", "NAM", "HAB", "ZEP", "HAG", "ZEC", "MAL",
    "MAT", "MRK", "LUK", "JHN", "ACT", "ROM", "1CO", "2CO", "GAL", "EPH", "PHP", "COL", "1TH",
    "2TH", "1TI", "2TI", "TIT", "PHM", "HEB", "JAS", "1PE", "2PE", "1JN", "2JN", "3JN", "JUD",
    "REV",
];

/// Código USFM do livro (`None` fora de `1..=66`).
pub fn usfm_code(book: u8) -> Option<&'static str> {
    if (1..=66).contains(&book) {
        Some(USFM[(book - 1) as usize])
    } else {
        None
    }
}

/// `passageId` USFM: `"JHN.3.16"`, `"JHN.3.16-JHN.3.18"`, `"JHN.3"`.
pub fn passage_id(reference: &Reference) -> Option<String> {
    let code = usfm_code(reference.book)?;
    let c = reference.chapter;
    Some(match reference.verses {
        VerseRange::Single(v) => format!("{code}.{c}.{v}"),
        VerseRange::Range { start, end } => format!("{code}.{c}.{start}-{code}.{c}.{end}"),
        VerseRange::WholeChapter => format!("{code}.{c}"),
    })
}

/// URL do endpoint de passagem (sem query).
pub fn passage_url(bible_id: &str, passage_id: &str) -> String {
    format!("{BASE}/bibles/{bible_id}/passages/{passage_id}")
}

/// Query de texto puro (sem números/títulos/notas).
fn text_query() -> [(&'static str, &'static str); 6] {
    [
        ("content-type", "text"),
        ("include-verse-numbers", "false"),
        ("include-chapter-numbers", "false"),
        ("include-notes", "false"),
        ("include-titles", "false"),
        ("include-verse-spans", "false"),
    ]
}

/// Extrai o texto de `data.content`.
pub fn parse_content(v: &Value) -> Result<String> {
    let text = v
        .pointer("/data/content")
        .and_then(Value::as_str)
        .ok_or_else(|| SourceError::Http("resposta sem `data.content`".into()))?;
    Ok(text.trim().to_string())
}

/// Fonte API.Bible para uma única tradução protegida.
pub struct ApiBibleSource {
    bible_id: String,
    translation: Translation,
    key: String,
}

impl ApiBibleSource {
    /// Cria o conector. `translation.id` é o slug usado em `--version`.
    pub fn new(
        bible_id: impl Into<String>,
        translation: Translation,
        key: impl Into<String>,
    ) -> Self {
        ApiBibleSource {
            bible_id: bible_id.into(),
            translation,
            key: key.into(),
        }
    }
}

impl BibleSource for ApiBibleSource {
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
        let pid = passage_id(r)
            .ok_or_else(|| SourceError::UnknownTranslation(format!("livro {}", r.book)))?;
        let url = passage_url(&self.bible_id, &pid);
        let json = http::get_json(&url, &[("api-key", &self.key)], &text_query())?;
        let content = parse_content(&json)?;
        if content.is_empty() {
            return Ok(Passage {
                reference: *r,
                verses: vec![],
            });
        }
        // Texto efêmero do conector: um único bloco, numerado no versículo inicial.
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
            id: TranslationId::new("ara"),
            abbrev: "ARA".into(),
            name: "Almeida Revista e Atualizada".into(),
            language: Lang::Pt,
            license: License::Proprietary,
            embeddable: false,
        }
    }

    #[test]
    fn usfm_codes() {
        assert_eq!(usfm_code(1), Some("GEN"));
        assert_eq!(usfm_code(43), Some("JHN"));
        assert_eq!(usfm_code(66), Some("REV"));
        assert_eq!(usfm_code(0), None);
        assert_eq!(usfm_code(67), None);
    }

    #[test]
    fn passage_ids() {
        let single = Reference::single(43, 3, 16);
        assert_eq!(passage_id(&single).unwrap(), "JHN.3.16");
        let range = Reference {
            book: 43,
            chapter: 3,
            verses: VerseRange::Range { start: 16, end: 18 },
        };
        assert_eq!(passage_id(&range).unwrap(), "JHN.3.16-JHN.3.18");
        let chap = Reference::whole_chapter(19, 23);
        assert_eq!(passage_id(&chap).unwrap(), "PSA.23");
    }

    #[test]
    fn url_and_query_shape() {
        let url = passage_url("de4e12af7f28f599-02", "JHN.3.16");
        assert!(url.starts_with("https://api.scripture.api.bible/v1/bibles/"));
        assert!(url.ends_with("/passages/JHN.3.16"));
        assert!(text_query().contains(&("content-type", "text")));
        assert!(text_query().contains(&("include-verse-numbers", "false")));
    }

    #[test]
    fn parse_content_reads_data_content() {
        let v = serde_json::json!({ "data": { "content": "  Porque Deus amou o mundo  " } });
        assert_eq!(parse_content(&v).unwrap(), "Porque Deus amou o mundo");
        let bad = serde_json::json!({ "data": {} });
        assert!(parse_content(&bad).is_err());
    }

    #[test]
    fn metadata_and_unsupported_search() {
        let s = ApiBibleSource::new("bid", translation(), "key");
        assert!(s.has_translation(&TranslationId::new("ara")).unwrap());
        assert!(!s.has_translation(&TranslationId::new("kjv")).unwrap());
        assert_eq!(s.translations().unwrap().len(), 1);
        assert!(!s.is_embeddable());
        assert!(matches!(
            s.search("graça", &SearchOptions::new(TranslationId::new("ara"))),
            Err(SourceError::Unsupported(_))
        ));
    }
}
