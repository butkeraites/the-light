//! Referências cruzadas (Treasury of Scripture Knowledge / OpenBible.info).
//!
//! Os dados são importados para a tabela `cross_references` (ver `xtask`). A
//! atribuição CC-BY ao OpenBible.info é obrigatória — ver `DATA_SOURCES.md`.

use rusqlite::{params, Connection};

use crate::model::{Lang, Reference, VerseRange};
use crate::reference::format_reference;

/// Limiar padrão de votos: oculta referências disputadas (votos negativos).
pub const DEFAULT_MIN_VOTES: i64 = 1;
/// Limite padrão de resultados.
pub const DEFAULT_LIMIT: usize = 20;

/// Uma referência cruzada de destino, com seu número de votos da comunidade.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrossRef {
    /// Versículo (ou intervalo) relacionado.
    pub reference: Reference,
    /// Votos da comunidade (maior = mais relevante; negativos = disputados).
    pub votes: i64,
}

/// Lista as referências cruzadas de um versículo, das mais votadas para as menos,
/// filtrando por `min_votes`.
pub fn for_verse(
    conn: &Connection,
    book: u8,
    chapter: u16,
    verse: u16,
    min_votes: i64,
    limit: usize,
) -> rusqlite::Result<Vec<CrossRef>> {
    let limit = limit.clamp(1, i64::MAX as usize) as i64;
    let mut stmt = conn.prepare(
        "SELECT to_book, to_chapter, to_verse_start, to_verse_end, votes \
         FROM cross_references \
         WHERE from_book = ?1 AND from_chapter = ?2 AND from_verse = ?3 AND votes >= ?4 \
         ORDER BY votes DESC, to_book, to_chapter, to_verse_start \
         LIMIT ?5",
    )?;
    let rows = stmt.query_map(
        params![book as i64, chapter as i64, verse as i64, min_votes, limit],
        |row| {
            let to_book: i64 = row.get(0)?;
            let to_chapter: i64 = row.get(1)?;
            let start: i64 = row.get(2)?;
            let end: i64 = row.get(3)?;
            let votes: i64 = row.get(4)?;
            let verses = if start >= end {
                VerseRange::Single(start as u16)
            } else {
                VerseRange::Range {
                    start: start as u16,
                    end: end as u16,
                }
            };
            Ok(CrossRef {
                reference: Reference {
                    book: to_book as u8,
                    chapter: to_chapter as u16,
                    verses,
                },
                votes,
            })
        },
    )?;
    rows.collect()
}

