//! Marcações (highlights) persistidas em `highlights.json`.
//!
//! Formato de arquivo legível e versionável: um array de objetos
//! `{ "ref": "John 3:16", "color": "yellow", "tag": "salvação" }`. A referência
//! é gravada em forma canônica (inglês) e re-analisada na carga.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::{Result, UserDataError};
use crate::model::{Lang, Reference};
use crate::reference::{format_reference, parse_reference};

/// Uma marcação sobre um versículo, intervalo ou capítulo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Highlight {
    /// Referência marcada.
    pub reference: Reference,
    /// Cor (nome livre, ex.: `"yellow"`).
    pub color: String,
    /// Etiqueta opcional.
    pub tag: Option<String>,
}

/// Forma serializada (legível) de uma marcação.
#[derive(Serialize, Deserialize)]
struct HighlightDto {
    #[serde(rename = "ref")]
    reference: String,
    color: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tag: Option<String>,
}

impl Highlight {
    fn to_dto(&self) -> HighlightDto {
        HighlightDto {
            reference: format_reference(&self.reference, Lang::En),
            color: self.color.clone(),
            tag: self.tag.clone(),
        }
    }

    fn from_dto(dto: HighlightDto) -> Option<Highlight> {
        let reference = parse_reference(&dto.reference).ok()?;
        Some(Highlight {
            reference,
            color: dto.color,
            tag: dto.tag,
        })
    }
}

/// Coleção de marcações ligada a um arquivo `highlights.json`.
pub struct HighlightStore {
    path: PathBuf,
    items: Vec<Highlight>,
}

impl HighlightStore {
    /// Carrega do caminho dado; arquivo ausente → coleção vazia. Entradas com
    /// referência inválida são ignoradas (arquivo editado à mão).
    pub fn load(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        let items = match std::fs::read_to_string(&path) {
            Ok(s) => {
                let dtos: Vec<HighlightDto> = serde_json::from_str(&s)?;
                dtos.into_iter().filter_map(Highlight::from_dto).collect()
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(UserDataError::Io(e)),
        };
        Ok(HighlightStore { path, items })
    }

    /// Carrega do caminho padrão (`highlights.json`).
    pub fn load_default() -> Result<Self> {
        Self::load(super::highlights_path()?)
    }

    /// Grava de volta no arquivo, de forma atômica.
    pub fn save(&self) -> Result<()> {
        let dtos: Vec<HighlightDto> = self.items.iter().map(Highlight::to_dto).collect();
        let json = serde_json::to_string_pretty(&dtos)?;
        crate::util::atomic_write(&self.path, json.as_bytes())?;
        Ok(())
    }

    /// Todas as marcações, na ordem de inserção.
    pub fn list(&self) -> &[Highlight] {
        &self.items
    }

    /// Adiciona uma marcação. Substitui uma marcação anterior com a **mesma
    /// referência** (atualiza cor/tag em vez de duplicar).
    pub fn add(&mut self, highlight: Highlight) {
        self.items.retain(|h| h.reference != highlight.reference);
        self.items.push(highlight);
    }

    /// Remove todas as marcações com a referência dada. Devolve quantas saíram.
    pub fn remove(&mut self, reference: &Reference) -> usize {
        let before = self.items.len();
        self.items.retain(|h| &h.reference != reference);
        before - self.items.len()
    }

    /// Marcações que cobrem um versículo específico (para exibir na leitura).
    pub fn covering(&self, book: u8, chapter: u16, verse: u16) -> Vec<&Highlight> {
        self.items
            .iter()
            .filter(|h| {
                h.reference.book == book
                    && h.reference.chapter == chapter
                    && h.reference.verses.contains(verse)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::VerseRange;

    fn hl(reference: Reference, color: &str, tag: Option<&str>) -> Highlight {
        Highlight {
            reference,
            color: color.to_string(),
            tag: tag.map(String::from),
        }
    }

    #[test]
    fn add_list_and_replace_same_reference() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = HighlightStore::load(dir.path().join("highlights.json")).unwrap();
        let r = parse_reference("John 3:16").unwrap();
        store.add(hl(r, "yellow", Some("salvação")));
        assert_eq!(store.list().len(), 1);
        // Mesma referência → substitui (não duplica).
        store.add(hl(r, "green", None));
        assert_eq!(store.list().len(), 1);
        assert_eq!(store.list()[0].color, "green");
    }

    #[test]
    fn file_roundtrip_is_human_readable() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("highlights.json");
        {
            let mut store = HighlightStore::load(&path).unwrap();
            store.add(hl(
                parse_reference("Gn 1.1-3").unwrap(),
                "blue",
                Some("criação"),
            ));
            store.add(hl(parse_reference("Sl 23").unwrap(), "yellow", None));
            store.save().unwrap();
        }
        // Conteúdo do arquivo usa referências legíveis.
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(raw.contains("\"ref\": \"Genesis 1:1-3\""), "{raw}");
        assert!(raw.contains("\"ref\": \"Psalms 23\""), "{raw}");

        // Recarrega idêntico.
        let store = HighlightStore::load(&path).unwrap();
        assert_eq!(store.list().len(), 2);
        assert_eq!(
            store.list()[0].reference,
            parse_reference("Gn 1.1-3").unwrap()
        );
        assert_eq!(store.list()[0].tag.as_deref(), Some("criação"));
    }

    #[test]
    fn covering_matches_single_range_and_whole_chapter() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = HighlightStore::load(dir.path().join("h.json")).unwrap();
        store.add(hl(parse_reference("John 3:16-17").unwrap(), "yellow", None));
        store.add(hl(parse_reference("Psalms 23").unwrap(), "green", None));

        // Intervalo cobre 16 e 17, não 18.
        assert_eq!(store.covering(43, 3, 16).len(), 1);
        assert_eq!(store.covering(43, 3, 17).len(), 1);
        assert_eq!(store.covering(43, 3, 18).len(), 0);
        // Capítulo inteiro cobre qualquer versículo do Salmo 23.
        assert_eq!(store.covering(19, 23, 6).len(), 1);
        assert_eq!(store.covering(19, 24, 1).len(), 0);
    }

    #[test]
    fn remove_returns_count() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = HighlightStore::load(dir.path().join("h.json")).unwrap();
        let r = parse_reference("John 3:16").unwrap();
        store.add(hl(r, "yellow", None));
        assert_eq!(store.remove(&r), 1);
        assert_eq!(store.remove(&r), 0);
        assert!(store.list().is_empty());
    }

    #[test]
    fn missing_file_is_empty_store() {
        let dir = tempfile::tempdir().unwrap();
        let store = HighlightStore::load(dir.path().join("absent.json")).unwrap();
        assert!(store.list().is_empty());
        // Single helper para silenciar unused no VerseRange import.
        assert!(matches!(
            parse_reference("Jo 3.16").unwrap().verses,
            VerseRange::Single(16)
        ));
    }
}
