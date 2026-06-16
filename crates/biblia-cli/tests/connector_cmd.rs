//! Testes de integração dos conectores de versões protegidas (sem rede).
//!
//! Exercitam a configuração e a resolução até o ponto anterior à chamada HTTP
//! (ex.: erro claro sem chave). Nenhum teste faz chamada de rede real.

use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use tempfile::TempDir;

fn biblia(home: &std::path::Path) -> Command {
    let mut cmd = Command::cargo_bin("biblia").unwrap();
    cmd.env("BIBLIA_CONFIG", home.join("config.toml"));
    cmd.env("BIBLIA_SECRETS", home.join("secrets.toml"));
    cmd.env("BIBLIA_DATA_DIR", home.join("data"));
    cmd
}

/// Banco vazio (só schema) — basta para abrir o store.
fn empty_db(dir: &std::path::Path) -> std::path::PathBuf {
    let db = dir.join("biblia.sqlite");
    biblia_core::store::Store::open(&db).unwrap();
    db
}

#[test]
fn connector_add_list_remove_roundtrip() {
    let dir = TempDir::new().unwrap();
    biblia(dir.path())
        .args([
            "config",
            "connector",
            "add",
            "ara",
            "--kind",
            "apibible",
            "--bible-id",
            "abc123",
            "--name",
            "Almeida Revista e Atualizada",
            "--abbrev",
            "ARA",
            "--lang",
            "pt",
        ])
        .assert()
        .success()
        .stdout(contains("conector `ara`"));

    biblia(dir.path())
        .args(["config", "connector", "list"])
        .assert()
        .success()
        .stdout(contains("ara"))
        .stdout(contains("apibible"))
        .stdout(contains("bible_id=abc123"));

    biblia(dir.path())
        .args(["config", "connector", "remove", "ara"])
        .assert()
        .success()
        .stdout(contains("removido"));

    biblia(dir.path())
        .args(["config", "connector", "list"])
        .assert()
        .success()
        .stdout(contains("Nenhum conector"));
}

#[test]
fn apibible_requires_bible_id() {
    let dir = TempDir::new().unwrap();
    biblia(dir.path())
        .args([
            "config",
            "connector",
            "add",
            "ara",
            "--kind",
            "apibible",
            "--name",
            "ARA",
            "--abbrev",
            "ARA",
        ])
        .assert()
        .failure()
        .code(2)
        .stderr(contains("bible-id"));
}

#[test]
fn connector_unknown_kind_is_usage_error() {
    let dir = TempDir::new().unwrap();
    biblia(dir.path())
        .args([
            "config",
            "connector",
            "add",
            "x",
            "--kind",
            "carrier-pigeon",
            "--name",
            "X",
            "--abbrev",
            "X",
        ])
        .assert()
        .failure()
        .code(2)
        .stderr(contains("inválido"));
}

#[test]
fn read_protected_without_key_fails_clearly_no_network() {
    let dir = TempDir::new().unwrap();
    let db = empty_db(dir.path());
    // Configura o conector, mas NÃO define a chave → erro antes de qualquer rede.
    biblia(dir.path())
        .args([
            "config",
            "connector",
            "add",
            "esv",
            "--kind",
            "esv",
            "--name",
            "English Standard Version",
            "--abbrev",
            "ESV",
            "--lang",
            "en",
        ])
        .assert()
        .success();

    biblia(dir.path())
        .args(["read", "John 3:16", "--version", "esv", "--db"])
        .arg(&db)
        .assert()
        .failure()
        .stderr(contains("sem chave"))
        .stderr(contains("set-key esv"));
}

#[test]
fn read_unknown_version_without_connector() {
    let dir = TempDir::new().unwrap();
    let db = empty_db(dir.path());
    // Sem conector e sem versão local → versão desconhecida (não trava em rede).
    biblia(dir.path())
        .args(["read", "John 3:16", "--version", "nvi", "--db"])
        .arg(&db)
        .assert()
        .failure()
        .stderr(contains("versão desconhecida").or(contains("Nenhuma versão importada")));
}

#[test]
fn set_key_accepts_connector_providers() {
    let dir = TempDir::new().unwrap();
    biblia(dir.path())
        .args(["config", "set-key", "apibible", "tok-123"])
        .assert()
        .success()
        .stdout(contains("gravada"));
    biblia(dir.path())
        .args(["config", "keys"])
        .assert()
        .success()
        .stdout(contains("apibible"))
        .stdout(contains("tok-123").not());
}
