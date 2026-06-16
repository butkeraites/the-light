//! Notas em Markdown, uma por versículo/intervalo, em `notes/`.
//!
//! Cada nota é um arquivo `.md` cujo nome codifica a referência em forma
//! canônica (inglês, com `_` no lugar de espaço e `.` no lugar de `:`), ex.:
//! `John_3.16.md`, `Genesis_1.1-3.md`, `Psalms_23.md`. O conteúdo é o Markdown
//! puro, editável à mão.

use std::path::PathBuf;

use super::Result;
use crate::model::{Lang, Reference};
use crate::reference::{format_reference, parse_reference};

/// Uma nota associada a uma referência.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    /// Referência da nota.
    pub reference: Reference,
    /// Corpo em Markdown.
    pub body: String,
}

/// Nome de arquivo (sem diretório) para uma referência.
fn slug(reference: &Reference) -> String {
    format!(
        "{}.md",
        format_reference(reference, Lang::En)
            .replace(' ', "_")
            .replace(':', ".")
    )
}

/// Recupera a referência a partir do nome de arquivo (`stem`, sem `.md`).
fn parse_slug(stem: &str) -> Option<Reference> {
    parse_reference(&stem.replace('_', " ")).ok()
}

/// Coleção de notas num diretório.
pub struct NoteStore {
    dir: PathBuf,
}

impl NoteStore {
    /// Cria um store ligado ao diretório dado.
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        NoteStore { dir: dir.into() }
    }

    /// Store no diretório padrão (`notes/`).
    pub fn open_default() -> Result<Self> {
        Ok(NoteStore::new(super::notes_dir()?))
    }

    /// Caminho do arquivo da nota para uma referência.
    pub fn path_for(&self, reference: &Reference) -> PathBuf {
        self.dir.join(slug(reference))
    }

    /// Lê a nota de uma referência (se existir).
    pub fn get(&self, reference: &Reference) -> Result<Option<Note>> {
        match std::fs::read_to_string(self.path_for(reference)) {
            Ok(body) => Ok(Some(Note {
                reference: *reference,
                body,
            })),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Grava (ou substitui) a nota de uma referência, de forma atômica.
    pub fn put(&self, reference: &Reference, body: &str) -> Result<()> {
        crate::util::atomic_write(&self.path_for(reference), body.as_bytes())?;
        Ok(())
    }

    /// Remove a nota de uma referência. Devolve `true` se havia uma.
    pub fn delete(&self, reference: &Reference) -> Result<bool> {
        match std::fs::remove_file(self.path_for(reference)) {
            Ok(()) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e.into()),
        }
    }

    /// Lista todas as notas, ordenadas por referência canônica.
    pub fn list(&self) -> Result<Vec<Note>> {
        let entries = match std::fs::read_dir(&self.dir) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e.into()),
        };
        let mut notes = Vec::new();
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("md") {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            let Some(reference) = parse_slug(stem) else {
                continue; // arquivo .md não-reconhecível: ignora
            };
            let body = std::fs::read_to_string(&path)?;
            notes.push(Note { reference, body });
        }
        notes.sort_by_key(|n| {
            (
                n.reference.book,
                n.reference.chapter,
                n.reference.verses.start().unwrap_or(0),
            )
        });
        Ok(notes)
    }

    /// Notas cujo intervalo cobre o versículo dado (para exibir na leitura).
    pub fn covering(&self, book: u8, chapter: u16, verse: u16) -> Result<Vec<Note>> {
        Ok(self
            .list()?
            .into_iter()
            .filter(|n| {
                n.reference.book == book
                    && n.reference.chapter == chapter
                    && n.reference.verses.contains(verse)
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(s: &str) -> Reference {
        parse_reference(s).unwrap()
    }

    #[test]
    fn slug_roundtrips_for_all_shapes() {
        for s in [
            "John 3:16",
            "Genesis 1:1-3",
            "Psalms 23",
            "1 Corinthians 13:4-7",
        ] {
            let reference = r(s);
            let name = slug(&reference);
            let stem = name.strip_suffix(".md").unwrap();
            assert_eq!(parse_slug(stem), Some(reference), "slug {name}");
        }
    }

    #[test]
    fn put_get_delete_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let store = NoteStore::new(dir.path());
        let reference = r("Jo 3.16");
        assert!(store.get(&reference).unwrap().is_none());

        store
            .put(&reference, "# Amor de Deus\n\nVersículo central.")
            .unwrap();
        let note = store.get(&reference).unwrap().unwrap();
        assert!(note.body.contains("Amor de Deus"));
        // Um arquivo .md por nota, com nome legível.
        assert!(dir.path().join("John_3.16.md").exists());

        assert!(store.delete(&reference).unwrap());
        assert!(!store.delete(&reference).unwrap());
        assert!(store.get(&reference).unwrap().is_none());
    }

    #[test]
    fn list_is_sorted_and_skips_non_notes() {
        let dir = tempfile::tempdir().unwrap();
        let store = NoteStore::new(dir.path());
        store.put(&r("John 3:16"), "a").unwrap();
        store.put(&r("Gn 1.1"), "b").unwrap();
        std::fs::write(dir.path().join("LEIA-ME.txt"), "ignore").unwrap();
        std::fs::write(dir.path().join("arquivo-qualquer.md"), "ignore").unwrap();

        let notes = store.list().unwrap();
        assert_eq!(notes.len(), 2, "{notes:?}");
        // Gênesis (1) antes de João (43).
        assert_eq!(notes[0].reference, r("Gn 1.1"));
        assert_eq!(notes[1].reference, r("John 3:16"));
    }

    #[test]
    fn covering_matches_range_and_whole_chapter() {
        let dir = tempfile::tempdir().unwrap();
        let store = NoteStore::new(dir.path());
        store.put(&r("John 3:16-17"), "intervalo").unwrap();
        store.put(&r("Psalms 23"), "capítulo").unwrap();
        assert_eq!(store.covering(43, 3, 16).unwrap().len(), 1);
        assert_eq!(store.covering(43, 3, 18).unwrap().len(), 0);
        assert_eq!(store.covering(19, 23, 4).unwrap().len(), 1);
    }

    #[test]
    fn missing_dir_lists_empty() {
        let dir = tempfile::tempdir().unwrap();
        let store = NoteStore::new(dir.path().join("does-not-exist"));
        assert!(store.list().unwrap().is_empty());
    }
}
