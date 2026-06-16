//! [`EmbeddedSource`] — fonte que lê do banco SQLite local (versões livres).

use std::str::FromStr;

use rusqlite::{params, Connection};

use super::{BibleSource, Result, SourceError};
use crate::model::{
    Lang, License, Passage, Reference, Translation, TranslationId, Verse, VerseRange,
};
use crate::store::Store;

/// Fonte de texto bíblico embarcado, lendo de uma conexão SQLite.
pub struct EmbeddedSource<'a> {
    conn: &'a Connection,
}

impl<'a> EmbeddedSource<'a> {
    /// Cria a fonte a partir de um [`Store`] já aberto.
    pub fn new(store: &'a Store) -> Self {
        EmbeddedSource { conn: store.conn() }
    }

    /// Cria a fonte a partir de uma conexão SQLite já migrada.
    pub fn from_conn(conn: &'a Connection) -> Self {
        EmbeddedSource { conn }
    }
}

impl BibleSource for EmbeddedSource<'_> {
    fn translations(&self) -> Result<Vec<Translation>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, abbrev, name, language, license, embeddable \
             FROM translations ORDER BY language, id",
        )?;
        let rows = stmt.query_map([], |row| {
            let id: String = row.get(0)?;
            let abbrev: String = row.get(1)?;
            let name: String = row.get(2)?;
            let language: String = row.get(3)?;
            let license: String = row.get(4)?;
            let embeddable: i64 = row.get(5)?;
            Ok((id, abbrev, name, language, license, embeddable))
        })?;

        let mut out = Vec::new();
        for row in rows {
            let (id, abbrev, name, language, license, embeddable) = row?;
            out.push(Translation {
                id: TranslationId::new(id),
                abbrev,
                name,
                language: Lang::from_str(&language).unwrap_or(Lang::En),
                license: License::from(license.as_str()),
                embeddable: embeddable != 0,
            });
        }
        Ok(out)
    }

    fn has_translation(&self, t: &TranslationId) -> Result<bool> {
        let found: Option<i64> = self
            .conn
            .query_row(
                "SELECT 1 FROM translations WHERE id = ?1",
                params![t.as_str()],
                |r| r.get(0),
            )
            .ok();
        Ok(found.is_some())
    }

    fn passage(&self, r: &Reference, t: &TranslationId) -> Result<Passage> {
        if !self.has_translation(t)? {
            return Err(SourceError::UnknownTranslation(t.to_string()));
        }

        let (sql, lo, hi) = match r.verses {
            VerseRange::WholeChapter => (
                "SELECT verse, text FROM verses \
                 WHERE translation_id = ?1 AND book_number = ?2 AND chapter = ?3 \
                 ORDER BY verse",
                None,
                None,
            ),
            VerseRange::Single(v) => (
                "SELECT verse, text FROM verses \
                 WHERE translation_id = ?1 AND book_number = ?2 AND chapter = ?3 AND verse = ?4 \
                 ORDER BY verse",
                Some(v),
                None,
            ),
            VerseRange::Range { start, end } => (
                "SELECT verse, text FROM verses \
                 WHERE translation_id = ?1 AND book_number = ?2 AND chapter = ?3 \
                 AND verse BETWEEN ?4 AND ?5 ORDER BY verse",
                Some(start),
                Some(end),
            ),
        };

        let mut stmt = self.conn.prepare(sql)?;
        let book = r.book as i64;
        let chapter = r.chapter as i64;

        let map_row = |row: &rusqlite::Row<'_>| -> rusqlite::Result<(u16, String)> {
            Ok((row.get::<_, i64>(0)? as u16, row.get::<_, String>(1)?))
        };

        let rows = match (lo, hi) {
            (None, None) => stmt.query_map(params![t.as_str(), book, chapter], map_row)?,
            (Some(v), None) => {
                stmt.query_map(params![t.as_str(), book, chapter, v as i64], map_row)?
            }
            (Some(a), Some(b)) => stmt.query_map(
                params![t.as_str(), book, chapter, a as i64, b as i64],
                map_row,
            )?,
            // `hi` sem `lo` não é produzido por nenhuma variante de VerseRange.
            (None, Some(_)) => unreachable!("limite superior sem inferior"),
        };

        let mut verses = Vec::new();
        for row in rows {
            let (verse, text) = row?;
            verses.push(Verse {
                reference: Reference::single(r.book, r.chapter, verse),
                text,
                translation: t.clone(),
            });
        }

        Ok(Passage {
            reference: *r,
            verses,
        })
    }

    fn is_embeddable(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reference::parse_reference;

    fn seeded_store() -> Store {
        let store = Store::open_in_memory().unwrap();
        let conn = store.conn();
        conn.execute(
            "INSERT INTO translations(id,abbrev,name,language,license,embeddable) \
             VALUES ('kjv','KJV','King James Version','en','public-domain',1)",
            [],
        )
        .unwrap();
        // João (43) capítulo 3, versículos 16 e 17.
        for (v, text) in [
            (16, "For God so loved the world"),
            (17, "For God sent not his Son to condemn the world"),
        ] {
            conn.execute(
                "INSERT INTO verses(translation_id,book_number,chapter,verse,text) \
                 VALUES ('kjv',43,3,?1,?2)",
                params![v as i64, text],
            )
            .unwrap();
        }
        store
    }

    #[test]
    fn reads_single_verse() {
        let store = seeded_store();
        let src = EmbeddedSource::new(&store);
        let p = src
            .passage(&parse_reference("John 3:16").unwrap(), &"kjv".into())
            .unwrap();
        assert_eq!(p.verses.len(), 1);
        assert_eq!(p.verses[0].text, "For God so loved the world");
        assert_eq!(p.verses[0].reference.verses, VerseRange::Single(16));
    }

    #[test]
    fn reads_range() {
        let store = seeded_store();
        let src = EmbeddedSource::new(&store);
        let p = src
            .passage(&parse_reference("John 3:16-17").unwrap(), &"kjv".into())
            .unwrap();
        assert_eq!(p.verses.len(), 2);
        assert_eq!(p.verses[1].reference.verses, VerseRange::Single(17));
    }

    #[test]
    fn empty_for_missing_chapter() {
        let store = seeded_store();
        let src = EmbeddedSource::new(&store);
        let p = src
            .passage(&parse_reference("John 99:1").unwrap(), &"kjv".into())
            .unwrap();
        assert!(p.is_empty());
    }

    #[test]
    fn unknown_translation_errors() {
        let store = seeded_store();
        let src = EmbeddedSource::new(&store);
        let err = src.passage(&parse_reference("John 3:16").unwrap(), &"nope".into());
        assert!(matches!(err, Err(SourceError::UnknownTranslation(_))));
    }

    #[test]
    fn lists_translations() {
        let store = seeded_store();
        let src = EmbeddedSource::new(&store);
        let ts = src.translations().unwrap();
        assert_eq!(ts.len(), 1);
        assert_eq!(ts[0].language, Lang::En);
        assert!(ts[0].license.is_embeddable());
    }
}
