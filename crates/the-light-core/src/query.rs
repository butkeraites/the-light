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

// ─────────────────────────────────────────────────────────────────────────────
// ai::lexicon — léxico verificado (agregado por Strong), interlinear e atribuições.
// Os SELECTs vivem aqui (fonte única nativo↔web); a agregação "primeiro não-nulo
// vence" e a ordenação por frequência seguem no consumidor (nativo `ai/lexicon.rs` e,
// no web, o shaper TS) — este módulo só entrega o `(sql, params)` de DADO.
// ─────────────────────────────────────────────────────────────────────────────

/// Strong **base**: remove as letras de desambiguação à direita (`"H7225G"` → `"H7225"`;
/// `"G0976"` termina em dígito → inalterado). Puro/compartilhado — a agregação do léxico
/// (por Strong base) e a verificação anti-alucinação (`ai::lexicon::verify`) dependem dele;
/// a fronteira do app o expõe ao web (fonte única, ADR-0062). Vivia em `ai::lexicon`.
pub fn base_strong(s: &str) -> String {
    s.trim()
        .trim_end_matches(|c: char| c.is_ascii_alphabetic())
        .to_string()
}

/// Resolve a lista de versículos abrangidos por uma passagem. `None` = capítulo inteiro
/// (sem filtro de versículo) quando não há números explícitos. Puro (só tipos de `model`);
/// movido de `ai::lexicon` (ADR-0062) para colocar junto dos planos de léxico. Números
/// explícitos vencem a referência; senão deriva de `VerseRange` (Single→`[v]`,
/// Range→`start..=end`, WholeChapter→`None`).
pub fn resolve_verses(reference: &Reference, verse_numbers: &[u16]) -> Option<Vec<u16>> {
    if !verse_numbers.is_empty() {
        return Some(verse_numbers.to_vec());
    }
    match reference.verses {
        VerseRange::Single(v) => Some(vec![v]),
        VerseRange::Range { start, end } => Some((start..=end).collect()),
        VerseRange::WholeChapter => None,
    }
}

/// `SELECT` base do **léxico verificado** de uma passagem (agregado adiante por Strong
/// base): tokens de `original_tokens` + glosa por `LEFT JOIN lexicon` (`COALESCE`
/// `gloss_pt`→`gloss`→token), filtrando por livro/capítulo e Strong não-vazio. `?1` =
/// livro, `?2` = capítulo. **Sem `ORDER BY`:** a ordem das linhas (rowid) é a que a
/// agregação "primeiro não-nulo vence" pressupõe (nativo e web).
pub const LEXICON_COLLECT_SELECT: &str = "SELECT t.strongs, t.lemma, t.translit, t.testament, \
     COALESCE(l.gloss_pt, l.gloss, t.gloss) AS gloss, t.source_id, l.source_id \
     FROM original_tokens t \
     LEFT JOIN lexicon l ON l.strongs = t.strongs \
     WHERE t.book_number = ?1 AND t.chapter = ?2 \
     AND t.strongs IS NOT NULL AND t.strongs <> ''";

/// Plano do léxico verificado de uma passagem: `(sql, [book, chapter, verse?])`. Com
/// `verse = Some(v)` acrescenta `AND t.verse = ?3` (um versículo); `None` = capítulo
/// inteiro (sem filtro). Params na ordem posicional.
pub fn lexicon_collect_plan(book: u8, chapter: u16, verse: Option<u16>) -> (String, Vec<SqlParam>) {
    let mut params = vec![SqlParam::Int(book as i64), SqlParam::Int(chapter as i64)];
    let sql = match verse {
        Some(v) => {
            params.push(SqlParam::Int(v as i64));
            format!("{LEXICON_COLLECT_SELECT} AND t.verse = ?3")
        }
        None => LEXICON_COLLECT_SELECT.to_string(),
    };
    (sql, params)
}

