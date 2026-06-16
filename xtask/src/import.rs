//! Importador de versões livres (PT + EN) para o banco SQLite.
//!
//! Regras (ver `DATA_SOURCES.md` e `IMPLEMENTATION_PLAN.md` §5):
//! - Só importa versões **de domínio público / livres** registradas em [`SPECS`].
//!   Apontar para qualquer outra URL é um erro explícito (nunca embarcar versão
//!   protegida).
//! - Idempotente: reimportar uma versão substitui suas linhas.
//! - Os arquivos brutos vão para `data/seed/` (fora do git); o app roda offline
//!   sobre o banco gerado.

use anyhow::{anyhow, bail, Context, Result};
use biblia_core::model::Lang;
use biblia_core::reference::BOOKS;
use biblia_core::store::Store;
use indicatif::{ProgressBar, ProgressStyle};
use rusqlite::{params, Connection};
use serde::Deserialize;
use std::path::PathBuf;

/// Formato bruto de um dataset suportado.
#[derive(Debug, Clone, Copy)]
enum Format {
    /// scrollmapper: objeto `{translation, books:[{name, chapters:[{chapter, verses:[{verse, text}]}]}]}`.
    Scrollmapper,
    /// thiagobodruk/damarals: array `[{abbrev, name, chapters:[[verso, ...], ...]}]`.
    ThiagobodrukArray,
}

/// Especificação de uma versão livre embarcável.
#[derive(Debug, Clone, Copy)]
struct TranslationSpec {
    /// Slug no banco.
    id: &'static str,
    /// Abreviação de exibição.
    abbrev: &'static str,
    /// Nome completo.
    name: &'static str,
    /// Idioma.
    lang: Lang,
    /// Texto da licença (gravado em `translations.license`).
    license: &'static str,
    /// URL fixada do dataset bruto.
    url: &'static str,
    /// Formato do arquivo bruto.
    format: Format,
    /// Contagem esperada de versículos (guarda contra drift).
    expected_verses: usize,
}

/// Registro de versões livres. **Nunca** adicionar versões protegidas aqui.
const SPECS: &[TranslationSpec] = &[
    TranslationSpec {
        id: "kjv",
        abbrev: "KJV",
        name: "King James Version",
        lang: Lang::En,
        license: "public-domain",
        url: "https://raw.githubusercontent.com/scrollmapper/bible_databases/master/formats/json/KJV.json",
        format: Format::Scrollmapper,
        expected_verses: 31_102,
    },
    TranslationSpec {
        id: "alm1911",
        abbrev: "ALM1911",
        name: "Almeida 1911",
        lang: Lang::Pt,
        license: "public-domain",
        url: "https://github.com/damarals/biblias/releases/download/v1.0.0/ALM1911.json",
        format: Format::ThiagobodrukArray,
        expected_verses: 31_101,
    },
];

fn spec(id: &str) -> Option<&'static TranslationSpec> {
    SPECS.iter().find(|s| s.id == id)
}

/// Um versículo já decomposto, pronto para inserção.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedVerse {
    book_number: u8,
    chapter: u16,
    verse: u16,
    text: String,
}

/// Opções da linha de comando `import`.
struct ImportOptions {
    versions: Vec<String>,
    db_path: Option<PathBuf>,
    seed_dir: PathBuf,
    force: bool,
    offline: bool,
}

/// Ponto de entrada do subcomando `import`.
pub fn run_import(args: &[String]) -> Result<()> {
    let opts = parse_args(args)?;
    if opts.versions.is_empty() {
        bail!("nenhuma versão informada — use `--version kjv,alm1911`");
    }

    let mut store = match &opts.db_path {
        Some(p) => Store::open(p).with_context(|| format!("abrindo banco em {}", p.display()))?,
        None => Store::open_default().context("abrindo banco no caminho padrão")?,
    };
    let db_path = opts
        .db_path
        .clone()
        .unwrap_or_else(|| Store::default_db_path().unwrap_or_default());
    println!("Banco: {}", db_path.display());

    for id in &opts.versions {
        let spec = spec(id).ok_or_else(|| {
            anyhow!(
                "versão `{id}` não é uma versão livre conhecida. Versões embarcáveis: {}. \
                 Versões protegidas NÃO podem ser importadas (ver DATA_SOURCES.md).",
                SPECS.iter().map(|s| s.id).collect::<Vec<_>>().join(", ")
            )
        })?;

        let bytes = obtain_bytes(spec, &opts)?;
        let verses = parse_dataset(spec, &bytes)
            .with_context(|| format!("parseando dataset `{}`", spec.id))?;

        if verses.len() < 30_000 {
            bail!(
                "`{}`: apenas {} versículos parseados (esperado ~{}); dataset incompleto?",
                spec.id,
                verses.len(),
                spec.expected_verses
            );
        }
        if verses.len() != spec.expected_verses {
            eprintln!(
                "aviso: `{}` tem {} versículos, esperado {} — possível atualização do dataset.",
                spec.id,
                verses.len(),
                spec.expected_verses
            );
        }

        let inserted = import_translation(store.conn_mut(), spec, &verses)
            .with_context(|| format!("inserindo `{}` no banco", spec.id))?;
        println!(
            "✓ {} ({}) — {} versículos importados.",
            spec.id, spec.lang, inserted
        );
    }

    Ok(())
}

