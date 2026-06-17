//! Tipos de domínio da The Light (ver `IMPLEMENTATION_PLAN.md` §4).
//!
//! Estes são tipos puros, sem I/O. O parser de referências vive em
//! [`crate::reference`]; a persistência em [`crate::store`].

use serde::{Deserialize, Serialize};
use std::fmt;

/// Idioma de uma tradução (ou de exibição de referências).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    /// Português.
    Pt,
    /// Inglês.
    En,
}

impl Lang {
    /// Código ISO 639-1 (`"pt"` / `"en"`), como gravado no banco.
    pub fn code(self) -> &'static str {
        match self {
            Lang::Pt => "pt",
            Lang::En => "en",
        }
    }
}

impl fmt::Display for Lang {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

/// Erro ao converter um código de idioma desconhecido.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("idioma desconhecido: {0:?}")]
pub struct UnknownLang(pub String);

impl std::str::FromStr for Lang {
    type Err = UnknownLang;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "pt" | "pt-br" | "por" | "português" | "portugues" => Ok(Lang::Pt),
            "en" | "en-us" | "eng" | "english" | "inglês" | "ingles" => Ok(Lang::En),
            other => Err(UnknownLang(other.to_string())),
        }
    }
}

/// Testamento ao qual um livro pertence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Testament {
    /// Antigo Testamento.
    Old,
    /// Novo Testamento.
    New,
}

impl Testament {
    /// Código gravado no banco (`"OT"` / `"NT"`).
    pub fn code(self) -> &'static str {
        match self {
            Testament::Old => "OT",
            Testament::New => "NT",
        }
    }
}

/// Licença de uma tradução. Governa se o texto pode ser embarcado/redistribuído.
///
/// Apenas [`License::PublicDomain`], [`License::Cc0`] e variantes livres de
/// [`License::Cc`] são consideradas embarcáveis (ver `SPEC.md` §3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum License {
    /// Domínio público.
    PublicDomain,
    /// Creative Commons Zero (equivalente a domínio público).
    Cc0,
    /// Creative Commons com atribuição (ex.: `"cc-by"`, `"cc-by-sa"`).
    Cc(String),
    /// Tradução protegida — somente via conector com chave do usuário.
    Proprietary,
    /// Outra licença, registrada como texto livre.
    Other(String),
}

impl License {
    /// Texto canônico gravado na coluna `translations.license`.
    pub fn as_str(&self) -> &str {
        match self {
            License::PublicDomain => "public-domain",
            License::Cc0 => "cc0",
            License::Cc(s) => s,
            License::Proprietary => "proprietary",
            License::Other(s) => s,
        }
    }

    /// `true` se a licença permite embarcar/redistribuir o texto no binário.
    ///
    /// Domínio público e CC0 são sempre livres. Entre as Creative Commons,
    /// apenas variantes **sem** cláusula NonCommercial (`nc`) ou NoDerivatives
    /// (`nd`) são consideradas livres para redistribuição — `cc-by` e `cc-by-sa`
    /// passam; `cc-by-nc`, `cc-by-nd`, `cc-by-nc-sa` etc. não.
    pub fn is_embeddable(&self) -> bool {
        match self {
            License::PublicDomain | License::Cc0 => true,
            License::Cc(s) => {
                let s = s.to_ascii_lowercase();
                !s.contains("nc") && !s.contains("nd")
            }
            License::Proprietary | License::Other(_) => false,
        }
    }
}

impl fmt::Display for License {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl From<&str> for License {
    fn from(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "public-domain" | "public domain" | "pd" => License::PublicDomain,
            "cc0" | "cc-0" => License::Cc0,
            "proprietary" | "copyrighted" | "all-rights-reserved" => License::Proprietary,
            other if other.starts_with("cc-") || other.starts_with("cc ") => {
                License::Cc(other.replace(' ', "-"))
            }
            other => License::Other(other.to_string()),
        }
    }
}

/// Identificador estável de uma tradução (slug usado como chave no banco),
/// ex.: `"kjv"`, `"almeida-livre"`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TranslationId(pub String);

impl TranslationId {
    /// Cria um id normalizando para minúsculas e trim.
    pub fn new(s: impl Into<String>) -> Self {
        TranslationId(s.into().trim().to_ascii_lowercase())
    }

    /// Slug interno.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for TranslationId {
    fn from(s: &str) -> Self {
        TranslationId::new(s)
    }
}

impl fmt::Display for TranslationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Metadados de uma tradução disponível.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Translation {
    /// Slug (`TranslationId`).
    pub id: TranslationId,
    /// Abreviação de exibição, ex.: `"KJV"`.
    pub abbrev: String,
    /// Nome completo, ex.: `"King James Version"`.
    pub name: String,
    /// Idioma do texto.
    pub language: Lang,
    /// Licença do texto.
    pub license: License,
    /// `true` se o texto está embarcado e pode ser redistribuído.
    pub embeddable: bool,
}

