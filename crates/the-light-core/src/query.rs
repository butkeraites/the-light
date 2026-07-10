//! Planejadores de SQL **puros** (ADR-0062): cada consulta do store é montada aqui
//! como um par `(sql, params)` de DADOS — a FONTE ÚNICA para o nativo (que executa via
//! `rusqlite`) e o web (que executa o MESMO `{sql, params}` via `wa-sqlite`/OPFS).
//!
//! `SqlParam` é um enum PRÓPRIO (nunca `rusqlite::types::Value`), então este módulo
//! compila em `ai-pure`/`wasm32` sem arrastar `rusqlite`; o mapeamento p/ `rusqlite`
//! vive num `From` gateado por `embedded` (o ÚNICO ponto que toca o driver).
//!
//! Os placeholders seguem o estilo de cada consulta original (`?1..?N` nas do
//! `source::embedded`, `?` nas do `search`) — os `params` saem na ordem posicional
//! `1..N`, então `params_from_iter` liga corretamente em ambos.

use crate::model::{Reference, VerseRange};

/// Marcador de início de termo casado (destaque da busca) — inserido pelo SQLite via
/// `highlight()`. Mora aqui (puro) e é re-exportado por `search` p/ compatibilidade.
pub const HL_START: &str = "\u{2}";
/// Marcador de fim de termo casado.
pub const HL_END: &str = "\u{3}";

/// Parâmetro de bind de uma consulta — enum PURO (wasm-safe), espelhado no web como
/// `SqlParam` da fronteira. Cobre os únicos tipos ligados pelo store: texto e i64.
#[derive(Debug, Clone, PartialEq)]
pub enum SqlParam {
    /// Texto (`?` ligado a uma `String`).
    Text(String),
    /// Inteiro de 64 bits (`?` ligado a um `i64`).
    Int(i64),
}

/// Clamp de `LIMIT` para `[1, i64::MAX]`: evita `LIMIT 0` (inútil) e o cast
/// `usize`→`i64` que transformaria valores enormes em `-1` (SQLite = "sem limite").
fn clamp_limit(limit: usize) -> i64 {
    limit.clamp(1, i64::MAX as usize) as i64
}

// ─────────────────────────────────────────────────────────────────────────────
// source::embedded — capítulo, contagem, traduções, presença.
// ─────────────────────────────────────────────────────────────────────────────

/// `SELECT` de traduções (estático, sem params).
pub const TRANSLATIONS_SELECT: &str =
    "SELECT id, abbrev, name, language, license, embeddable FROM translations ORDER BY language, id";

/// `SELECT` de presença de tradução (`?1` = id).
pub const HAS_TRANSLATION_SELECT: &str = "SELECT 1 FROM translations WHERE id = ?1";

/// `SELECT max(chapter)` de um livro numa tradução (`?1` = tradução, `?2` = livro).
pub const CHAPTER_COUNT_SELECT: &str =
    "SELECT max(chapter) FROM verses WHERE translation_id = ?1 AND book_number = ?2";

/// Plano da contagem de capítulos: `(sql, [translation, book])`.
pub fn chapter_count_plan(book: u8, translation: &str) -> (String, Vec<SqlParam>) {
    (
        CHAPTER_COUNT_SELECT.to_string(),
        vec![
            SqlParam::Text(translation.to_string()),
            SqlParam::Int(book as i64),
        ],
    )
}

/// Plano da presença de tradução: `(sql, [translation])`.
pub fn has_translation_plan(translation: &str) -> (String, Vec<SqlParam>) {
    (
        HAS_TRANSLATION_SELECT.to_string(),
        vec![SqlParam::Text(translation.to_string())],
    )
}

/// Plano das traduções: `(sql, [])`.
pub fn translations_plan() -> (String, Vec<SqlParam>) {
    (TRANSLATIONS_SELECT.to_string(), Vec::new())
}

/// Plano de uma passagem — cobre as três variantes de `VerseRange`. `?1` = tradução,
/// `?2` = livro, `?3` = capítulo; Single acrescenta `?4` = versículo; Range acrescenta
/// `?4..?5` = intervalo. Params na ordem posicional.
pub fn passage_plan(reference: &Reference, translation: &str) -> (String, Vec<SqlParam>) {
    let base = "SELECT verse, text FROM verses \
                WHERE translation_id = ?1 AND book_number = ?2 AND chapter = ?3";
    let mut params = vec![
        SqlParam::Text(translation.to_string()),
        SqlParam::Int(reference.book as i64),
        SqlParam::Int(reference.chapter as i64),
    ];
    let sql = match reference.verses {
        VerseRange::WholeChapter => format!("{base} ORDER BY verse"),
        VerseRange::Single(v) => {
            params.push(SqlParam::Int(v as i64));
            format!("{base} AND verse = ?4 ORDER BY verse")
        }
        VerseRange::Range { start, end } => {
            params.push(SqlParam::Int(start as i64));
            params.push(SqlParam::Int(end as i64));
            format!("{base} AND verse BETWEEN ?4 AND ?5 ORDER BY verse")
        }
    };
    (sql, params)
}

// ─────────────────────────────────────────────────────────────────────────────
// search — FTS5 (BM25), com destaque e clamp de limite.
// ─────────────────────────────────────────────────────────────────────────────

