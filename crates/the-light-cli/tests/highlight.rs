//! Testes de integração do comando `light highlight` e da exibição na leitura.

use std::path::{Path, PathBuf};

use assert_cmd::Command;
use predicates::str::contains;
use tempfile::TempDir;
use the_light_core::store::Store;

/// Cria um banco temporário (KJV, João 3.16-17) e devolve (tmp, db_path, data_dir).
fn fixture() -> (TempDir, PathBuf, PathBuf) {
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("biblia.sqlite");
    {
        let store = Store::open(&db).unwrap();
        let conn = store.conn();
        conn.execute(
            "INSERT INTO translations(id,abbrev,name,language,license,embeddable) \
             VALUES ('kjv','KJV','King James Version','en','public-domain',1)",
            [],
        )
        .unwrap();
        for (v, t) in [(16, "For God so loved the world"), (17, "For God sent not")] {
            conn.execute(
                "INSERT INTO verses(translation_id,book_number,chapter,verse,text) \
                 VALUES ('kjv',43,3,?1,?2)",
                rusqlite::params![v as i64, t],
            )
            .unwrap();
        }
    }
    let data_dir = dir.path().join("data");
    (dir, db, data_dir)
}

/// `light` com config/dados isolados no diretório de dados dado.
fn light(data_dir: &Path) -> Command {
    let mut cmd = Command::cargo_bin("light").unwrap();
    cmd.env("LIGHT_DATA_DIR", data_dir);
    cmd.env("LIGHT_CONFIG", data_dir.join("config.toml"));
    cmd
}

#[test]
fn add_persists_list_and_remove() {
    let (_dir, _db, data_dir) = fixture();

    light(&data_dir)
        .args([
            "highlight",
            "add",
            "Jo 3.16",
            "--color",
            "yellow",
            "--tag",
            "salvação",
        ])
        .assert()
        .success()
        .stdout(contains("Marcado"))
        .stdout(contains("[salvação]"));

    // highlights.json foi criado no diretório de dados.
    assert!(data_dir.join("highlights.json").exists());

    light(&data_dir)
        .args(["highlight", "list"])
        .assert()
        .success()
        .stdout(contains("yellow"))
        .stdout(contains("[salvação]"));

    light(&data_dir)
        .args(["highlight", "remove", "Jo 3.16"])
        .assert()
        .success()
        .stdout(contains("Removida"));

    light(&data_dir)
        .args(["highlight", "list"])
        .assert()
        .success()
        .stdout(contains("Nenhuma marcação"));
}

#[test]
fn highlight_appears_in_reading() {
    let (_dir, db, data_dir) = fixture();
    light(&data_dir)
        .args([
            "highlight",
            "add",
            "Jo 3.16",
            "--color",
            "green",
            "--tag",
            "fé",
        ])
        .assert()
        .success();

    // Ao ler a passagem, a marcação aparece no rodapé.
    light(&data_dir)
        .args(["read", "John 3:16-17", "--version", "kjv", "--db"])
        .arg(&db)
        .assert()
        .success()
        .stdout(contains("Marcações:"))
        .stdout(contains("green"))
        .stdout(contains("[fé]"));
}

#[test]
fn unknown_color_exits_usage() {
    let (_dir, _db, data_dir) = fixture();
    light(&data_dir)
        .args(["highlight", "add", "Jo 3.16", "--color", "chartreuse"])
        .assert()
        .failure()
        .code(2)
        .stderr(contains("Cor desconhecida"));
}

#[test]
fn invalid_reference_exits_usage() {
    let (_dir, _db, data_dir) = fixture();
    light(&data_dir)
        .args(["highlight", "add", "NotARef!!!"])
        .assert()
        .failure()
        .code(2)
        .stderr(contains("Referência inválida"));
}
