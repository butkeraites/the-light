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
use rusqlite::Connection;

use crate::model::Lang;
// `Reference` só entra na recuperação SQLite (`embedded`); os tipos e a verificação
// anti-alucinação são puros.
#[cfg(feature = "embedded")]
use crate::model::Reference;
// `base_strong` (Strong base) e `resolve_verses` vivem em `crate::query` (fonte única
// nativo↔web, ADR-0062). `base_strong` é usado pela agregação SQLite (`embedded`) E pela
// verificação PURA `verify` — por isso o import é un-gated; `resolve_verses` (só
// `embedded`) é chamado por caminho fully-qualified.
use crate::query::base_strong;

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

/// Um **token de idioma original** (interlinear), na ordem de leitura (`word_index`).
///
/// Diferente de [`LexicalEntry`] (agregado por Strong, para o estudo), aqui cada PALAVRA do
/// versículo é uma linha — incluindo partículas sem Strong — preservando a ordem. Tipo **puro**
/// (presente em todos os alvos, como [`LexicalEntry`]); só a leitura do SQLite é `embedded`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterlinearToken {
    /// A palavra na língua original (hebraico/grego), como impressa.
    pub surface: String,
    /// Transliteração.
    pub translit: Option<String>,
    /// Lema.
    pub lemma: Option<String>,
    /// Número de Strong (desambiguado), quando a palavra é etiquetada.
    pub strongs: Option<String>,
    /// Código de morfologia VERBATIM (a `morph_legend` ainda não o decodifica — dado futuro).
    pub morph_code: Option<String>,
    /// Glosa breve: `COALESCE(léxico.gloss_pt, léxico.gloss, token.gloss)`.
    pub gloss: Option<String>,
    /// Posição da palavra no versículo (0-based).
    pub word_index: u32,
    /// Testamento ("OT" hebraico | "NT" grego).
    pub testament: String,
}

/// Tokens interlineares de UM versículo + as fontes (atribuição CC-BY obrigatória).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InterlinearVerse {
    /// Palavras na ordem de leitura (`word_index`). Vazio = versículo sem cobertura no acervo.
    pub tokens: Vec<InterlinearToken>,
    /// Atribuições verbatim das fontes usadas (STEP Bible CC-BY).
    pub sources: Vec<String>,
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

/// Acumula os tokens de uma passagem (um versículo, ou o capítulo todo se
/// `verse = None`) no agregador por Strong base. O `(sql, params)` vem do plano puro
/// `crate::query::lexicon_collect_plan` (fonte única nativo↔web, ADR-0062); a agregação
/// "primeiro não-nulo vence" segue aqui (nativo) — o web a espelha no shaper TS.
#[cfg(feature = "embedded")]
fn collect(
    conn: &Connection,
    book: u8,
    chapter: u16,
    verse: Option<u16>,
    by_base: &mut BTreeMap<String, Agg>,
    sources: &mut BTreeSet<String>,
) -> rusqlite::Result<()> {
    let (sql, params) = crate::query::lexicon_collect_plan(book, chapter, verse);
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query(rusqlite::params_from_iter(
        params.into_iter().map(rusqlite::types::Value::from),
    ))?;
    while let Some(row) = rows.next()? {
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
        match crate::query::resolve_verses(reference, verse_numbers) {
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

/// Busca as atribuições (verbatim) das fontes usadas. O `(sql, params)` vem do plano puro
/// `crate::query::attributions_plan` (fonte única nativo↔web, ADR-0062); a dedup
/// preservando a ordem segue aqui.
#[cfg(feature = "embedded")]
fn attributions_for(conn: &Connection, ids: &BTreeSet<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for id in ids {
        let (sql, params) = crate::query::attributions_plan(id);
        if let Ok(attr) = conn.query_row(
            &sql,
            rusqlite::params_from_iter(params.into_iter().map(rusqlite::types::Value::from)),
            |r| r.get::<_, String>(0),
        ) {
            if seen.insert(attr.clone()) {
                out.push(attr);
            }
        }
    }
    out
}

/// Tokens INTERLINEARES de um versículo, na ordem de leitura, para o modo de leitura em idioma
/// original. **Sem** filtro de Strong (partículas sem etiqueta também aparecem) e **sem** agregação
/// (uma linha por palavra), ao contrário de [`verified_lexicon`]. `LEFT JOIN lexicon` traz a glosa
/// PT quando existe. Best-effort: acervo sem `original_tokens` → [`InterlinearVerse::default`].
///
/// Lê do SQLite (`rusqlite`) → só `embedded`. No web a leitura é espelhada em TS (ADR-0011); o tipo
/// [`InterlinearToken`] é puro/compartilhado. Anti-alucinação: os campos vêm SÓ do banco, verbatim.
#[cfg(feature = "embedded")]
pub fn interlinear_tokens(
    conn: &Connection,
    book: u8,
    chapter: u16,
    verse: u16,
) -> InterlinearVerse {
    let mut source_ids: BTreeSet<String> = BTreeSet::new();
    let read = (|| -> rusqlite::Result<Vec<InterlinearToken>> {
        // `(sql, params)` do plano puro `crate::query::interlinear_plan` (fonte única
        // nativo↔web, ADR-0062); a montagem dos tokens na ordem de leitura segue aqui.
        let (sql, params) = crate::query::interlinear_plan(book, chapter, verse);
        let mut stmt = conn.prepare(&sql)?;
        let mut rows = stmt.query(rusqlite::params_from_iter(
            params.into_iter().map(rusqlite::types::Value::from),
        ))?;
        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            if let Some(s) = row.get::<_, Option<String>>(8)? {
                source_ids.insert(s);
            }
            out.push(InterlinearToken {
                surface: row.get(0)?,
                translit: row.get(1)?,
                lemma: row.get(2)?,
                strongs: row.get(3)?,
                morph_code: row.get(4)?,
                gloss: row.get(5)?,
                word_index: row.get::<_, i64>(6)? as u32,
                testament: row.get(7)?,
            });
        }
        Ok(out)
    })();
    match read {
        Ok(tokens) => {
            let sources = attributions_for(conn, &source_ids);
            InterlinearVerse { tokens, sources }
        }
        Err(_) => InterlinearVerse::default(),
    }
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
    // `params!` agora é só de teste (a recuperação usa `params_from_iter` sobre o plano).
    use rusqlite::params;

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

    // `base_strong` movido p/ `crate::query` (ADR-0062); seu teste unitário mora lá.

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
    fn interlinear_tokens_ordered_words_no_aggregation() {
        let store = seeded();
        let iv = interlinear_tokens(store.conn(), 1, 1, 1);
        // Uma linha POR PALAVRA (sem agregar por Strong), na ordem de leitura.
        assert_eq!(iv.tokens.len(), 3);
        assert_eq!(iv.tokens[0].surface, "רֵאשִׁית"); // superfície verbatim do store
        assert_eq!(iv.tokens[0].strongs.as_deref(), Some("H7225G")); // Strong CRU (não a base)
        assert_eq!(iv.tokens[0].gloss.as_deref(), Some("first: beginning")); // COALESCE do léxico
        assert_eq!(iv.tokens[1].surface, "בָּרָא");
        assert_eq!(iv.tokens[2].surface, "אֱלֹהִים");
        assert_eq!(iv.tokens[0].testament, "OT");
        assert!(iv.sources.iter().any(|s| s.contains("STEP Bible"))); // CC-BY obrigatório
                                                                      // Versículo sem cobertura → vazio (best-effort, acervo base).
        assert!(interlinear_tokens(store.conn(), 1, 2, 1).tokens.is_empty());
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
