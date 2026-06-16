//! Testes de integração do comando `biblia xref`.

use std::path::PathBuf;

use assert_cmd::Command;
use biblia_core::store::Store;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use tempfile::TempDir;

fn seeded_db() -> (TempDir, PathBuf) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("biblia.sqlite");
    {
        let store = Store::open(&path).unwrap();
        let conn = store.conn();
        conn.execute(
            "INSERT INTO translations(id,abbrev,name,language,license,embeddable) \
             VALUES ('kjv','KJV','King James Version','en','public-domain',1)",
            [],
        )
        .unwrap();
        // Versículos de destino.
        for (b, c, v, t) in [
            (45, 6, 23, "For the wages of sin is death"),
            (45, 3, 9, "they are all under sin"),
        ] {
            conn.execute(
                "INSERT INTO verses(translation_id,book_number,chapter,verse,text) \
                 VALUES ('kjv',?1,?2,?3,?4)",
                rusqlite::params![b, c, v, t],
            )
            .unwrap();
        }
        // Referências cruzadas a partir de Romanos 3:23.
        for (tb, tc, ts, te, votes) in [(45, 6, 23, 23, 50i64), (45, 3, 9, 9, -5i64)] {
            conn.execute(
                "INSERT INTO cross_references \
                 (from_book,from_chapter,from_verse,to_book,to_chapter,to_verse_start,to_verse_end,votes) \
                 VALUES (45,3,23,?1,?2,?3,?4,?5)",
                rusqlite::params![tb, tc, ts, te, votes],
            )
            .unwrap();
        }
    }
    (dir, path)
}

fn biblia() -> Command {
    let mut cmd = Command::cargo_bin("biblia").unwrap();
    cmd.env(
        "BIBLIA_CONFIG",
        std::env::temp_dir().join("biblia_absent_config.toml"),
    );
    cmd.env(
        "BIBLIA_DATA_DIR",
        std::env::temp_dir().join("biblia_absent_data_dir"),
    );
    cmd
}

#[test]
fn lists_related_verses_with_text_and_hides_disputed() {
    let (_dir, path) = seeded_db();
    biblia()
        .args(["xref", "Rm 3.23", "--version", "kjv", "--db"])
        .arg(&path)
        .assert()
        .success()
        .stdout(contains("Romanos 6.23")) // idioma padrão pt
        .stdout(contains("For the wages of sin"))
        .stdout(contains("(50)"))
        // A disputada (votos -5) não aparece por padrão.
        .stdout(contains("Romanos 3.9").not());
}

#[test]
fn min_votes_includes_disputed() {
    let (_dir, path) = seeded_db();
    biblia()
        .args([
            "xref",
            "Rm 3.23",
            "--version",
            "kjv",
            "--min-votes",
            "-100",
            "--db",
        ])
        .arg(&path)
        .assert()
        .success()
        .stdout(contains("Romanos 6.23"))
        .stdout(contains("Romanos 3.9"));
}

#[test]
fn whole_chapter_reference_is_usage_error() {
    let (_dir, path) = seeded_db();
    biblia()
        .args(["xref", "Sl 23", "--db"])
        .arg(&path)
        .assert()
        .failure()
        .code(2)
        .stderr(contains("Informe um versículo"));
}

#[test]
fn no_xrefs_exits_not_found() {
    let (_dir, path) = seeded_db();
    biblia()
        .args(["xref", "Gn 1.1", "--db"])
        .arg(&path)
        .assert()
        .failure()
        .code(1)
        .stdout(contains("Nenhuma referência cruzada"));
}
