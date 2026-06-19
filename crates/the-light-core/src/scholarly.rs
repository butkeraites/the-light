//! Importação de dados acadêmicos **STEPBible (CC BY 4.0)**: tokens de língua
//! original (TAHOT hebraico, TAGNT grego) + léxicos breves (TBESH/TBESG).
//!
//! A lógica vive aqui (e não no `xtask`) para ser acessível ao binário enviado:
//! a TUI dispara a instalação numa thread de fundo e a CLI bloqueia o modo
//! acadêmico até que os dados estejam presentes. Procedência e licença são
//! gravadas em `scholarly_sources` ANTES dos dados; fontes não-embarcáveis ou na
//! *denylist* são recusadas (defesa em profundidade contra violação de licença).
//!
//! Atribuição exigida (verbatim, do README/arquivos do release):
//! "Credit it to 'STEP Bible' linked to www.STEPBible.org".
//!
//! O progresso é reportado por um *callback* (`FnMut(&str)`), sem acoplar a
//! nenhuma UI: o xtask imprime no stdout, a TUI envia por um canal.

use std::collections::HashSet;
use std::path::Path;
use std::time::Duration;

use rusqlite::{params, Connection};

use crate::reference::book_number;

const STEP_RAW: &str = "https://raw.githubusercontent.com/STEPBible/STEPBible-Data/master";
const STEP_URL: &str = "https://github.com/STEPBible/STEPBible-Data";
/// Atribuição obrigatória (gravada em `scholarly_sources` e exibível pela UI).
pub const ATTRIBUTION: &str =
    "Credit it to 'STEP Bible' linked to www.STEPBible.org (data based on work at \
     Tyndale House, Cambridge; CC BY 4.0)";
const STEP_VERSION: &str = "STEPBible-Data master";
const HTTP_TIMEOUT: Duration = Duration::from_secs(120);

/// Fontes que NUNCA podem ser embarcadas — recusadas independentemente de
/// qualquer string de licença (texto SBLGNT sob EULA; léxicos proprietários).
const DENYLIST: &[&str] = &["sblgnt", "morphgnt", "louwnida", "bdag", "halot"];

/// Erros da importação acadêmica.
#[derive(Debug, thiserror::Error)]
pub enum ScholarlyError {
    /// Erro genérico com mensagem.
    #[error("{0}")]
    Msg(String),
    /// Erro de rede/HTTP.
    #[error("erro de rede: {0}")]
    Http(String),
    /// Erro de I/O.
    #[error("erro de I/O: {0}")]
    Io(#[from] std::io::Error),
    /// Erro de SQLite.
    #[error("erro de SQLite: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

/// Resultado da importação acadêmica.
pub type Result<T> = std::result::Result<T, ScholarlyError>;

/// Tipo de conteúdo de um conjunto STEP.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// Tokens hebraicos (TAHOT) → `original_tokens`, testamento OT.
    HebrewTokens,
    /// Tokens gregos (TAGNT) → `original_tokens`, testamento NT.
    GreekTokens,
    /// Léxico breve (TBESH/TBESG) → `lexicon`.
    Lexicon,
}

/// Especificação de um conjunto embarcável (registro + arquivos remotos).
struct ScholarlySpec {
    id: &'static str,
    name: &'static str,
    license: &'static str,
    /// Espelha `License::is_embeddable`; precisa ser `true` para importar.
    embeddable: bool,
    kind: Kind,
    /// Pasta no repo (decodificada).
    folder: &'static str,
    /// Arquivos no repo (decodificados); baixados em ordem e concatenados.
    files: &'static [&'static str],
}

