//! Busca full-text sobre o índice FTS5 dos versículos.
//!
//! A busca é acento-insensível (o índice usa `remove_diacritics 2`), então
//! `graca` casa `graça`. Múltiplas palavras são combinadas com AND.

use rusqlite::Connection;

use crate::model::{Reference, SearchHit, TranslationId};
// Superfície de query PURA (ADR-0062): destaque, builder FTS e o plano de busca vivem
// em `crate::query` (fonte única nativo↔web); re-exportados aqui p/ compatibilidade.
pub use crate::query::{build_match_query, HL_END, HL_START};

/// Limite padrão de resultados.
pub const DEFAULT_LIMIT: usize = 20;

/// Opções de uma busca.
#[derive(Debug, Clone)]
pub struct SearchOptions {
    /// Tradução onde buscar.
    pub translation: TranslationId,
    /// Filtro opcional por número canônico de livro.
    pub book: Option<u8>,
    /// Máximo de resultados.
    pub limit: usize,
}

impl SearchOptions {
    /// Cria opções com `limit` padrão e sem filtro de livro.
    pub fn new(translation: TranslationId) -> Self {
        SearchOptions {
            translation,
            book: None,
            limit: DEFAULT_LIMIT,
        }
    }
}

/// Executa a busca, devolvendo os melhores resultados por relevância (BM25).
pub fn search(
    conn: &Connection,
    query: &str,
    opts: &SearchOptions,
) -> rusqlite::Result<Vec<SearchHit>> {
    // SQL + params + builder FTS + clamp vêm do plano puro (fonte única nativo↔web).
    let Some((sql, params)) =
        crate::query::search_plan(query, opts.translation.as_str(), opts.book, opts.limit)
    else {
        return Ok(Vec::new());
    };

    let mut stmt = conn.prepare(&sql)?;
    let tid = opts.translation.clone();
    let rows = stmt.query_map(
        rusqlite::params_from_iter(params.into_iter().map(rusqlite::types::Value::from)),
        |row| {
            let book: i64 = row.get(0)?;
            let chapter: i64 = row.get(1)?;
            let verse: i64 = row.get(2)?;
            let text: String = row.get(3)?;
            let highlighted: String = row.get(4)?;
            let score: f64 = row.get(5)?;
            Ok(SearchHit {
                reference: Reference::single(book as u8, chapter as u16, verse as u16),
                translation: tid.clone(),
                text,
                highlighted,
                score,
            })
        },
    )?;

    rows.collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;

    #[test]
    fn build_match_query_quotes_terms() {
        assert_eq!(build_match_query("graça").as_deref(), Some("\"graça\""));
        assert_eq!(
            build_match_query("Deus amou").as_deref(),
            Some("\"Deus\" \"amou\"")
        );
        assert_eq!(build_match_query("   ").as_deref(), None);
        assert_eq!(build_match_query("").as_deref(), None);
    }

    #[test]
    fn build_match_query_escapes_quotes_and_operators() {
        // Aspas internas são escapadas (FTS5: duplica a aspa).
        assert_eq!(build_match_query("a\"b").as_deref(), Some("\"a\"\"b\""));
        // Operadores FTS5 (-, *, :, NEAR) ficam literais dentro das aspas.
        assert_eq!(
            build_match_query("foo-bar OR baz").as_deref(),
            Some("\"foo-bar\" \"OR\" \"baz\"")
        );
    }

    fn seeded() -> Store {
        let store = Store::open_in_memory().unwrap();
        let conn = store.conn();
        conn.execute(
            "INSERT INTO translations(id,abbrev,name,language,license,embeddable) \
             VALUES ('alm','ALM','Almeida','pt','public-domain',1)",
            [],
        )
        .unwrap();
        // (book, ch, v, text)
        let verses = [
            (45, 3, 24, "sendo justificados gratuitamente pela sua graça"), // Romanos 3:24
            (49, 2, 8, "Porque pela graça sois salvos, por meio da fé"),    // Efésios 2:8
            (43, 1, 14, "cheio de graça e de verdade"),                     // João 1:14
            (1, 1, 1, "No princípio criou Deus os céus e a terra"),         // Gênesis 1:1
        ];
        for (i, (b, c, v, t)) in verses.iter().enumerate() {
            conn.execute(
                "INSERT INTO verses(id,translation_id,book_number,chapter,verse,text) \
                 VALUES (?1,'alm',?2,?3,?4,?5)",
                rusqlite::params![(i + 1) as i64, *b as i64, *c as i64, *v as i64, t],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO verses_fts(text, translation_id, verse_id) VALUES (?1,'alm',?2)",
                rusqlite::params![t, (i + 1) as i64],
            )
            .unwrap();
        }
        store
    }

    #[test]
    fn search_finds_accented_term_without_accent() {
        let store = seeded();
        let opts = SearchOptions::new("alm".into());
        let hits = search(store.conn(), "graca", &opts).unwrap();
        // 3 versículos contêm "graça".
        assert_eq!(hits.len(), 3, "hits: {hits:#?}");
        assert!(hits.iter().all(|h| h.text.contains("graça")));
        // Destaque envolve o termo casado.
        assert!(hits.iter().any(|h| h.highlighted.contains(HL_START)));
    }

    #[test]
    fn search_with_accent_also_matches() {
        let store = seeded();
        let opts = SearchOptions::new("alm".into());
        assert_eq!(search(store.conn(), "graça", &opts).unwrap().len(), 3);
    }

    #[test]
    fn search_book_filter() {
        let store = seeded();
        let mut opts = SearchOptions::new("alm".into());
        opts.book = Some(45); // só Romanos
        let hits = search(store.conn(), "graca", &opts).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].reference.book, 45);
    }

    #[test]
    fn search_respects_limit() {
        let store = seeded();
        let mut opts = SearchOptions::new("alm".into());
        opts.limit = 2;
        assert_eq!(search(store.conn(), "graca", &opts).unwrap().len(), 2);
    }

    #[test]
    fn search_and_semantics_for_multiple_words() {
        let store = seeded();
        let opts = SearchOptions::new("alm".into());
        // "Deus céus" só casa Gênesis 1:1 (ambas as palavras).
        let hits = search(store.conn(), "Deus ceus", &opts).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].reference.book, 1);
    }

    #[test]
    fn search_limit_zero_is_clamped_to_one() {
        let store = seeded();
        let mut opts = SearchOptions::new("alm".into());
        opts.limit = 0; // clamp → 1 (não 0, nem "sem limite")
        assert_eq!(search(store.conn(), "graca", &opts).unwrap().len(), 1);
    }

    #[test]
    fn search_huge_limit_does_not_wrap_to_unlimited() {
        let store = seeded();
        let mut opts = SearchOptions::new("alm".into());
        opts.limit = usize::MAX; // não deve virar LIMIT -1 (sem limite)
                                 // Há 3 versículos com "graça"; o cap não os esconde.
        assert_eq!(search(store.conn(), "graca", &opts).unwrap().len(), 3);
    }

    #[test]
    fn search_empty_query_returns_nothing() {
        let store = seeded();
        let opts = SearchOptions::new("alm".into());
        assert!(search(store.conn(), "   ", &opts).unwrap().is_empty());
    }
}