/// Obtém os bytes do dataset: do arquivo em `seed_dir` ou baixando (se ausente).
fn obtain_bytes(spec: &TranslationSpec, opts: &ImportOptions) -> Result<Vec<u8>> {
    std::fs::create_dir_all(&opts.seed_dir)
        .with_context(|| format!("criando {}", opts.seed_dir.display()))?;
    let path = opts.seed_dir.join(format!("{}.json", spec.id));

    if path.exists() && !opts.force {
        return std::fs::read(&path).with_context(|| format!("lendo {}", path.display()));
    }
    if opts.offline {
        bail!(
            "`{}` ausente em {} e modo --offline ativo; baixe o arquivo manualmente de {}",
            spec.id,
            path.display(),
            spec.url
        );
    }
    let bytes = download(spec.url)?;
    std::fs::write(&path, &bytes).with_context(|| format!("gravando {}", path.display()))?;
    println!(
        "baixado {} ({} bytes) → {}",
        spec.id,
        bytes.len(),
        path.display()
    );
    Ok(bytes)
}

/// Baixa uma URL seguindo redirects (reqwest segue por padrão).
pub(crate) fn download(url: &str) -> Result<Vec<u8>> {
    println!("baixando {url} ...");
    let client = reqwest::blocking::Client::builder()
        .user_agent("biblia-cli-importer")
        .build()
        .context("criando cliente HTTP")?;
    let resp = client
        .get(url)
        .send()
        .with_context(|| format!("requisição GET {url}"))?
        .error_for_status()
        .with_context(|| format!("status HTTP de {url}"))?;
    let bytes = resp.bytes().context("lendo corpo da resposta")?;
    Ok(bytes.to_vec())
}

/// Decompõe os bytes brutos em versículos, conforme o formato.
fn parse_dataset(spec: &TranslationSpec, bytes: &[u8]) -> Result<Vec<ParsedVerse>> {
    let bytes = strip_bom(bytes);
    match spec.format {
        Format::Scrollmapper => parse_scrollmapper(bytes),
        Format::ThiagobodrukArray => parse_thiagobodruk(bytes),
    }
}

/// Remove um BOM UTF-8 inicial, se presente (alguns datasets têm).
fn strip_bom(bytes: &[u8]) -> &[u8] {
    bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes)
}

#[derive(Deserialize)]
struct SmRoot {
    books: Vec<SmBook>,
}
#[derive(Deserialize)]
struct SmBook {
    chapters: Vec<SmChapter>,
}
#[derive(Deserialize)]
struct SmChapter {
    chapter: u16,
    verses: Vec<SmVerse>,
}
#[derive(Deserialize)]
struct SmVerse {
    verse: u16,
    text: String,
}

fn parse_scrollmapper(bytes: &[u8]) -> Result<Vec<ParsedVerse>> {
    let root: SmRoot = serde_json::from_slice(bytes).context("JSON scrollmapper inválido")?;
    if root.books.len() != 66 {
        bail!("esperados 66 livros, encontrados {}", root.books.len());
    }
    let mut out = Vec::with_capacity(31_200);
    for (bi, book) in root.books.iter().enumerate() {
        let book_number = (bi + 1) as u8;
        for chapter in &book.chapters {
            for verse in &chapter.verses {
                let text = verse.text.trim().to_string();
                out.push(ParsedVerse {
                    book_number,
                    chapter: chapter.chapter,
                    verse: verse.verse,
                    text,
                });
            }
        }
    }
    Ok(out)
}

#[derive(Deserialize)]
struct TbBook {
    chapters: Vec<Vec<String>>,
}