const SPECS: &[ScholarlySpec] = &[
    ScholarlySpec {
        id: "tahot",
        name: "Translators Amalgamated Hebrew OT (TAHOT)",
        license: "cc-by",
        embeddable: true,
        kind: Kind::HebrewTokens,
        folder: "Translators Amalgamated OT+NT",
        files: &[
            "TAHOT Gen-Deu - Translators Amalgamated Hebrew OT - STEPBible.org CC BY.txt",
            "TAHOT Jos-Est - Translators Amalgamated Hebrew OT - STEPBible.org CC BY.txt",
            "TAHOT Job-Sng - Translators Amalgamated Hebrew OT - STEPBible.org CC BY.txt",
            "TAHOT Isa-Mal - Translators Amalgamated Hebrew OT - STEPBible.org CC BY.txt",
        ],
    },
    ScholarlySpec {
        id: "tagnt",
        name: "Translators Amalgamated Greek NT (TAGNT)",
        license: "cc-by",
        embeddable: true,
        kind: Kind::GreekTokens,
        folder: "Translators Amalgamated OT+NT",
        files: &[
            "TAGNT Mat-Jhn - Translators Amalgamated Greek NT - STEPBible.org CC-BY.txt",
            "TAGNT Act-Rev - Translators Amalgamated Greek NT - STEPBible.org CC-BY.txt",
        ],
    },
    ScholarlySpec {
        id: "tbesh",
        name: "Translators Brief lexicon of Extended Strongs for Hebrew (TBESH)",
        license: "cc-by",
        embeddable: true,
        kind: Kind::Lexicon,
        folder: "Lexicons",
        files: &["TBESH - Translators Brief lexicon of Extended Strongs for Hebrew - STEPBible.org CC BY.txt"],
    },
    ScholarlySpec {
        id: "tbesg",
        name: "Translators Brief lexicon of Extended Strongs for Greek (TBESG)",
        license: "cc-by",
        embeddable: true,
        kind: Kind::Lexicon,
        folder: "Lexicons",
        files: &["TBESG - Translators Brief lexicon of Extended Strongs for Greek - STEPBible.org CC BY.txt"],
    },
];

/// Conjuntos importáveis, em ordem (set completo padrão).
pub fn default_datasets() -> Vec<String> {
    SPECS.iter().map(|s| s.id.to_string()).collect()
}

/// Lista as ids importáveis como texto (para ajuda/erros).
pub fn available_datasets() -> String {
    SPECS.iter().map(|s| s.id).collect::<Vec<_>>().join(", ")
}

/// `true` se há tokens de língua original no banco (dados acadêmicos instalados).
pub fn is_populated(conn: &Connection) -> bool {
    conn.query_row("SELECT count(*) FROM original_tokens", [], |r| {
        r.get::<_, i64>(0)
    })
    .map(|n| n > 0)
    .unwrap_or(false)
}

// ----------------------------------------------------------------------------
// Mapeamento dos códigos de livro do STEPBible → número canônico 1..66.
// STEP usa códigos próprios (ex.: NT `Mrk`, `Jhn`); tentamos este mapa primeiro
// e caímos em `reference::book_number` como rede de segurança.
// ----------------------------------------------------------------------------

#[rustfmt::skip]
const STEP_BOOKS: &[(&str, u8)] = &[
    ("Gen", 1), ("Exo", 2), ("Lev", 3), ("Num", 4), ("Deu", 5), ("Jos", 6),
    ("Jdg", 7), ("Rut", 8), ("1Sa", 9), ("2Sa", 10), ("1Ki", 11), ("2Ki", 12),
    ("1Ch", 13), ("2Ch", 14), ("Ezr", 15), ("Neh", 16), ("Est", 17), ("Job", 18),
    ("Psa", 19), ("Pro", 20), ("Ecc", 21), ("Sng", 22), ("Isa", 23), ("Jer", 24),
    ("Lam", 25), ("Ezk", 26), ("Dan", 27), ("Hos", 28), ("Jol", 29), ("Amo", 30),
    ("Oba", 31), ("Jon", 32), ("Mic", 33), ("Nam", 34), ("Hab", 35), ("Zep", 36),
    ("Hag", 37), ("Zec", 38), ("Mal", 39), ("Mat", 40), ("Mrk", 41), ("Luk", 42),
    ("Jhn", 43), ("Act", 44), ("Rom", 45), ("1Co", 46), ("2Co", 47), ("Gal", 48),
    ("Eph", 49), ("Php", 50), ("Col", 51), ("1Th", 52), ("2Th", 53), ("1Ti", 54),
    ("2Ti", 55), ("Tit", 56), ("Phm", 57), ("Heb", 58), ("Jas", 59), ("1Pe", 60),
    ("2Pe", 61), ("1Jn", 62), ("2Jn", 63), ("3Jn", 64), ("Jud", 65), ("Rev", 66),
];

fn step_book_number(code: &str) -> Option<u8> {
    STEP_BOOKS
        .iter()
        .find(|(c, _)| c.eq_ignore_ascii_case(code))
        .map(|(_, n)| *n)
        .or_else(|| book_number(code))
}