/// Rótulos de referências cruzadas locais agregados por **toda a passagem** (RAG
/// leve): consulta cada versículo de `verse_numbers`, deduplica por referência
/// (mantendo o maior nº de votos), ordena por votos e limita o total. Se
/// `verse_numbers` vier vazio, ancora no versículo inicial de `reference`. Melhor
/// esforço — ignora erros de consulta por versículo.
pub fn passage_labels(
    conn: &Connection,
    reference: &Reference,
    verse_numbers: &[u16],
    lang: Lang,
    limit: usize,
) -> Vec<String> {
    use std::collections::HashMap;

    // Salvaguarda: sem números (ex.: capítulo inteiro), ancora no início da ref.
    let fallback;
    let verses: &[u16] = if verse_numbers.is_empty() {
        fallback = [match reference.verses {
            VerseRange::Single(v) => v,
            VerseRange::Range { start, .. } => start,
            VerseRange::WholeChapter => 1,
        }];
        &fallback
    } else {
        verse_numbers
    };

    let per = limit.max(1);
    let mut best: HashMap<Reference, i64> = HashMap::new();
    for &v in verses {
        if let Ok(hits) = for_verse(
            conn,
            reference.book,
            reference.chapter,
            v,
            DEFAULT_MIN_VOTES,
            per,
        ) {
            for h in hits {
                best.entry(h.reference)
                    .and_modify(|votes| *votes = (*votes).max(h.votes))
                    .or_insert(h.votes);
            }
        }
    }

    let mut all: Vec<(Reference, i64)> = best.into_iter().collect();
    all.sort_by(|a, b| {
        b.1.cmp(&a.1)
            .then_with(|| a.0.book.cmp(&b.0.book))
            .then_with(|| a.0.chapter.cmp(&b.0.chapter))
    });
    all.into_iter()
        .take(limit)
        .map(|(r, _)| format_reference(&r, lang))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;

    fn seeded() -> Store {
        let store = Store::open_in_memory().unwrap();
        let conn = store.conn();
        // from Romanos 3:23 (45,3,23) → Romanos 6:23, Romanos 3:9 (disputada), Gn 1:1 range
        let rows = [
            // from_b, from_c, from_v, to_b, to_c, to_vs, to_ve, votes
            (45, 3, 23, 45, 6, 23, 23, 50),
            (45, 3, 23, 45, 3, 9, 9, -5), // disputada (voto negativo)
            (45, 3, 23, 1, 1, 1, 3, 10),  // intervalo
        ];
        for (fb, fc, fv, tb, tc, ts, te, votes) in rows {
            conn.execute(
                "INSERT INTO cross_references \
                 (from_book,from_chapter,from_verse,to_book,to_chapter,to_verse_start,to_verse_end,votes) \
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
                params![fb, fc, fv, tb, tc, ts, te, votes],
            )
            .unwrap();
        }
        store
    }

    #[test]
    fn lists_by_votes_and_hides_disputed() {
        let store = seeded();
        let hits = for_verse(store.conn(), 45, 3, 23, DEFAULT_MIN_VOTES, DEFAULT_LIMIT).unwrap();
        // A disputada (votos -5) fica de fora; sobram 2, mais votada primeiro.
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].reference, Reference::single(45, 6, 23));
        assert_eq!(hits[0].votes, 50);
        // O intervalo vira Range.
        assert_eq!(
            hits[1].reference.verses,
            VerseRange::Range { start: 1, end: 3 }
        );
    }

    #[test]
    fn lower_threshold_includes_disputed() {
        let store = seeded();
        let hits = for_verse(store.conn(), 45, 3, 23, -100, DEFAULT_LIMIT).unwrap();
        assert_eq!(hits.len(), 3);
    }

    #[test]
    fn limit_caps_results() {
        let store = seeded();
        let hits = for_verse(store.conn(), 45, 3, 23, -100, 1).unwrap();
        assert_eq!(hits.len(), 1);
    }

    #[test]
    fn no_xrefs_for_unknown_verse() {
        let store = seeded();
        assert!(
            for_verse(store.conn(), 1, 1, 1, DEFAULT_MIN_VOTES, DEFAULT_LIMIT)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn passage_labels_aggregate_dedupe_and_format() {
        let store = seeded();
        let reference = Reference::whole_chapter(45, 3);
        // Agrega o v.23 (e um v.99 sem xref): só os não-disputados, ordenados por
        // votos, formatados no idioma. O intervalo (1.1-3) vira "Genesis 1:1-3".
        let labels = passage_labels(store.conn(), &reference, &[23, 99], Lang::En, 20);
        assert_eq!(labels, vec!["Romans 6:23", "Genesis 1:1-3"]);
        // PT usa o separador "." e nomes em português.
        let pt = passage_labels(store.conn(), &reference, &[23], Lang::Pt, 20);
        assert_eq!(pt[0], "Romanos 6.23");
    }

    #[test]
    fn passage_labels_empty_numbers_fall_back_to_reference() {
        let store = seeded();
        // Sem números: ancora no versículo inicial da referência (v.23).
        let labels = passage_labels(
            store.conn(),
            &Reference::single(45, 3, 23),
            &[],
            Lang::En,
            20,
        );
        assert!(labels.contains(&"Romans 6:23".to_string()), "{labels:?}");
    }

    #[test]
    fn passage_labels_limit_caps_total() {
        let store = seeded();
        let reference = Reference::whole_chapter(45, 3);
        let labels = passage_labels(store.conn(), &reference, &[23], Lang::En, 1);
        assert_eq!(labels.len(), 1);
        assert_eq!(labels[0], "Romans 6:23"); // o mais votado
    }
}
