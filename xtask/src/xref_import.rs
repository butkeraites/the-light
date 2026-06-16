//! Importador das referências cruzadas (OpenBible.info / TSK).
//!
//! Fonte: TSV de `cross-references` da OpenBible.info (CC-BY — atribuição
//! obrigatória; ver `DATA_SOURCES.md`). Usamos o espelho raw do
//! scrollmapper/bible_databases (mesmo schema, single-file, sem zip).
//!
//! Colunas: `From Verse` (OSIS `Gen.1.1`), `To Verse` (OSIS único ou intervalo
//! `John.1.1-John.1.3`), `Votes` (inteiro, pode ser negativo). Os códigos OSIS
//! são resolvidos por [`the_light_core::reference::book_number`].

use anyhow::{bail, Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
use rusqlite::{params, Connection};
use std::path::PathBuf;
use the_light_core::reference::book_number;
use the_light_core::store::Store;

const XREF_URL: &str =
    "https://raw.githubusercontent.com/scrollmapper/bible_databases/master/sources/extras/cross_references.txt";

/// Uma linha já decomposta de referência cruzada.
#[derive(Debug, Clone, PartialEq, Eq)]
struct XrefRow {
    from_book: u8,
    from_chapter: u16,
    from_verse: u16,
    to_book: u8,
    to_chapter: u16,
    to_verse_start: u16,
    to_verse_end: u16,
    votes: i64,
}

/// Decompõe um id OSIS `Book.Chapter.Verse` em `(livro, capítulo, versículo)`.
fn parse_osis(s: &str) -> Option<(u8, u16, u16)> {
    let mut parts = s.trim().split('.');
    let book = parts.next()?;
    let chapter = parts.next()?;
    let verse = parts.next()?;
    if parts.next().is_some() {
        return None;
    }
    Some((
        book_number(book)?,
        chapter.parse().ok()?,
        verse.parse().ok()?,
    ))
}

/// Decompõe a coluna `To Verse` (id único ou intervalo `A-B`).
/// Para intervalos que cruzam capítulo/livro, mantém livro/capítulo do início e
/// degrada o fim para versículo único (limitação documentada).
fn parse_to(to: &str) -> Option<(u8, u16, u16, u16)> {
    match to.split_once('-') {
        Some((start, end)) => {
            let (tb, tc, ts) = parse_osis(start)?;
            let (eb, ec, ev) = parse_osis(end)?;
            let te = if eb == tb && ec == tc && ev >= ts {
                ev
            } else {
                ts
            };
            Some((tb, tc, ts, te))
        }
        None => {
            let (tb, tc, ts) = parse_osis(to)?;
            Some((tb, tc, ts, ts))
        }
    }
}

/// Decompõe uma linha do TSV. `None` para cabeçalho/linhas inválidas.
fn parse_line(line: &str) -> Option<XrefRow> {
    let mut cols = line.split('\t');
    let from = cols.next()?;
    let to = cols.next()?;
    let votes = cols.next()?.trim().parse::<i64>().ok()?;
    let (from_book, from_chapter, from_verse) = parse_osis(from)?;
    let (to_book, to_chapter, to_verse_start, to_verse_end) = parse_to(to)?;
    Some(XrefRow {
        from_book,
        from_chapter,
        from_verse,
        to_book,
        to_chapter,
        to_verse_start,
        to_verse_end,
        votes,
    })
}

/// Parser do conteúdo completo do TSV.
fn parse_tsv(data: &str) -> Vec<XrefRow> {
    data.lines().filter_map(parse_line).collect()
}

/// Insere as referências cruzadas (substituindo as anteriores), transacional.
fn import_rows(conn: &mut Connection, rows: &[XrefRow]) -> Result<usize> {
    let tx = conn.transaction()?;
    tx.execute("DELETE FROM cross_references", [])?;
    let pb = ProgressBar::new(rows.len() as u64);
    pb.set_style(
        ProgressStyle::with_template("  {bar:40} {pos}/{len} xrefs")
            .unwrap_or_else(|_| ProgressStyle::default_bar()),
    );
    {
        let mut stmt = tx.prepare(
            "INSERT INTO cross_references \
             (from_book,from_chapter,from_verse,to_book,to_chapter,to_verse_start,to_verse_end,votes) \
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
        )?;
        for r in rows {
            stmt.execute(params![
                r.from_book,
                r.from_chapter,
                r.from_verse,
                r.to_book,
                r.to_chapter,
                r.to_verse_start,
                r.to_verse_end,
                r.votes,
            ])?;
            pb.inc(1);
        }
    }
    pb.finish_and_clear();
    let count: i64 = tx.query_row("SELECT count(*) FROM cross_references", [], |r| r.get(0))?;
    tx.commit()?;
    Ok(count as usize)
}

/// Ponto de entrada do subcomando `import-xref`.
pub fn run(args: &[String]) -> Result<()> {
    let mut db_path: Option<PathBuf> = None;
    let mut seed_dir = PathBuf::from("data/seed");
    let mut force = false;
    let mut offline = false;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--db" => db_path = Some(PathBuf::from(it.next().context("--db requer caminho")?)),
            "--seed-dir" => {
                seed_dir = PathBuf::from(it.next().context("--seed-dir requer caminho")?)
            }
            "--force" => force = true,
            "--offline" => offline = true,
            other => bail!("argumento desconhecido para `import-xref`: {other}"),
        }
    }

    std::fs::create_dir_all(&seed_dir)?;
    let path = seed_dir.join("cross_references.txt");
    let data = if path.exists() && !force {
        std::fs::read_to_string(&path).with_context(|| format!("lendo {}", path.display()))?
    } else if offline {
        bail!(
            "{} ausente e --offline ativo; baixe de {}",
            path.display(),
            XREF_URL
        );
    } else {
        let bytes = crate::import::download(XREF_URL)?;
        std::fs::write(&path, &bytes).with_context(|| format!("gravando {}", path.display()))?;
        println!("baixado xrefs ({} bytes) → {}", bytes.len(), path.display());
        String::from_utf8(bytes).context("TSV de xrefs não é UTF-8")?
    };

    let rows = parse_tsv(&data);
    if rows.len() < 300_000 {
        bail!(
            "apenas {} referências cruzadas parseadas (esperado ~344.799); fonte incompleta?",
            rows.len()
        );
    }

    let mut store = match &db_path {
        Some(p) => Store::open(p)?,
        None => Store::open_default()?,
    };
    let n = import_rows(store.conn_mut(), &rows)?;
    println!("✓ {n} referências cruzadas importadas (OpenBible.info, CC-BY).");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_osis_resolves_codes() {
        assert_eq!(parse_osis("Gen.1.1"), Some((1, 1, 1)));
        assert_eq!(parse_osis("Ps.23.1"), Some((19, 23, 1))); // 'Ps' singular
        assert_eq!(parse_osis("1Cor.13.4"), Some((46, 13, 4)));
        assert_eq!(parse_osis("Rom.3.23"), Some((45, 3, 23)));
        assert_eq!(parse_osis("Rev.22.21"), Some((66, 22, 21)));
        assert_eq!(parse_osis("Nope.1.1"), None);
        assert_eq!(parse_osis("From Verse"), None); // cabeçalho
    }

    #[test]
    fn parse_to_single_and_range() {
        assert_eq!(parse_to("John.1.1"), Some((43, 1, 1, 1)));
        assert_eq!(parse_to("John.1.1-John.1.3"), Some((43, 1, 1, 3)));
        // Intervalo cruzando capítulo → degrada para único.
        assert_eq!(parse_to("John.1.50-John.2.2"), Some((43, 1, 50, 50)));
    }

    #[test]
    fn parse_line_skips_header_and_parses_rows() {
        let header = "From Verse\tTo Verse\tVotes\t#www.openbible.info CC-BY";
        assert_eq!(parse_line(header), None);
        let row = parse_line("Rom.3.23\tRom.6.23\t50").unwrap();
        assert_eq!(row.from_book, 45);
        assert_eq!(row.from_verse, 23);
        assert_eq!(row.to_book, 45);
        assert_eq!(row.to_chapter, 6);
        assert_eq!(row.votes, 50);
        // Voto negativo é preservado.
        assert_eq!(parse_line("Rom.3.23\tRom.3.9\t-5").unwrap().votes, -5);
    }

    #[test]
    fn parse_tsv_filters_invalid() {
        let tsv = "From Verse\tTo Verse\tVotes\nRom.3.23\tRom.6.23\t50\nlixo\nGen.1.1\tJohn.1.1-John.1.3\t12\n";
        let rows = parse_tsv(tsv);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1].to_verse_end, 3);
    }

    #[test]
    fn import_rows_is_idempotent() {
        let mut store = Store::open_in_memory().unwrap();
        let rows = vec![XrefRow {
            from_book: 45,
            from_chapter: 3,
            from_verse: 23,
            to_book: 45,
            to_chapter: 6,
            to_verse_start: 23,
            to_verse_end: 23,
            votes: 50,
        }];
        assert_eq!(import_rows(store.conn_mut(), &rows).unwrap(), 1);
        // Reimportar não duplica.
        assert_eq!(import_rows(store.conn_mut(), &rows).unwrap(), 1);
    }
}