// ----------------------------------------------------------------------------
// Modelo de dados intermediário.
// ----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
struct Token {
    testament: &'static str,
    book: u8,
    chapter: u16,
    verse: u16,
    word_index: u16,
    surface: String,
    translit: Option<String>,
    lemma: Option<String>,
    strongs: Option<String>,
    strongs_raw: Option<String>,
    morph: Option<String>,
    gloss: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LexEntry {
    strongs: String,
    lemma: Option<String>,
    translit: Option<String>,
    gloss: Option<String>,
    definition: Option<String>,
    derivation: Option<String>,
}

// ----------------------------------------------------------------------------
// Parsers.
// ----------------------------------------------------------------------------

/// Decompõe a coluna de referência STEP `Eng(+Heb)#word=type` → (book, ch, vs,
/// word_index). Usa a referência **em inglês** (antes de `(`), que é a
/// versificação das traduções embarcadas. `None` para linhas inválidas.
fn parse_ref(col: &str) -> Option<(u8, u16, u16, u16)> {
    let (refpart, after_hash) = col.split_once('#')?;
    let eng = refpart.split('(').next()?.trim();
    let mut it = eng.split('.');
    let book = step_book_number(it.next()?.trim())?;
    let chapter: u16 = it.next()?.trim().parse().ok()?;
    let verse: u16 = leading_number(it.next()?.trim())?;
    let word_index: u16 = leading_number(after_hash)?;
    Some((book, chapter, verse, word_index))
}

fn leading_number(s: &str) -> Option<u16> {
    let digits: String = s
        .trim()
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.parse().ok()
}

/// Extrai o primeiro Strong (`H1254A`/`G0976`) de um trecho.
fn extract_strong(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if (c == 'H' || c == 'G') && bytes.get(i + 1).is_some_and(|b| b.is_ascii_digit()) {
            let mut out = String::from(c);
            let mut j = i + 1;
            while j < bytes.len() && (bytes[j] as char).is_ascii_digit() {
                out.push(bytes[j] as char);
                j += 1;
            }
            while j < bytes.len() && (bytes[j] as char).is_ascii_alphabetic() {
                out.push(bytes[j] as char);
                j += 1;
            }
            return Some(out);
        }
        i += 1;
    }
    None
}

/// Conteúdo entre as primeiras chaves `{...}` (a raiz, nas dStrongs do TAHOT).
fn braces_content(s: &str) -> Option<&str> {
    let start = s.find('{')?;
    let end = s[start..].find('}')? + start;
    Some(&s[start + 1..end])
}

fn clean_surface(s: &str) -> String {
    collapse_ws(&s.replace(['/', '\\'], ""))
}

fn clean_gloss(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_angle = false;
    for c in s.chars() {
        match c {
            '<' => in_angle = true,
            '>' => in_angle = false,
            '/' | '[' | ']' => {}
            _ if in_angle => {}
            _ => out.push(c),
        }
    }
    collapse_ws(&out)
}

fn clean_definition(s: &str) -> String {
    let lowered = s.replace("<br>", "; ").replace("<BR>", "; ");
    let mut out = String::with_capacity(lowered.len());
    let mut in_tag = false;
    for c in lowered.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if in_tag => {}
            _ => out.push(c),
        }
    }
    collapse_ws(&out)
}

fn collapse_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn opt(s: String) -> Option<String> {
    let t = s.trim().to_string();
    if t.is_empty() {
        None
    } else {
        Some(t)
    }
}

fn parse_hebrew_row(line: &str) -> Option<Token> {
    let cols: Vec<&str> = line.split('\t').collect();
    if cols.len() < 6 {
        return None;
    }
    let (book, chapter, verse, word_index) = parse_ref(cols[0])?;
    let raw_strong = braces_content(cols[4]).and_then(extract_strong);
    Some(Token {
        testament: "OT",
        book,
        chapter,
        verse,
        word_index,
        surface: clean_surface(cols[1]),
        translit: opt(cols[2].replace('/', "")),
        lemma: None,
        strongs: raw_strong.clone(),
        strongs_raw: raw_strong,
        morph: opt(cols[5].to_string()),
        gloss: opt(clean_gloss(cols[3])),
    })
}