/// Constrói uma expressão FTS5 segura a partir da consulta do usuário: cada palavra
/// vira um termo entre aspas (escapando aspas internas), combinados com AND implícito.
/// `None` se não houver nenhum termo utilizável — evita injeção de sintaxe FTS5.
pub fn build_match_query(input: &str) -> Option<String> {
    let terms: Vec<String> = input
        .split_whitespace()
        .map(|w| format!("\"{}\"", w.replace('"', "\"\"")))
        .collect();
    if terms.is_empty() {
        None
    } else {
        Some(terms.join(" "))
    }
}

/// Plano da busca FTS5. `None` se a consulta não tiver termos (nenhum resultado).
/// Params na ordem: HL_START, HL_END, match_query, translation, [book?], limit.
pub fn search_plan(
    query: &str,
    translation: &str,
    book: Option<u8>,
    limit: usize,
) -> Option<(String, Vec<SqlParam>)> {
    let match_query = build_match_query(query)?;
    let mut sql = String::from(
        "SELECT v.book_number, v.chapter, v.verse, v.text, \
         highlight(verses_fts, 0, ?, ?) AS hl, bm25(verses_fts) AS score \
         FROM verses_fts JOIN verses v ON v.id = verses_fts.verse_id \
         WHERE verses_fts MATCH ? AND verses_fts.translation_id = ?",
    );
    let mut params = vec![
        SqlParam::Text(HL_START.to_string()),
        SqlParam::Text(HL_END.to_string()),
        SqlParam::Text(match_query),
        SqlParam::Text(translation.to_string()),
    ];
    if let Some(book) = book {
        sql.push_str(" AND v.book_number = ?");
        params.push(SqlParam::Int(book as i64));
    }
    sql.push_str(" ORDER BY score LIMIT ?");
    params.push(SqlParam::Int(clamp_limit(limit)));
    Some((sql, params))
}

// ─────────────────────────────────────────────────────────────────────────────
// xref — referências cruzadas por versículo (filtradas por votos, com clamp).
// ─────────────────────────────────────────────────────────────────────────────

/// `SELECT` de referências cruzadas (`?1..?5` = from_book, from_chapter, from_verse,
/// min_votes, limit).
pub const XREF_SELECT: &str = "SELECT to_book, to_chapter, to_verse_start, to_verse_end, votes \
     FROM cross_references \
     WHERE from_book = ?1 AND from_chapter = ?2 AND from_verse = ?3 AND votes >= ?4 \
     ORDER BY votes DESC, to_book, to_chapter, to_verse_start \
     LIMIT ?5";

/// Plano das referências cruzadas de um versículo: `(sql, [book, chapter, verse,
/// min_votes, limit])` (limit já com clamp).
pub fn xref_plan(
    book: u8,
    chapter: u16,
    verse: u16,
    min_votes: i64,
    limit: usize,
) -> (String, Vec<SqlParam>) {
    (
        XREF_SELECT.to_string(),
        vec![
            SqlParam::Int(book as i64),
            SqlParam::Int(chapter as i64),
            SqlParam::Int(verse as i64),
            SqlParam::Int(min_votes),
            SqlParam::Int(clamp_limit(limit)),
        ],
    )
}

/// Mapeamento p/ `rusqlite` — ÚNICO ponto que toca o driver (gateado por `embedded`).
#[cfg(feature = "embedded")]
impl From<SqlParam> for rusqlite::types::Value {
    fn from(p: SqlParam) -> Self {
        match p {
            SqlParam::Text(s) => rusqlite::types::Value::Text(s),
            SqlParam::Int(i) => rusqlite::types::Value::Integer(i),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_match_query_quotes_and_escapes() {
        assert_eq!(build_match_query("graça").as_deref(), Some("\"graça\""));
        assert_eq!(
            build_match_query("Deus amou").as_deref(),
            Some("\"Deus\" \"amou\"")
        );
        assert_eq!(build_match_query("a\"b").as_deref(), Some("\"a\"\"b\""));
        assert_eq!(build_match_query("   ").as_deref(), None);
    }

    #[test]
    fn passage_plan_covers_all_shapes() {
        let whole = passage_plan(&Reference::whole_chapter(43, 3), "kjv");
        assert!(whole.0.contains("ORDER BY verse") && !whole.0.contains("verse ="));
        assert_eq!(whole.1.len(), 3);

        let single = passage_plan(&Reference::single(43, 3, 16), "kjv");
        assert!(single.0.contains("AND verse = ?4"));
        assert_eq!(single.1.len(), 4);

        let range = passage_plan(
            &Reference {
                book: 43,
                chapter: 3,
                verses: VerseRange::Range { start: 16, end: 18 },
            },
            "kjv",
        );
        assert!(range.0.contains("BETWEEN ?4 AND ?5"));
        assert_eq!(range.1.len(), 5);
    }

    #[test]
    fn search_plan_none_for_empty_and_clamps_limit() {
        assert!(search_plan("   ", "kjv", None, 20).is_none());
        let (_, params) = search_plan("grace", "kjv", None, 0).unwrap();
        // limit clamp → 1 (último param).
        assert_eq!(params.last(), Some(&SqlParam::Int(1)));
        // book filter adiciona um param entre translation e limit.
        let (sql, params) = search_plan("grace", "kjv", Some(45), 5).unwrap();
        assert!(sql.contains("AND v.book_number = ?"));
        assert_eq!(params.len(), 6);
    }

    #[test]
    fn xref_plan_clamps_limit() {
        let (_, params) = xref_plan(43, 3, 16, 1, usize::MAX);
        assert_eq!(params.last(), Some(&SqlParam::Int(i64::MAX)));
    }
}