fn parse_thiagobodruk(bytes: &[u8]) -> Result<Vec<ParsedVerse>> {
    let books: Vec<TbBook> = serde_json::from_slice(bytes).context("JSON thiagobodruk inválido")?;
    if books.len() != 66 {
        bail!("esperados 66 livros, encontrados {}", books.len());
    }
    let mut out = Vec::with_capacity(31_200);
    for (bi, book) in books.iter().enumerate() {
        let book_number = (bi + 1) as u8;
        for (ci, chapter) in book.chapters.iter().enumerate() {
            let chapter_number = (ci + 1) as u16;
            for (vi, text) in chapter.iter().enumerate() {
                out.push(ParsedVerse {
                    book_number,
                    chapter: chapter_number,
                    verse: (vi + 1) as u16,
                    text: text.trim().to_string(),
                });
            }
        }
    }
    Ok(out)
}

/// Insere uma tradução (substituindo a anterior, de forma idempotente).
fn import_translation(
    conn: &mut Connection,
    spec: &TranslationSpec,
    verses: &[ParsedVerse],
) -> Result<usize> {
    let tx = conn.transaction()?;

    // Limpeza idempotente. A FK ON DELETE CASCADE remove books/verses ao apagar
    // a tradução; verses_fts é tabela virtual (sem FK), apagada à mão.
    tx.execute(
        "DELETE FROM verses_fts WHERE translation_id = ?1",
        params![spec.id],
    )?;
    tx.execute("DELETE FROM translations WHERE id = ?1", params![spec.id])?;

    tx.execute(
        "INSERT INTO translations(id, abbrev, name, language, license, embeddable) \
         VALUES (?1, ?2, ?3, ?4, ?5, 1)",
        params![
            spec.id,
            spec.abbrev,
            spec.name,
            spec.lang.code(),
            spec.license
        ],
    )?;

    {
        let mut bstmt = tx.prepare(
            "INSERT INTO books(translation_id, number, name, abbrev, testament) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )?;
        for b in &BOOKS {
            let (name, abbrev) = match spec.lang {
                Lang::En => (b.name_en, b.abbrev_en),
                Lang::Pt => (b.name_pt, b.abbrev_pt),
            };
            bstmt.execute(params![spec.id, b.number, name, abbrev, b.testament.code()])?;
        }
    }

    let pb = ProgressBar::new(verses.len() as u64);
    pb.set_style(
        ProgressStyle::with_template("  {bar:40} {pos}/{len} versículos")
            .unwrap_or_else(|_| ProgressStyle::default_bar()),
    );
    {
        let mut vstmt = tx.prepare(
            "INSERT INTO verses(translation_id, book_number, chapter, verse, text) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )?;
        let mut fstmt = tx.prepare(
            "INSERT INTO verses_fts(text, translation_id, verse_id) VALUES (?1, ?2, ?3)",
        )?;
        for v in verses {
            vstmt.execute(params![spec.id, v.book_number, v.chapter, v.verse, v.text])?;
            let verse_id = tx.last_insert_rowid();
            fstmt.execute(params![v.text, spec.id, verse_id])?;
            pb.inc(1);
        }
    }
    pb.finish_and_clear();

    let count: i64 = tx.query_row(
        "SELECT count(*) FROM verses WHERE translation_id = ?1",
        params![spec.id],
        |r| r.get(0),
    )?;
    tx.commit()?;
    Ok(count as usize)
}

/// Parser de argumentos do subcomando `import`.
fn parse_args(args: &[String]) -> Result<ImportOptions> {
    let mut versions = Vec::new();
    let mut db_path = None;
    let mut seed_dir = PathBuf::from("data/seed");
    let mut force = false;
    let mut offline = false;

    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--version" | "-v" => {
                let val = it
                    .next()
                    .ok_or_else(|| anyhow!("--version requer um valor"))?;
                versions.extend(
                    val.split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty()),
                );
            }
            "--db" => {
                db_path = Some(PathBuf::from(
                    it.next().ok_or_else(|| anyhow!("--db requer um caminho"))?,
                ));
            }
            "--seed-dir" => {
                seed_dir = PathBuf::from(
                    it.next()
                        .ok_or_else(|| anyhow!("--seed-dir requer um caminho"))?,
                );
            }
            "--force" => force = true,
            "--offline" => offline = true,
            other => bail!("argumento desconhecido para `import`: {other}"),
        }
    }

    Ok(ImportOptions {
        versions,
        db_path,
        seed_dir,
        force,
        offline,
    })
}