fn parse_greek_row(line: &str) -> Option<Token> {
    let cols: Vec<&str> = line.split('\t').collect();
    if cols.len() < 5 {
        return None;
    }
    let (book, chapter, verse, word_index) = parse_ref(cols[0])?;
    let (surface, translit) = match cols[1].split_once('(') {
        Some((s, t)) => (
            s.trim().to_string(),
            opt(t.trim_end_matches(')').to_string()),
        ),
        None => (cols[1].trim().to_string(), None),
    };
    let (strongs, morph) = match cols[3].split_once('=') {
        Some((s, m)) => (extract_strong(s), opt(m.to_string())),
        None => (extract_strong(cols[3]), None),
    };
    let lemma = cols
        .get(4)
        .and_then(|c| c.split('=').next())
        .and_then(|l| opt(l.to_string()));
    Some(Token {
        testament: "NT",
        book,
        chapter,
        verse,
        word_index,
        surface,
        translit,
        lemma,
        strongs: strongs.clone(),
        strongs_raw: strongs,
        morph,
        gloss: opt(clean_gloss(cols[2])),
    })
}

fn parse_lexicon_row(line: &str) -> Option<LexEntry> {
    let cols: Vec<&str> = line.split('\t').collect();
    if cols.len() < 7 {
        return None;
    }
    let (key_part, relation) = match cols[1].split_once('=') {
        Some((k, r)) => (k.trim(), opt(r.to_string())),
        None => (cols[1].trim(), None),
    };
    let strongs = extract_strong(key_part)?;
    Some(LexEntry {
        strongs,
        lemma: opt(cols[3].to_string()),
        translit: opt(cols[4].to_string()),
        gloss: opt(clean_gloss(cols[6])),
        definition: cols.get(7).and_then(|d| opt(clean_definition(d))),
        derivation: relation,
    })
}

// ----------------------------------------------------------------------------
// Inserção (transacional, idempotente por fonte).
// ----------------------------------------------------------------------------

fn record_source(conn: &Connection, spec: &ScholarlySpec) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO scholarly_sources \
         (id,name,license,embeddable,attribution,url,version) VALUES (?1,?2,?3,?4,?5,?6,?7)",
        params![
            spec.id,
            spec.name,
            spec.license,
            spec.embeddable as i64,
            ATTRIBUTION,
            STEP_URL,
            STEP_VERSION
        ],
    )?;
    Ok(())
}

fn import_tokens(conn: &mut Connection, spec: &ScholarlySpec, tokens: &[Token]) -> Result<usize> {
    let tx = conn.transaction()?;
    record_source(&tx, spec)?;
    tx.execute(
        "DELETE FROM original_tokens WHERE source_id = ?1",
        params![spec.id],
    )?;
    {
        let mut stmt = tx.prepare(
            "INSERT OR IGNORE INTO original_tokens \
             (testament,book_number,chapter,verse,word_index,surface,translit,lemma,\
              strongs,strongs_raw,morph_code,gloss,source_id) \
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
        )?;
        for t in tokens {
            stmt.execute(params![
                t.testament,
                t.book,
                t.chapter,
                t.verse,
                t.word_index,
                t.surface,
                t.translit,
                t.lemma,
                t.strongs,
                t.strongs_raw,
                t.morph,
                t.gloss,
                spec.id,
            ])?;
        }
    }
    let count: i64 = tx.query_row(
        "SELECT count(*) FROM original_tokens WHERE source_id = ?1",
        params![spec.id],
        |r| r.get(0),
    )?;
    tx.commit()?;
    Ok(count as usize)
}

fn import_lexicon(
    conn: &mut Connection,
    spec: &ScholarlySpec,
    entries: &[LexEntry],
) -> Result<usize> {
    let tx = conn.transaction()?;
    record_source(&tx, spec)?;
    tx.execute("DELETE FROM lexicon WHERE source_id = ?1", params![spec.id])?;
    {
        let mut stmt = tx.prepare(
            "INSERT OR IGNORE INTO lexicon \
             (strongs,lemma,translit,pron,gloss,gloss_pt,definition,derivation,source_id) \
             VALUES (?1,?2,?3,NULL,?4,NULL,?5,?6,?7)",
        )?;
        for e in entries {
            stmt.execute(params![
                e.strongs,
                e.lemma,
                e.translit,
                e.gloss,
                e.definition,
                e.derivation,
                spec.id,
            ])?;
        }
    }
    let count: i64 = tx.query_row(
        "SELECT count(*) FROM lexicon WHERE source_id = ?1",
        params![spec.id],
        |r| r.get(0),
    )?;
    tx.commit()?;
    Ok(count as usize)
}

