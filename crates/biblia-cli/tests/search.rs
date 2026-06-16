//! Testes de integração do comando `biblia search`.

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
             VALUES ('alm','ALM','Almeida','pt','public-domain',1)",
            [],
        )
        .unwrap();
        let verses = [
            (45, 3, 24, "sendo justificados gratuitamente pela sua graça"),
            (49, 2, 8, "Porque pela graça sois salvos, por meio da fé"),
            (43, 1, 14, "cheio de graça e de verdade"),
        ];
        for (i, (b, c, v, t)) in verses.iter().enumerate() {
            let id = (i + 1) as i64;
            conn.execute(
                "INSERT INTO verses(id,translation_id,book_number,chapter,verse,text) \
                 VALUES (?1,'alm',?2,?3,?4,?5)",
                rusqlite::params![id, *b as i64, *c as i64, *v as i64, t],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO verses_fts(text, translation_id, verse_id) VALUES (?1,'alm',?2)",
                rusqlite::params![t, id],
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
    cmd
}

#[test]
fn search_without_accent_finds_accented_and_highlights() {
    let (_dir, path) = seeded_db();
    biblia()
        .args(["search", "graca", "--version", "alm", "--db"])
        .arg(&path)
        .assert()
        .success()
        .stdout(contains("3 resultado"))
        .stdout(contains("[graça]")) // destaque
        .stdout(contains("Romanos 3.24"));
}

#[test]
fn search_book_filter_narrows_results() {
    let (_dir, path) = seeded_db();
    biblia()
        .args([
            "search",
            "graca",
            "--version",
            "alm",
            "--book",
            "Romanos",
            "--db",
        ])
        .arg(&path)
        .assert()
        .success()
        .stdout(contains("1 resultado"))
        .stdout(contains("Romanos 3.24"))
        .stdout(contains("Efésios").not());
}

#[test]
fn search_limit_caps_results() {
    let (_dir, path) = seeded_db();
    let out = biblia()
        .args([
            "search",
            "graca",
            "--version",
            "alm",
            "--limit",
            "2",
            "--db",
        ])
        .arg(&path)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(out).unwrap();
    assert_eq!(text.matches("[graça]").count(), 2);
}

#[test]
fn search_no_results_exits_one() {
    let (_dir, path) = seeded_db();
    biblia()
        .args(["search", "zzzznotfound", "--version", "alm", "--db"])
        .arg(&path)
        .assert()
        .failure()
        .code(1)
        .stdout(contains("Nenhum resultado"));
}

#[test]
fn search_unknown_book_exits_usage() {
    let (_dir, path) = seeded_db();
    biblia()
        .args([
            "search",
            "graca",
            "--version",
            "alm",
            "--book",
            "Xyz",
            "--db",
        ])
        .arg(&path)
        .assert()
        .failure()
        .code(2)
        .stderr(contains("Livro desconhecido"));
}

#[test]
fn search_empty_db_exits_not_found() {
    // Banco sem versões importadas = recurso ausente → código 1 (igual ao read).
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("empty.sqlite");
    Store::open(&path).unwrap(); // cria banco migrado, porém vazio
    biblia()
        .args(["search", "graca", "--db"])
        .arg(&path)
        .assert()
        .failure()
        .code(1)
        .stderr(contains("Nenhuma versão importada"));
}

#[test]
fn search_unknown_version_exits_usage() {
    let (_dir, path) = seeded_db();
    biblia()
        .args(["search", "graca", "--version", "zzz", "--db"])
        .arg(&path)
        .assert()
        .failure()
        .code(2)
        .stderr(contains("Versão desconhecida"));
}
