//! Testes de integração do comando `light note` e da exibição na leitura.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use predicates::str::contains;
use tempfile::TempDir;
use the_light_core::store::Store;

fn fixture() -> (TempDir, PathBuf, PathBuf) {
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("biblia.sqlite");
    {
        let store = Store::open(&db).unwrap();
        store
            .conn()
            .execute(
                "INSERT INTO translations(id,abbrev,name,language,license,embeddable) \
                 VALUES ('kjv','KJV','King James Version','en','public-domain',1)",
                [],
            )
            .unwrap();
        store
            .conn()
            .execute(
                "INSERT INTO verses(translation_id,book_number,chapter,verse,text) \
                 VALUES ('kjv',43,3,16,'For God so loved the world')",
                [],
            )
            .unwrap();
    }
    let data_dir = dir.path().join("data");
    (dir, db, data_dir)
}

fn light(data_dir: &Path) -> Command {
    let mut cmd = Command::cargo_bin("light").unwrap();
    cmd.env("LIGHT_DATA_DIR", data_dir);
    cmd.env("LIGHT_CONFIG", data_dir.join("config.toml"));
    cmd
}

/// Cria um "editor" falso que escreve um Markdown fixo no arquivo recebido.
fn fake_editor(dir: &Path) -> PathBuf {
    let script = dir.join("fake-editor.sh");
    std::fs::write(
        &script,
        "#!/bin/sh\ncat > \"$1\" <<'EOF'\n# Estudo\n\nConteúdo via editor.\nEOF\n",
    )
    .unwrap();
    let mut perms = std::fs::metadata(&script).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&script, perms).unwrap();
    script
}

#[test]
fn add_inline_show_list_remove() {
    let (_dir, _db, data_dir) = fixture();

    light(&data_dir)
        .args(["note", "add", "Jo 3.16", "Texto **forte** sobre amor."])
        .assert()
        .success()
        .stdout(contains("Nota salva"));

    // Um arquivo .md por nota.
    assert!(data_dir.join("notes").join("John_3.16.md").exists());

    light(&data_dir)
        .args(["note", "show", "Jo 3.16"])
        .assert()
        .success()
        .stdout(contains("forte"))
        .stdout(contains("João 3.16"));

    light(&data_dir)
        .args(["note", "list"])
        .assert()
        .success()
        .stdout(contains("João 3.16"))
        .stdout(contains("Texto"));

    light(&data_dir)
        .args(["note", "remove", "Jo 3.16"])
        .assert()
        .success()
        .stdout(contains("removida"));

    light(&data_dir)
        .args(["note", "list"])
        .assert()
        .success()
        .stdout(contains("Nenhuma nota"));
}

#[test]
fn add_via_editor_writes_note() {
    let (dir, _db, data_dir) = fixture();
    let editor = fake_editor(dir.path());

    light(&data_dir)
        .env("EDITOR", &editor)
        .args(["note", "add", "Sl 23"])
        .assert()
        .success()
        .stdout(contains("Nota salva"));

    light(&data_dir)
        .args(["note", "show", "Sl 23"])
        .assert()
        .success()
        .stdout(contains("Estudo"))
        .stdout(contains("Conteúdo via editor"));
}

#[test]
fn show_missing_exits_not_found() {
    let (_dir, _db, data_dir) = fixture();
    light(&data_dir)
        .args(["note", "show", "Jo 3.16"])
        .assert()
        .failure()
        .code(1)
        .stdout(contains("Sem nota"));
}

#[test]
fn note_appears_in_reading() {
    let (_dir, db, data_dir) = fixture();
    light(&data_dir)
        .args(["note", "add", "Jo 3.16", "Comentário do versículo."])
        .assert()
        .success();

    light(&data_dir)
        .args(["read", "John 3:16", "--version", "kjv", "--db"])
        .arg(&db)
        .assert()
        .success()
        .stdout(contains("Notas:"))
        .stdout(contains("Comentário do versículo"));
}

#[test]
fn invalid_reference_exits_usage() {
    let (_dir, _db, data_dir) = fixture();
    light(&data_dir)
        .args(["note", "add", "NotARef!!!", "x"])
        .assert()
        .failure()
        .code(2)
        .stderr(contains("Referência inválida"));
}
