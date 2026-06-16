//! Testes de integração do comando `biblia config` (via `BIBLIA_CONFIG`).

use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use tempfile::TempDir;

/// Comando `biblia` com `BIBLIA_CONFIG`/`BIBLIA_SECRETS` em arquivos temporários
/// no mesmo diretório do `cfg`.
fn biblia(cfg: &std::path::Path) -> Command {
    let mut cmd = Command::cargo_bin("biblia").unwrap();
    cmd.env("BIBLIA_CONFIG", cfg);
    let secrets = cfg.parent().unwrap().join("secrets.toml");
    cmd.env("BIBLIA_SECRETS", secrets);
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
fn set_key_persists_without_leaking_value() {
    let dir = TempDir::new().unwrap();
    let cfg = dir.path().join("config.toml");

    let out = biblia(&cfg)
        .args(["config", "set-key", "anthropic", "sk-ant-secret-123"])
        .assert()
        .success()
        .get_output()
        .clone();
    let stdout = String::from_utf8(out.stdout).unwrap();
    // A chave NUNCA é ecoada.
    assert!(
        !stdout.contains("sk-ant-secret-123"),
        "chave vazou no stdout: {stdout}"
    );
    assert!(stdout.contains("gravada"));

    biblia(&cfg)
        .args(["config", "keys"])
        .assert()
        .success()
        .stdout(contains("anthropic"))
        .stdout(contains("sk-ant-secret-123").not());

    // O secrets.toml existe e não está no config.toml.
    let secrets = dir.path().join("secrets.toml");
    assert!(secrets.exists());
    let cfg_content = std::fs::read_to_string(&cfg).unwrap_or_default();
    assert!(!cfg_content.contains("sk-ant-secret-123"));

    biblia(&cfg)
        .args(["config", "remove-key", "anthropic"])
        .assert()
        .success()
        .stdout(contains("removida"));
}

#[test]
fn set_key_unknown_provider_is_usage_error() {
    let dir = TempDir::new().unwrap();
    let cfg = dir.path().join("config.toml");
    biblia(&cfg)
        .args(["config", "set-key", "skynet", "x"])
        .assert()
        .failure()
        .code(2)
        .stderr(contains("Provedor desconhecido"));
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
        .stderr(contains("Chave desconhecida"))
        .stderr(contains("provider")); // a lista de chaves válidas inclui provider
}

#[test]
fn provider_is_a_valid_config_key() {
    let dir = TempDir::new().unwrap();
    let cfg = dir.path().join("config.toml");
    biblia(&cfg)
        .args(["config", "set", "provider", "anthropic"])
        .assert()
        .success();
    biblia(&cfg)
        .args(["config", "get", "provider"])
        .assert()
        .success()
        .stdout(contains("anthropic"));
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
