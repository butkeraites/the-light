//! Recuperação de **dados léxicos verificados** (línguas originais + número de
//! Strong + glosa) de uma passagem, a partir do acervo local (tabelas v2).
//!
//! Espelha [`crate::xref::passage_labels`]: função pura de banco que alimenta o
//! prompt com **fatos que o modelo deve CITAR** (e jamais inventar). A glosa vem
//! do léxico (TBESH/TBESG, CC-BY) com recuo para a glosa do próprio token.
//!
//! Como TAHOT/TAGNT usam versificação inglesa (que casa com as traduções
//! embarcadas), a busca é por referência canônica direta — sem mapa de
//! versificação no caminho comum (ver `DATA_SOURCES.md` §1.4).

use std::collections::HashSet;
// `BTreeMap`/`BTreeSet` só são usados na recuperação SQLite (`embedded`).
#[cfg(feature = "embedded")]
use std::collections::{BTreeMap, BTreeSet};

#[cfg(feature = "embedded")]
use rusqlite::{params, Connection};

use crate::model::Lang;
// `Reference`/`VerseRange` só entram na recuperação SQLite (`embedded`); os tipos
// e a verificação anti-alucinação são puros.
#[cfg(feature = "embedded")]
use crate::model::{Reference, VerseRange};

/// Sentinela quando não há dados léxicos para a passagem. O prompt instrui o
/// modelo a **declarar que não há base** em vez de inventar (anti-alucinação).
pub const EMPTY_SENTINEL: &str = "(nenhum dado léxico verificado disponível para esta passagem)";

/// Uma entrada léxica verificada, agregada por número de Strong **base**.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexicalEntry {
    /// Número de Strong base (ex.: "H7225") — a chave de citação `[V:H7225]`.
    pub strongs: String,
    /// Lema na língua original.
    pub lemma: Option<String>,
    /// Transliteração.
    pub translit: Option<String>,
    /// Glosa breve (do léxico, com recuo para a glosa do token).
    pub gloss: Option<String>,
    /// Ocorrências do termo na passagem.
    pub occurrences: u32,
    /// Testamento ("OT" hebraico | "NT" grego).
    pub testament: String,
}

/// Dados léxicos verificados de uma passagem + fontes (atribuição obrigatória).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VerifiedLexicon {
    /// Entradas por Strong base, das mais frequentes para as menos.
    pub entries: Vec<LexicalEntry>,
    /// Atribuições das fontes usadas (gravadas em `scholarly_sources`).
    pub sources: Vec<String>,
}

impl VerifiedLexicon {
    /// `true` se não há nenhuma entrada (acervo base / passagem sem cobertura).
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Conjunto dos números de Strong base presentes — usado por [`verify`].
    pub fn strong_set(&self) -> HashSet<String> {
        self.entries.iter().map(|e| e.strongs.clone()).collect()
    }
}

/// Strong base: remove letras de desambiguação à direita ("H7225G" → "H7225").
fn base_strong(s: &str) -> String {
    s.trim()
        .trim_end_matches(|c: char| c.is_ascii_alphabetic())
        .to_string()
}

#[cfg(feature = "embedded")]
#[derive(Default)]
struct Agg {
    lemma: Option<String>,
    translit: Option<String>,
    gloss: Option<String>,
    testament: String,
    occ: u32,
}

/// Resolve a lista de versículos da passagem. `None` = capítulo inteiro (sem
/// filtro de versículo) quando não há números explícitos.
#[cfg(feature = "embedded")]
fn resolve_verses(reference: &Reference, verse_numbers: &[u16]) -> Option<Vec<u16>> {
    if !verse_numbers.is_empty() {
        return Some(verse_numbers.to_vec());
    }
    match reference.verses {
        VerseRange::Single(v) => Some(vec![v]),
        VerseRange::Range { start, end } => Some((start..=end).collect()),
        VerseRange::WholeChapter => None,
    }
}

