//! Testes de integração do subcomando `biblia tui` (somente caminhos que não
//! entram em raw mode — a TUI interativa exige um TTY real).

use std::path::PathBuf;

use assert_cmd::Command;
use biblia_core::store::Store;
use predicates::str::contains;
use tempfile::TempDir;

fn biblia() -> Command {
    let mut cmd = Command::cargo_bin("biblia").unwrap();
    cmd.env(
        "BIBLIA_CONFIG",
        std::env::temp_dir().join("biblia_absent_config.toml"),
    );
    cmd.env(
        "BIBLIA_SECRETS",
        std::env::temp_dir().join("biblia_absent_secrets.toml"),
    );
    cmd
}

fn empty_db() -> (TempDir, PathBuf) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("empty.sqlite");
    Store::open(&path).unwrap(); // banco migrado, sem versões
    (dir, path)
}

fn seeded_db() -> (TempDir, PathBuf) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("biblia.sqlite");
    let store = Store::open(&path).unwrap();
    store
        .conn()
        .execute(
            "INSERT INTO translations(id,abbrev,name,language,license,embeddable) \
             VALUES ('kjv','KJV','King James Version','en','public-domain',1)",
            [],
        )
        .unwrap();
    (dir, path)
}

#[test]
fn tui_with_empty_db_exits_not_found() {
    let (_dir, path) = empty_db();
    biblia()
        .args(["tui", "--db"])
        .arg(&path)
        .assert()
        .failure()
        .code(1)
        .stderr(contains("Nenhuma versão importada"));
}

#[test]
fn tui_unknown_version_exits_usage() {
    let (_dir, path) = seeded_db();
    biblia()
        .args(["tui", "--version", "zzz", "--db"])
        .arg(&path)
        .assert()
        .failure()
        .code(2)
        .stderr(contains("Versão desconhecida"));
}
