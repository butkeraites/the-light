//! Testes de integração do comando `biblia export`.

use std::path::Path;

use assert_cmd::Command;
use predicates::str::contains;
use tempfile::TempDir;

fn biblia(data_dir: &Path) -> Command {
    let mut cmd = Command::cargo_bin("biblia").unwrap();
    cmd.env("BIBLIA_DATA_DIR", data_dir);
    cmd.env("BIBLIA_CONFIG", data_dir.join("config.toml"));
    cmd
}

#[test]
fn export_notes_to_markdown_stdout() {
    let dir = TempDir::new().unwrap();
    let data_dir = dir.path();
    biblia(data_dir)
        .args(["note", "add", "Jo 3.16", "Comentário **central**."])
        .assert()
        .success();
    biblia(data_dir)
        .args(["note", "add", "Gn 1.1", "No princípio."])
        .assert()
        .success();

    biblia(data_dir)
        .args(["export", "notes"])
        .assert()
        .success()
        .stdout(contains("# Notas"))
        // Ordenado por referência canônica: Gênesis antes de João.
        .stdout(contains("## Gênesis 1.1"))
        .stdout(contains("## João 3.16"))
        .stdout(contains("Comentário **central**."));
}

#[test]
fn export_notes_to_file() {
    let dir = TempDir::new().unwrap();
    let data_dir = dir.path();
    let out = dir.path().join("notas.md");
    biblia(data_dir)
        .args(["note", "add", "Sl 23", "O Senhor é o meu pastor."])
        .assert()
        .success();

    biblia(data_dir)
        .args(["export", "notes", "--format", "md", "--output"])
        .arg(&out)
        .assert()
        .success()
        .stdout(contains("Exportado para"));

    let content = std::fs::read_to_string(&out).unwrap();
    assert!(content.contains("## Salmos 23"));
    assert!(content.contains("O Senhor é o meu pastor."));
}

#[test]
fn export_notes_empty_says_so() {
    let dir = TempDir::new().unwrap();
    biblia(dir.path())
        .args(["export", "notes"])
        .assert()
        .success()
        .stdout(contains("Nenhuma nota"));
}

#[test]
fn export_unknown_target_is_usage_error() {
    let dir = TempDir::new().unwrap();
    biblia(dir.path())
        .args(["export", "planilhas"])
        .assert()
        .failure()
        .code(2)
        .stderr(contains("Não sei exportar"));
}

#[test]
fn export_unknown_format_is_usage_error() {
    let dir = TempDir::new().unwrap();
    biblia(dir.path())
        .args(["export", "notes", "--format", "docx"])
        .assert()
        .failure()
        .code(2)
        .stderr(contains("Formato desconhecido"));
}