/// Acumula os tokens de uma passagem (um versículo, ou o capítulo todo se
/// `verse = None`) no agregador por Strong base.
#[cfg(feature = "embedded")]
fn collect(
    conn: &Connection,
    book: u8,
    chapter: u16,
    verse: Option<u16>,
    by_base: &mut BTreeMap<String, Agg>,
    sources: &mut BTreeSet<String>,
) -> rusqlite::Result<()> {
    let base_sql = "SELECT t.strongs, t.lemma, t.translit, t.testament, \
                    COALESCE(l.gloss_pt, l.gloss, t.gloss) AS gloss, t.source_id, l.source_id \
                    FROM original_tokens t \
                    LEFT JOIN lexicon l ON l.strongs = t.strongs \
                    WHERE t.book_number = ?1 AND t.chapter = ?2 \
                    AND t.strongs IS NOT NULL AND t.strongs <> ''";
    let mut handle = |row: &rusqlite::Row| -> rusqlite::Result<()> {
        let strongs: String = row.get(0)?;
        let lemma: Option<String> = row.get(1)?;
        let translit: Option<String> = row.get(2)?;
        let testament: String = row.get(3)?;
        let gloss: Option<String> = row.get(4)?;
        let token_src: Option<String> = row.get(5)?;
        let lex_src: Option<String> = row.get(6)?;
        let key = base_strong(&strongs);
        let agg = by_base.entry(key).or_default();
        agg.occ += 1;
        if agg.lemma.is_none() {
            agg.lemma = lemma;
        }
        if agg.translit.is_none() {
            agg.translit = translit;
        }
        if agg.gloss.is_none() {
            agg.gloss = gloss;
        }
        if agg.testament.is_empty() {
            agg.testament = testament;
        }
        if let Some(s) = token_src {
            sources.insert(s);
        }
        if let Some(s) = lex_src {
            sources.insert(s);
        }
        Ok(())
    };

    match verse {
        Some(v) => {
            let mut stmt = conn.prepare(&format!("{base_sql} AND t.verse = ?3"))?;
            let mut rows = stmt.query(params![book as i64, chapter as i64, v as i64])?;
            while let Some(r) = rows.next()? {
                handle(r)?;
            }
        }
        None => {
            let mut stmt = conn.prepare(base_sql)?;
            let mut rows = stmt.query(params![book as i64, chapter as i64])?;
            while let Some(r) = rows.next()? {
                handle(r)?;
            }
        }
    }
    Ok(())
}

/// Recupera os dados léxicos verificados de uma passagem, agregados por Strong
/// base, ordenados por frequência (desc) e limitados a `limit`. Best-effort:
/// num acervo base (sem `import-scholarly`) devolve vazio — caso offline normal.
///
/// Lê do banco SQLite (`rusqlite`) → só no caminho `embedded`. No web o léxico é
/// recuperado pela infra TS (precedente ADR-0011); os tipos/verificação abaixo
/// são compartilhados (puros).
#[cfg(feature = "embedded")]
pub fn verified_lexicon(
    conn: &Connection,
    reference: &Reference,
    verse_numbers: &[u16],
    _lang: Lang,
    limit: usize,
) -> VerifiedLexicon {
    let mut by_base: BTreeMap<String, Agg> = BTreeMap::new();
    let mut source_ids: BTreeSet<String> = BTreeSet::new();

    let result = (|| -> rusqlite::Result<()> {
        match resolve_verses(reference, verse_numbers) {
            Some(verses) => {
                for v in verses {
                    collect(
                        conn,
                        reference.book,
                        reference.chapter,
                        Some(v),
                        &mut by_base,
                        &mut source_ids,
                    )?;
                }
            }
            None => collect(
                conn,
                reference.book,
                reference.chapter,
                None,
                &mut by_base,
                &mut source_ids,
            )?,
        }
        Ok(())
    })();
    if result.is_err() {
        return VerifiedLexicon::default();
    }

    let mut entries: Vec<LexicalEntry> = by_base
        .into_iter()
        .map(|(strongs, a)| LexicalEntry {
            strongs,
            lemma: a.lemma,
            translit: a.translit,
            gloss: a.gloss,
            occurrences: a.occ,
            testament: a.testament,
        })
        .collect();
    // Mais frequentes primeiro; desempate estável pelo número de Strong.
    entries.sort_by(|a, b| {
        b.occurrences
            .cmp(&a.occurrences)
            .then_with(|| a.strongs.cmp(&b.strongs))
    });
    entries.truncate(limit);

    let sources = attributions_for(conn, &source_ids);
    VerifiedLexicon { entries, sources }
}