/// `SELECT` **interlinear** de UM versículo (tokens na ordem de leitura, **sem** agregar e
/// **sem** filtro de Strong — partículas incluídas). `LEFT JOIN lexicon` traz a glosa PT
/// (`COALESCE`). `?1..?3` = livro/capítulo/versículo; `ORDER BY t.word_index` fixa a ordem.
pub const INTERLINEAR_SELECT: &str =
    "SELECT t.surface, t.translit, t.lemma, t.strongs, t.morph_code, \
     COALESCE(l.gloss_pt, l.gloss, t.gloss) AS gloss, t.word_index, t.testament, t.source_id \
     FROM original_tokens t \
     LEFT JOIN lexicon l ON l.strongs = t.strongs \
     WHERE t.book_number = ?1 AND t.chapter = ?2 AND t.verse = ?3 \
     ORDER BY t.word_index";

/// Plano interlinear de um versículo: `(sql, [book, chapter, verse])`.
pub fn interlinear_plan(book: u8, chapter: u16, verse: u16) -> (String, Vec<SqlParam>) {
    (
        INTERLINEAR_SELECT.to_string(),
        vec![
            SqlParam::Int(book as i64),
            SqlParam::Int(chapter as i64),
            SqlParam::Int(verse as i64),
        ],
    )
}

/// `SELECT` da atribuição (verbatim, CC-BY) de uma fonte usada. `?1` = id da fonte.
pub const ATTRIBUTION_SELECT: &str = "SELECT attribution FROM scholarly_sources WHERE id = ?1";

/// Plano da atribuição de uma fonte: `(sql, [id])`. O consumidor deduplica (nativo:
/// `BTreeSet`/`HashSet`; web: shaper) preservando a ordem — o plano só entrega a query.
pub fn attributions_plan(id: &str) -> (String, Vec<SqlParam>) {
    (
        ATTRIBUTION_SELECT.to_string(),
        vec![SqlParam::Text(id.to_string())],
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

    #[test]
    fn base_strong_strips_disambiguation() {
        assert_eq!(base_strong("H7225G"), "H7225");
        assert_eq!(base_strong("G0976"), "G0976");
        assert_eq!(base_strong("H0853"), "H0853");
        // Espaços em volta são aparados antes (como no acervo).
        assert_eq!(base_strong(" H7225G "), "H7225");
    }

    #[test]
    fn resolve_verses_prefers_numbers_then_derives_from_reference() {
        // Números explícitos vencem a referência.
        assert_eq!(
            resolve_verses(&Reference::whole_chapter(1, 1), &[2, 4]),
            Some(vec![2, 4])
        );
        // Single → [v]; Range → start..=end; WholeChapter (sem números) → None.
        assert_eq!(
            resolve_verses(&Reference::single(1, 1, 3), &[]),
            Some(vec![3])
        );
        assert_eq!(
            resolve_verses(
                &Reference {
                    book: 1,
                    chapter: 1,
                    verses: VerseRange::Range { start: 2, end: 4 },
                },
                &[]
            ),
            Some(vec![2, 3, 4])
        );
        assert_eq!(resolve_verses(&Reference::whole_chapter(1, 1), &[]), None);
    }

    #[test]
    fn lexicon_collect_plan_adds_verse_filter_when_present() {
        let (sql, params) = lexicon_collect_plan(1, 1, None);
        assert!(sql.contains("FROM original_tokens") && !sql.contains("t.verse = ?3"));
        assert_eq!(params, vec![SqlParam::Int(1), SqlParam::Int(1)]);
        let (sql, params) = lexicon_collect_plan(1, 1, Some(5));
        assert!(sql.contains("AND t.verse = ?3"));
        assert_eq!(
            params,
            vec![SqlParam::Int(1), SqlParam::Int(1), SqlParam::Int(5)]
        );
    }

    #[test]
    fn interlinear_plan_orders_by_word_index() {
        let (sql, params) = interlinear_plan(43, 3, 16);
        assert!(sql.contains("ORDER BY t.word_index"));
        assert_eq!(
            params,
            vec![SqlParam::Int(43), SqlParam::Int(3), SqlParam::Int(16)]
        );
    }

    #[test]
    fn attributions_plan_binds_id() {
        let (sql, params) = attributions_plan("tahot");
        assert!(sql.contains("scholarly_sources"));
        assert_eq!(params, vec![SqlParam::Text("tahot".to_string())]);
    }
}