/// Quais versículos de um capítulo uma referência abrange.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum VerseRange {
    /// Um único versículo (`João 3.16`).
    Single(u16),
    /// Intervalo inclusivo (`Gênesis 1.1-3`). Invariante: `start <= end`.
    Range { start: u16, end: u16 },
    /// O capítulo inteiro (`Salmos 23`).
    WholeChapter,
}

impl VerseRange {
    /// Primeiro versículo abrangido, ou `None` para capítulo inteiro.
    pub fn start(&self) -> Option<u16> {
        match self {
            VerseRange::Single(v) => Some(*v),
            VerseRange::Range { start, .. } => Some(*start),
            VerseRange::WholeChapter => None,
        }
    }

    /// Último versículo abrangido, ou `None` para capítulo inteiro.
    pub fn end(&self) -> Option<u16> {
        match self {
            VerseRange::Single(v) => Some(*v),
            VerseRange::Range { end, .. } => Some(*end),
            VerseRange::WholeChapter => None,
        }
    }

    /// `true` se o versículo `v` está dentro deste intervalo.
    pub fn contains(&self, v: u16) -> bool {
        match self {
            VerseRange::Single(s) => *s == v,
            VerseRange::Range { start, end } => *start <= v && v <= *end,
            VerseRange::WholeChapter => true,
        }
    }
}

/// Uma referência bíblica resolvida: livro canônico + capítulo + intervalo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Reference {
    /// Número canônico do livro, `1..=66`.
    pub book: u8,
    /// Capítulo, `>= 1`.
    pub chapter: u16,
    /// Versículos abrangidos.
    pub verses: VerseRange,
}

impl Reference {
    /// Referência a um único versículo.
    pub fn single(book: u8, chapter: u16, verse: u16) -> Self {
        Reference {
            book,
            chapter,
            verses: VerseRange::Single(verse),
        }
    }

    /// Referência a um capítulo inteiro.
    pub fn whole_chapter(book: u8, chapter: u16) -> Self {
        Reference {
            book,
            chapter,
            verses: VerseRange::WholeChapter,
        }
    }
}

/// Um versículo com seu texto, vindo de uma tradução específica.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Verse {
    /// Livro/capítulo/versículo deste texto (`verses` é sempre `Single`).
    pub reference: Reference,
    /// Texto do versículo.
    pub text: String,
    /// Tradução de origem.
    pub translation: TranslationId,
}

/// Uma passagem: a referência pedida e os versículos resolvidos, em ordem.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Passage {
    /// Referência originalmente solicitada.
    pub reference: Reference,
    /// Versículos resolvidos, em ordem canônica.
    pub verses: Vec<Verse>,
}

impl Passage {
    /// `true` se nenhum versículo foi encontrado para a referência.
    pub fn is_empty(&self) -> bool {
        self.verses.is_empty()
    }

    /// Números de versículo presentes (início de cada intervalo); pula capítulo
    /// inteiro. Usado para agregar referências cruzadas de toda a passagem.
    pub fn verse_numbers(&self) -> Vec<u16> {
        self.verses
            .iter()
            .filter_map(|v| match v.reference.verses {
                VerseRange::Single(n) => Some(n),
                VerseRange::Range { start, .. } => Some(start),
                VerseRange::WholeChapter => None,
            })
            .collect()
    }
}

/// Um resultado de busca full-text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchHit {
    /// Versículo onde o termo foi encontrado.
    pub reference: Reference,
    /// Tradução de origem.
    pub translation: TranslationId,
    /// Texto do versículo (sem marcação).
    pub text: String,
    /// Texto com os termos casados envolvidos por marcadores
    /// ([`crate::search::HL_START`]/[`crate::search::HL_END`]).
    pub highlighted: String,
    /// Pontuação BM25 (menor = melhor correspondência).
    pub score: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embeddable_licenses() {
        assert!(License::PublicDomain.is_embeddable());
        assert!(License::Cc0.is_embeddable());
        assert!(License::from("cc-by").is_embeddable());
        assert!(License::from("cc-by-sa").is_embeddable());
    }

    #[test]
    fn non_embeddable_licenses() {
        // NonCommercial e NoDerivatives não são livres para redistribuição.
        assert!(!License::from("cc-by-nc").is_embeddable());
        assert!(!License::from("cc-by-nd").is_embeddable());
        assert!(!License::from("cc-by-nc-sa").is_embeddable());
        assert!(!License::Proprietary.is_embeddable());
        assert!(!License::from("all-rights-reserved").is_embeddable());
    }

    #[test]
    fn license_roundtrips_text() {
        assert_eq!(License::from("public-domain"), License::PublicDomain);
        assert_eq!(License::PublicDomain.as_str(), "public-domain");
        assert_eq!(License::from("cc-by").as_str(), "cc-by");
    }
}