// ----------------------------------------------------------------------------
// Download + orquestração.
// ----------------------------------------------------------------------------

/// Percent-encoda um caminho do repo (mantém `/`, `-`, `.`, `_`, `~`).
fn encode_path(path: &str) -> String {
    let mut out = String::with_capacity(path.len() * 2);
    for b in path.bytes() {
        let c = b as char;
        if c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_' | '~' | '/') {
            out.push(c);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Baixa uma URL (segue redirects por padrão).
fn download(url: &str) -> Result<Vec<u8>> {
    let client = reqwest::blocking::Client::builder()
        .user_agent("the-light/scholarly-importer")
        .timeout(HTTP_TIMEOUT)
        .build()
        .map_err(|e| ScholarlyError::Http(e.to_string()))?;
    let resp = client
        .get(url)
        .send()
        .map_err(|e| ScholarlyError::Http(e.to_string()))?
        .error_for_status()
        .map_err(|e| ScholarlyError::Http(e.to_string()))?;
    let bytes = resp
        .bytes()
        .map_err(|e| ScholarlyError::Http(e.to_string()))?;
    Ok(bytes.to_vec())
}

/// Obtém o texto de uma spec (cache em `seed_dir`, respeitando `offline`/`force`).
fn obtain_text(
    spec: &ScholarlySpec,
    seed_dir: &Path,
    offline: bool,
    force: bool,
) -> Result<String> {
    let mut combined = String::new();
    for (i, file) in spec.files.iter().enumerate() {
        let cache = seed_dir.join(format!("{}-{i}.txt", spec.id));
        let text = if cache.exists() && !force {
            std::fs::read_to_string(&cache)?
        } else if offline {
            return Err(ScholarlyError::Msg(format!(
                "{} ausente e modo offline ativo",
                cache.display()
            )));
        } else {
            let url = format!(
                "{STEP_RAW}/{}/{}",
                encode_path(spec.folder),
                encode_path(file)
            );
            let bytes = download(&url)?;
            std::fs::write(&cache, &bytes)?;
            String::from_utf8(bytes)
                .map_err(|_| ScholarlyError::Msg(format!("{file} não é UTF-8")))?
        };
        combined.push_str(&text);
        combined.push('\n');
    }
    Ok(combined)
}

fn sanity_floor(spec: &ScholarlySpec) -> usize {
    match spec.id {
        "tahot" => 300_000,
        "tagnt" => 100_000,
        "tbesh" | "tbesg" => 5_000,
        _ => 1,
    }
}

fn seen_insert(seen: &mut HashSet<(u8, u16, u16, u16)>, t: &Token) -> bool {
    seen.insert((t.book, t.chapter, t.verse, t.word_index))
}

/// Valida licença/embarcabilidade ANTES de qualquer importação (defesa em
/// profundidade): denylist + flag `embeddable` + string de licença CC-BY/PD.
fn ensure_embeddable(spec: &ScholarlySpec) -> Result<()> {
    let id = spec.id.to_ascii_lowercase();
    if DENYLIST.iter().any(|d| *d == id) {
        return Err(ScholarlyError::Msg(format!(
            "`{}` está na denylist e não pode ser embarcado",
            spec.id
        )));
    }
    if !spec.embeddable {
        return Err(ScholarlyError::Msg(format!(
            "`{}` não é embarcável (embeddable=false)",
            spec.id
        )));
    }
    let lic = spec.license.to_ascii_lowercase();
    let ok = matches!(lic.as_str(), "public-domain" | "cc0") || lic.starts_with("cc-by");
    if !ok || lic.contains("-sa") || lic.contains("-nc") || lic.contains("-nd") {
        return Err(ScholarlyError::Msg(format!(
            "licença `{}` de `{}` não passa na verificação de embarcabilidade",
            spec.license, spec.id
        )));
    }
    Ok(())
}

fn import_spec(
    conn: &mut Connection,
    spec: &ScholarlySpec,
    seed_dir: &Path,
    offline: bool,
    force: bool,
    progress: &mut dyn FnMut(&str),
) -> Result<usize> {
    progress(&format!("Baixando {} ({})…", spec.name, spec.license));
    let text = obtain_text(spec, seed_dir, offline, force)?;
    match spec.kind {
        Kind::HebrewTokens | Kind::GreekTokens => {
            let parse = if spec.kind == Kind::HebrewTokens {
                parse_hebrew_row
            } else {
                parse_greek_row
            };
            // Dedupe por (livro,capítulo,versículo,palavra): a 1ª leitura vence.
            let mut seen: HashSet<(u8, u16, u16, u16)> = HashSet::new();
            let mut tokens: Vec<Token> = Vec::new();
            for line in text.lines() {
                if let Some(t) = parse(line) {
                    if seen_insert(&mut seen, &t) {
                        tokens.push(t);
                    }
                }
            }
            let floor = sanity_floor(spec);
            if tokens.len() < floor {
                return Err(ScholarlyError::Msg(format!(
                    "apenas {} tokens em `{}` (piso {floor}); fonte incompleta?",
                    tokens.len(),
                    spec.id
                )));
            }
            progress(&format!(
                "Importando {} ({} tokens)…",
                spec.id,
                tokens.len()
            ));
            import_tokens(conn, spec, &tokens)
        }
        Kind::Lexicon => {
            let mut seen: HashSet<String> = HashSet::new();
            let mut entries: Vec<LexEntry> = Vec::new();
            for line in text.lines() {
                if let Some(e) = parse_lexicon_row(line) {
                    if seen.insert(e.strongs.clone()) {
                        entries.push(e);
                    }
                }
            }
            let floor = sanity_floor(spec);
            if entries.len() < floor {
                return Err(ScholarlyError::Msg(format!(
                    "apenas {} entradas em `{}` (piso {floor}); fonte incompleta?",
                    entries.len(),
                    spec.id
                )));
            }
            progress(&format!(
                "Importando {} ({} entradas)…",
                spec.id,
                entries.len()
            ));
            import_lexicon(conn, spec, &entries)
        }
    }
}

/// Importa os `datasets` (ids) para o banco `conn`, baixando/cacheando em
/// `seed_dir`. Devolve `(id, registros)` por conjunto. O `progress` recebe
/// mensagens de fase (UI-agnóstico). Valida licença/denylist antes de baixar.
pub fn import(
    conn: &mut Connection,
    datasets: &[String],
    seed_dir: &Path,
    offline: bool,
    force: bool,
    progress: &mut dyn FnMut(&str),
) -> Result<Vec<(String, usize)>> {
    if datasets.is_empty() {
        return Err(ScholarlyError::Msg(format!(
            "informe ao menos um conjunto (disponíveis: {})",
            available_datasets()
        )));
    }
    // Resolve specs e valida licença ANTES de baixar.
    let mut chosen: Vec<&ScholarlySpec> = Vec::new();
    for v in datasets {
        let v = v.trim().to_ascii_lowercase();
        let spec = SPECS.iter().find(|s| s.id == v).ok_or_else(|| {
            ScholarlyError::Msg(format!(
                "conjunto desconhecido `{v}` (use: {})",
                available_datasets()
            ))
        })?;
        ensure_embeddable(spec)?;
        chosen.push(spec);
    }
    std::fs::create_dir_all(seed_dir)?;

    let mut out = Vec::new();
    for spec in chosen {
        let n = import_spec(conn, spec, seed_dir, offline, force, progress)?;
        progress(&format!("✓ {} — {n} registros", spec.id));
        out.push((spec.id.to_string(), n));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;

    #[test]
    fn step_book_codes_resolve() {
        assert_eq!(step_book_number("Gen"), Some(1));
        assert_eq!(step_book_number("Deu"), Some(5));
        assert_eq!(step_book_number("Psa"), Some(19));
        assert_eq!(step_book_number("Mrk"), Some(41));
        assert_eq!(step_book_number("Jhn"), Some(43));
        assert_eq!(step_book_number("Rev"), Some(66));
        assert_eq!(step_book_number("Nope"), None);
    }

    #[test]
    fn parse_ref_uses_english_versification() {
        assert_eq!(parse_ref("Gen.1.1#01=L"), Some((1, 1, 1, 1)));
        assert_eq!(parse_ref("Mat.1.1#03=NKO"), Some((40, 1, 1, 3)));
        assert_eq!(parse_ref("Psa.3.1(3.2)#05=L"), Some((19, 3, 1, 5)));
        assert_eq!(parse_ref("lixo"), None);
    }

    #[test]
    fn extract_strong_handles_prefixes_and_letters() {
        assert_eq!(extract_strong("{H7225G}").as_deref(), Some("H7225G"));
        assert_eq!(extract_strong("H1254A").as_deref(), Some("H1254A"));
        assert_eq!(extract_strong("G0976=N-NSF").as_deref(), Some("G0976"));
        assert_eq!(braces_content("H9003/{H7225G}"), Some("H7225G"));
        assert_eq!(extract_strong("H9003/{H7225G}").as_deref(), Some("H9003"));
    }

    #[test]
    fn parse_hebrew_row_extracts_root_strong() {
        let line = "Gen.1.1#01=L\tבְּ/רֵאשִׁ֖ית\tbe./re.Shit\tin/ beginning\tH9003/{H7225G}\tHR/Ncfsa\t\t\tH7225G";
        let t = parse_hebrew_row(line).unwrap();
        assert_eq!(t.testament, "OT");
        assert_eq!((t.book, t.chapter, t.verse, t.word_index), (1, 1, 1, 1));
        assert_eq!(t.strongs.as_deref(), Some("H7225G"));
        assert_eq!(t.gloss.as_deref(), Some("in beginning"));
        assert_eq!(t.morph.as_deref(), Some("HR/Ncfsa"));
    }

    #[test]
    fn parse_greek_row_splits_strong_and_morph() {
        let line = "Mat.1.1#01=NKO\tΒίβλος (Biblos)\t[The] book\tG0976=N-NSF\tβίβλος=book";
        let t = parse_greek_row(line).unwrap();
        assert_eq!(t.testament, "NT");
        assert_eq!((t.book, t.chapter, t.verse, t.word_index), (40, 1, 1, 1));
        assert_eq!(t.surface, "Βίβλος");
        assert_eq!(t.translit.as_deref(), Some("Biblos"));
        assert_eq!(t.strongs.as_deref(), Some("G0976"));
        assert_eq!(t.morph.as_deref(), Some("N-NSF"));
        assert_eq!(t.lemma.as_deref(), Some("βίβλος"));
        assert_eq!(t.gloss.as_deref(), Some("The book"));
    }

    #[test]
    fn parse_lexicon_row_keys_on_extended_strong() {
        let line = "H0001\tH0001G =\tH0001G\tאָב\tav\tH:N-M\tfather\t1) father of an individual<br>2) of God";
        let e = parse_lexicon_row(line).unwrap();
        assert_eq!(e.strongs, "H0001G");
        assert_eq!(e.lemma.as_deref(), Some("אָב"));
        assert_eq!(e.translit.as_deref(), Some("av"));
        assert_eq!(e.gloss.as_deref(), Some("father"));
        assert_eq!(
            e.definition.as_deref(),
            Some("1) father of an individual; 2) of God")
        );
    }

    #[test]
    fn denylist_and_license_are_enforced() {
        let bad = ScholarlySpec {
            id: "sblgnt",
            name: "x",
            license: "cc-by",
            embeddable: true,
            kind: Kind::GreekTokens,
            folder: "",
            files: &[],
        };
        assert!(ensure_embeddable(&bad).is_err());
        let sa = ScholarlySpec {
            id: "foo",
            name: "x",
            license: "cc-by-sa",
            embeddable: true,
            kind: Kind::Lexicon,
            folder: "",
            files: &[],
        };
        assert!(ensure_embeddable(&sa).is_err());
        assert!(ensure_embeddable(&SPECS[0]).is_ok());
    }

    #[test]
    fn encode_path_percent_encodes_spaces_and_plus() {
        assert_eq!(
            encode_path("Translators Amalgamated OT+NT"),
            "Translators%20Amalgamated%20OT%2BNT"
        );
        assert_eq!(encode_path("a-b_c.txt"), "a-b_c.txt");
    }

    #[test]
    fn is_populated_false_on_base_db() {
        let store = Store::open_in_memory().unwrap();
        assert!(!is_populated(store.conn()));
    }

    #[test]
    fn offline_without_cache_errors() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open_in_memory().unwrap();
        let mut noop = |_: &str| {};
        let err = import(
            store.conn_mut(),
            &["tbesh".to_string()],
            dir.path(),
            true,
            false,
            &mut noop,
        );
        assert!(matches!(err, Err(ScholarlyError::Msg(_))));
    }
}