/// Busca as atribuições (verbatim) das fontes usadas.
#[cfg(feature = "embedded")]
fn attributions_for(conn: &Connection, ids: &BTreeSet<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for id in ids {
        if let Ok(attr) = conn.query_row(
            "SELECT attribution FROM scholarly_sources WHERE id = ?1",
            params![id],
            |r| r.get::<_, String>(0),
        ) {
            if seen.insert(attr.clone()) {
                out.push(attr);
            }
        }
    }
    out
}

/// Monta o **bloco de dados léxicos verificados** para o prompt. Vazio →
/// devolve o [`EMPTY_SENTINEL`] (mantém o modelo honesto).
pub fn format_verified_block(vl: &VerifiedLexicon, _lang: Lang) -> String {
    if vl.entries.is_empty() {
        return EMPTY_SENTINEL.to_string();
    }
    let mut out = String::from(
        "DADOS LÉXICOS VERIFICADOS (acervo local — cite o Strong com a marca [V:NÚMERO], \
         ex.: [V:H7225]; use SOMENTE estes números e sentidos; NÃO invente):\n",
    );
    for e in &vl.entries {
        let lemma = e.lemma.as_deref().unwrap_or("");
        let translit = e
            .translit
            .as_deref()
            .map(|t| format!(" ({t})"))
            .unwrap_or_default();
        let gloss = e.gloss.as_deref().unwrap_or("—");
        out.push_str(&format!(
            "- [V:{}] {lemma}{translit} — \"{gloss}\" [×{}]\n",
            e.strongs, e.occurrences
        ));
    }
    if !vl.sources.is_empty() {
        out.push_str(&format!("Fontes: {}\n", vl.sources.join(" · ")));
    }
    out
}

/// Resultado da verificação anti-alucinação de um texto gerado.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VerifiedOutput {
    /// Texto (inalterado na política `flag`, a padrão).
    pub text: String,
    /// Avisos: marcas `[V:...]` que não constam do acervo da passagem.
    pub warnings: Vec<String>,
}

/// Extrai os números de Strong citados na forma âncora `[V:H1234]`/`[V:G1234]`.
/// Ancorado à marca (não a dígitos soltos) para evitar falsos positivos.
fn cited_strongs(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let needle = b"[V:";
    let mut i = 0;
    while i + needle.len() < bytes.len() {
        if &bytes[i..i + needle.len()] == needle {
            let mut j = i + needle.len();
            // Aceita apenas a forma de Strong (H/G + dígitos + letra opcional).
            if j < bytes.len() && (bytes[j] == b'H' || bytes[j] == b'G') {
                let mut tok = String::new();
                tok.push(bytes[j] as char);
                j += 1;
                while j < bytes.len() && (bytes[j] as char).is_ascii_digit() {
                    tok.push(bytes[j] as char);
                    j += 1;
                }
                while j < bytes.len() && (bytes[j] as char).is_ascii_alphabetic() {
                    tok.push(bytes[j] as char);
                    j += 1;
                }
                // Precisa fechar com ']' e ter ao menos um dígito.
                if j < bytes.len() && bytes[j] == b']' && tok.len() > 1 {
                    out.push(tok);
                }
            }
        }
        i += 1;
    }
    out
}

