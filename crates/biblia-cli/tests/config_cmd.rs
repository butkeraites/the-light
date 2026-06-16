//! Testes de integração do comando `biblia config` (via `BIBLIA_CONFIG`).

use assert_cmd::Command;
use predicates::str::contains;
use tempfile::TempDir;

/// Comando `biblia` com `BIBLIA_CONFIG` apontando para um arquivo temporário.
fn biblia(cfg: &std::path::Path) -> Command {
    let mut cmd = Command::cargo_bin("biblia").unwrap();
    cmd.env("BIBLIA_CONFIG", cfg);
    cmd
}

#[test]
fn set_persists_and_get_reads_back() {
    let dir = TempDir::new().unwrap();
    let cfg = dir.path().join("config.toml");

    biblia(&cfg)
        .args(["config", "set", "versions", "kjv,alm1911"])
        .assert()
        .success()
        .stdout(contains("ok: versions = kjv,alm1911"));

    biblia(&cfg)
        .args(["config", "get", "versions"])
        .assert()
        .success()
        .stdout(contains("kjv,alm1911"));

    // O arquivo foi realmente criado.
    assert!(cfg.exists());
}

#[test]
fn list_shows_defaults_and_path() {
    let dir = TempDir::new().unwrap();
    let cfg = dir.path().join("config.toml");
    biblia(&cfg)
        .args(["config", "list"])
        .assert()
        .success()
        .stdout(contains("versions = kjv"))
        .stdout(contains("language = pt"))
        .stdout(contains("theme = auto"))
        .stdout(contains("font-size = none"));
}

#[test]
fn set_unknown_key_exits_usage() {
    let dir = TempDir::new().unwrap();
    let cfg = dir.path().join("config.toml");
    biblia(&cfg)
        .args(["config", "set", "nope", "x"])
        .assert()
        .failure()
        .code(2)
        .stderr(contains("chave desconhecida"));
}

#[test]
fn set_bad_value_exits_usage() {
    let dir = TempDir::new().unwrap();
    let cfg = dir.path().join("config.toml");
    biblia(&cfg)
        .args(["config", "set", "language", "klingon"])
        .assert()
        .failure()
        .code(2)
        .stderr(contains("valor inválido"));
}

#[test]
fn get_unknown_key_exits_usage() {
    let dir = TempDir::new().unwrap();
    let cfg = dir.path().join("config.toml");
    biblia(&cfg)
        .args(["config", "get", "nope"])
        .assert()
        .failure()
        .code(2)
        .stderr(contains("Chave desconhecida"));
}

#[test]
fn config_versions_drives_read_default() {
    // `read` sem --version deve usar a primeira versão do config.
    let dir = TempDir::new().unwrap();
    let cfg = dir.path().join("config.toml");
    let db = dir.path().join("biblia.sqlite");
    {
        let store = biblia_core::store::Store::open(&db).unwrap();
        let conn = store.conn();
        conn.execute(
            "INSERT INTO translations(id,abbrev,name,language,license,embeddable) \
             VALUES ('alm','ALM','Almeida','pt','public-domain',1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO verses(translation_id,book_number,chapter,verse,text) \
             VALUES ('alm',43,3,16,'Porque Deus amou o mundo')",
            [],
        )
        .unwrap();
    }
    biblia(&cfg)
        .args(["config", "set", "versions", "alm"])
        .assert()
        .success();
    // Sem --version: usa 'alm' do config.
    biblia(&cfg)
        .args(["read", "John 3:16", "--db"])
        .arg(&db)
        .assert()
        .success()
        .stdout(contains("Porque Deus amou o mundo"));
}