/// Lista as versões livres disponíveis (para o `help`).
pub fn available_versions() -> String {
    SPECS
        .iter()
        .map(|s| format!("{} ({}, {})", s.id, s.lang, s.license))
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    // Fixtures mínimas com 66 "livros" para satisfazer a checagem de contagem.
    fn scrollmapper_fixture() -> Vec<u8> {
        let mut books = Vec::new();
        for bi in 0..66 {
            // livro 1 ganha 2 versículos reais; os demais 1 versículo placeholder.
            let verses = if bi == 0 {
                r#"[{"verse":1,"text":"  In the beginning God created the heaven and the earth.  "},{"verse":2,"text":"And the earth was without form."}]"#
            } else {
                r#"[{"verse":1,"text":"placeholder"}]"#
            };
            books.push(format!(
                r#"{{"name":"Book{bi}","chapters":[{{"chapter":1,"verses":{verses}}}]}}"#
            ));
        }
        format!(r#"{{"translation":"KJV","books":[{}]}}"#, books.join(",")).into_bytes()
    }

    fn thiagobodruk_fixture() -> Vec<u8> {
        let mut books = Vec::new();
        for bi in 0..66 {
            let chapters = if bi == 0 {
                r#"[["No princípio criou Deus os céus e a terra. ","E a terra era sem forma."]]"#
            } else {
                r#"[["placeholder"]]"#
            };
            books.push(format!(
                r#"{{"abbrev":"Bk{bi}","name":"Livro{bi}","chapters":{chapters}}}"#
            ));
        }
        format!("[{}]", books.join(",")).into_bytes()
    }

    #[test]
    fn parses_scrollmapper_shape_and_trims() {
        let v = parse_scrollmapper(&scrollmapper_fixture()).unwrap();
        assert_eq!(v.len(), 67); // 2 + 65*1
        assert_eq!(v[0].book_number, 1);
        assert_eq!(v[0].chapter, 1);
        assert_eq!(v[0].verse, 1);
        assert_eq!(
            v[0].text,
            "In the beginning God created the heaven and the earth."
        );
        assert_eq!(v[66].book_number, 66);
    }

    #[test]
    fn parses_thiagobodruk_positional_and_trims() {
        let v = parse_thiagobodruk(&thiagobodruk_fixture()).unwrap();
        assert_eq!(v.len(), 67);
        assert_eq!(v[0].book_number, 1);
        assert_eq!(v[0].chapter, 1);
        assert_eq!(v[0].verse, 1);
        assert_eq!(v[0].text, "No princípio criou Deus os céus e a terra.");
        assert_eq!(v[1].verse, 2);
    }

    #[test]
    fn strip_bom_removes_leading_marker() {
        let with_bom = [0xEF, 0xBB, 0xBF, b'[', b']'];
        assert_eq!(strip_bom(&with_bom), b"[]");
        assert_eq!(strip_bom(b"[]"), b"[]");
    }

    #[test]
    fn rejects_wrong_book_count() {
        assert!(parse_scrollmapper(br#"{"translation":"x","books":[]}"#).is_err());
    }

    #[test]
    fn import_and_query_roundtrip() {
        // Importa as fixtures num banco em memória e confere a leitura por SQL.
        let mut store = Store::open_in_memory().unwrap();
        let en = parse_scrollmapper(&scrollmapper_fixture()).unwrap();
        let spec_en = spec("kjv").unwrap();
        let n = import_translation(store.conn_mut(), spec_en, &en).unwrap();
        assert_eq!(n, 67);

        // Reimport idempotente: continua 67, não 134.
        let n2 = import_translation(store.conn_mut(), spec_en, &en).unwrap();
        assert_eq!(n2, 67);

        let text: String = store
            .conn()
            .query_row(
                "SELECT text FROM verses WHERE translation_id='kjv' AND book_number=1 AND chapter=1 AND verse=1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            text,
            "In the beginning God created the heaven and the earth."
        );

        // FTS5 indexado e acento-insensível para o lado PT.
        let pt = parse_thiagobodruk(&thiagobodruk_fixture()).unwrap();
        import_translation(store.conn_mut(), spec("alm1911").unwrap(), &pt).unwrap();
        let hits: i64 = store
            .conn()
            .query_row(
                "SELECT count(*) FROM verses_fts WHERE translation_id='alm1911' AND verses_fts MATCH 'ceus'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(hits, 1, "busca 'ceus' deveria casar 'céus'");

        // books: 66 por tradução, com nomes canônicos por idioma.
        let bname: String = store
            .conn()
            .query_row(
                "SELECT name FROM books WHERE translation_id='alm1911' AND number=43",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(bname, "João");
    }
}