/// Verifica o texto gerado contra o acervo léxico da passagem: marca como aviso
/// qualquer `[V:Strong]` citado que **não** esteja entre os dados fornecidos
/// (Strong inventado). Política `flag` (padrão): não altera o texto.
pub fn verify(text: &str, lexicon: &VerifiedLexicon) -> VerifiedOutput {
    let allowed = lexicon.strong_set();
    let mut warnings = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for cited in cited_strongs(text) {
        let base = base_strong(&cited);
        if !allowed.contains(&base) && seen.insert(base.clone()) {
            warnings.push(format!(
                "Strong não verificado citado: [V:{cited}] — não consta do acervo da passagem"
            ));
        }
    }
    VerifiedOutput {
        text: text.to_string(),
        warnings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Reference;
    use crate::store::Store;

    fn seeded() -> Store {
        let store = Store::open_in_memory().unwrap();
        let conn = store.conn();
        conn.execute(
            "INSERT INTO scholarly_sources(id,name,license,embeddable,attribution,url,version) \
             VALUES ('tahot','TAHOT','cc-by',1,'STEP Bible (CC BY 4.0)','u','v')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO scholarly_sources(id,name,license,embeddable,attribution,url,version) \
             VALUES ('tbesh','TBESH','cc-by',1,'STEP Bible (CC BY 4.0)','u','v')",
            [],
        )
        .unwrap();
        // Gênesis 1.1: bereshit (H7225G), bara (H1254A), elohim (H0430G).
        let toks = [
            (1u16, 1u16, "H7225G", "רֵאשִׁית", "beginning"),
            (1, 2, "H1254A", "בָּרָא", "to create"),
            (1, 3, "H0430G", "אֱלֹהִים", "God"),
        ];
        for (i, (ch, wi, st, lemma, _g)) in toks.iter().enumerate() {
            conn.execute(
                "INSERT INTO original_tokens(testament,book_number,chapter,verse,word_index,\
                 surface,lemma,strongs,strongs_raw,source_id) \
                 VALUES ('OT',1,?1,1,?2,?3,?4,?5,?5,'tahot')",
                params![*ch as i64, *wi as i64, lemma, lemma, st],
            )
            .unwrap();
            let _ = i;
        }
        for (st, gloss) in [
            ("H7225G", "first: beginning"),
            ("H1254A", "to create"),
            ("H0430G", "God"),
        ] {
            conn.execute(
                "INSERT INTO lexicon(strongs,lemma,gloss,source_id) VALUES (?1,'x',?2,'tbesh')",
                params![st, gloss],
            )
            .unwrap();
        }
        store
    }

    #[test]
    fn base_strong_strips_disambiguation() {
        assert_eq!(base_strong("H7225G"), "H7225");
        assert_eq!(base_strong("G0976"), "G0976");
        assert_eq!(base_strong("H0853"), "H0853");
    }

    #[test]
    fn verified_lexicon_aggregates_and_joins_gloss() {
        let store = seeded();
        let r = Reference::single(1, 1, 1);
        let vl = verified_lexicon(store.conn(), &r, &[1], Lang::Pt, 16);
        assert_eq!(vl.entries.len(), 3);
        // Strong base (sem o sufixo de desambiguação) é a chave de citação.
        let beginning = vl.entries.iter().find(|e| e.strongs == "H7225").unwrap();
        assert_eq!(beginning.gloss.as_deref(), Some("first: beginning"));
        assert!(vl.sources.iter().any(|s| s.contains("STEP Bible")));

        let block = format_verified_block(&vl, Lang::Pt);
        assert!(block.contains("[V:H7225]"));
        assert!(block.contains("first: beginning"));
        assert!(block.contains("Fontes:"));
    }

    #[test]
    fn empty_passage_yields_sentinel() {
        let store = Store::open_in_memory().unwrap(); // acervo base, sem dados
        let r = Reference::single(40, 1, 1);
        let vl = verified_lexicon(store.conn(), &r, &[1], Lang::Pt, 16);
        assert!(vl.is_empty());
        assert_eq!(format_verified_block(&vl, Lang::Pt), EMPTY_SENTINEL);
    }

    #[test]
    fn verify_flags_only_invented_strongs() {
        let store = seeded();
        let r = Reference::single(1, 1, 1);
        let vl = verified_lexicon(store.conn(), &r, &[1], Lang::Pt, 16);
        // H7225 é real (no acervo); G9999 é inventado.
        let out = verify(
            "A criação [V:H7225] é obra de Deus; também nota [V:G9999] (falso).",
            &vl,
        );
        assert_eq!(out.warnings.len(), 1);
        assert!(out.warnings[0].contains("G9999"));
        // Dígitos soltos fora da marca [V:] NÃO disparam falso positivo.
        let clean = verify("Texto sobre H9999 e G1234 sem marcação.", &vl);
        assert!(clean.warnings.is_empty());
    }
}
