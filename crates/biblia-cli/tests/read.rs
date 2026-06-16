//! Testes de integração do comando `biblia read` (via `assert_cmd`).
//!
//! Semeia um banco temporário com algumas linhas e roda o binário compilado.

use std::path::PathBuf;

use assert_cmd::Command;
use biblia_core::store::Store;
use predicates::str::contains;
use tempfile::TempDir;

/// Cria um banco temporário com KJV e João 3.16-17.
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
        for (v, text) in [
            (16, "For God so loved the world"),
            (17, "For God sent not his Son into the world to condemn"),
        ] {
            conn.execute(
                "INSERT INTO verses(translation_id,book_number,chapter,verse,text) \
                 VALUES ('kjv',43,3,?1,?2)",
                rusqlite::params![v as i64, text],
            )
            .unwrap();
        }
        // Segunda versão (PT) para testar leitura paralela.
        conn.execute(
            "INSERT INTO translations(id,abbrev,name,language,license,embeddable) \
             VALUES ('alm','ALM','Almeida','pt','public-domain',1)",
            [],
        )
        .unwrap();
        for (v, text) in [
            (16, "Porque Deus amou o mundo"),
            (17, "Porque Deus enviou o seu Filho ao mundo"),
        ] {
            conn.execute(
                "INSERT INTO verses(translation_id,book_number,chapter,verse,text) \
                 VALUES ('alm',43,3,?1,?2)",
                rusqlite::params![v as i64, text],
            )
            .unwrap();
        }
    }
    (dir, path)
}

fn biblia() -> Command {
    let mut cmd = Command::cargo_bin("biblia").unwrap();
    // Isola do config/dados reais do desenvolvedor (caminhos inexistentes → padrões).
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
fn read_single_verse_prints_text_and_exits_zero() {
    let (_dir, path) = seeded_db();
    biblia()
        .args(["read", "John 3:16", "--version", "kjv", "--db"])
        .arg(&path)
        .assert()
        .success()
        .stdout(contains("For God so loved the world"))
        .stdout(contains("John 3:16"));
}

#[test]
fn read_accepts_portuguese_reference_for_english_version() {
    let (_dir, path) = seeded_db();
    // Referência em PT ("Jo 3.16") deve resolver o mesmo livro/verso.
    biblia()
        .args(["read", "Jo 3.16", "--db"])
        .arg(&path)
        .assert()
        .success()
        .stdout(contains("For God so loved the world"));
}

#[test]
fn read_range_prints_both_verses() {
    let (_dir, path) = seeded_db();
    biblia()
        .args(["read", "John 3:16-17", "--db"])
        .arg(&path)
        .assert()
        .success()
        .stdout(contains("16"))
        .stdout(contains("17"))
        .stdout(contains("For God sent not"));
}

#[test]
fn invalid_reference_exits_with_usage_code() {
    let (_dir, path) = seeded_db();
    biblia()
        .args(["read", "NotARef!!!", "--db"])
        .arg(&path)
        .assert()
        .failure()
        .code(2)
        .stderr(contains("Referência inválida"));
}

#[test]
fn unknown_version_exits_usage() {
    let (_dir, path) = seeded_db();
    biblia()
        .args(["read", "John 3:16", "--version", "zzz", "--db"])
        .arg(&path)
        .assert()
        .failure()
        .code(2)
        .stderr(contains("versão desconhecida"));
}

#[test]
fn mixed_known_and_unknown_version_exits_usage() {
    let (_dir, path) = seeded_db();
    // kjv imprime, mas a versão inexistente faz o código de saída ser !=0.
    biblia()
        .args(["read", "John 3:16", "--version", "kjv,nonexistent", "--db"])
        .arg(&path)
        .assert()
        .failure()
        .code(2)
        .stdout(contains("For God so loved the world"))
        .stderr(contains("versão desconhecida"));
}

#[test]
fn parallel_two_versions_shows_both_aligned() {
    let (_dir, path) = seeded_db();
    // Saída capturada (não-TTY) → blocos intercalados por versículo.
    biblia()
        .args(["read", "John 3:16-17", "--version", "kjv,alm", "--db"])
        .arg(&path)
        .assert()
        .success()
        .stdout(contains("For God so loved the world"))
        .stdout(contains("Porque Deus amou o mundo"))
        .stdout(contains("For God sent not"))
        .stdout(contains("Porque Deus enviou"));
}

#[test]
fn duplicate_version_is_printed_once() {
    let (_dir, path) = seeded_db();
    let out = biblia()
        .args(["read", "John 3:16", "--version", "kjv,kjv", "--db"])
        .arg(&path)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(out).unwrap();
    assert_eq!(
        text.matches("For God so loved the world").count(),
        1,
        "passagem deveria aparecer uma única vez para versões duplicadas"
    );
}

#[test]
fn piped_output_has_no_ansi_escapes() {
    // Saída capturada (não-TTY) deve sair sem códigos ANSI, mesmo sem --plain.
    let (_dir, path) = seeded_db();
    let out = biblia()
        .args(["read", "John 3:16", "--version", "kjv", "--db"])
        .arg(&path)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    assert!(
        !out.contains(&0x1b),
        "saída em pipe não deveria conter ESC/ANSI"
    );
}

#[test]
fn missing_passage_exits_not_found_with_notice() {
    let (_dir, path) = seeded_db();
    biblia()
        .args(["read", "John 99:1", "--db"])
        .arg(&path)
        .assert()
        .failure()
        .code(1)
        .stdout(contains("nenhum versículo"));
}
