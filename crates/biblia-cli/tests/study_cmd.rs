//! Testes de integração de `biblia study` e `biblia ask` (provedor `mock`, sem rede).

use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use tempfile::TempDir;

/// Cria um banco temporário com uma versão e alguns versículos de Efésios 2.
fn fixture() -> (TempDir, std::path::PathBuf) {
    let dir = TempDir::new().unwrap();
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
        // Efésios = livro 49.
        conn.execute(
            "INSERT INTO verses(translation_id,book_number,chapter,verse,text) \
             VALUES ('alm',49,2,8,'Porque pela graça sois salvos, por meio da fé')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO verses(translation_id,book_number,chapter,verse,text) \
             VALUES ('alm',49,2,9,'Não vem das obras, para que ninguém se glorie')",
            [],
        )
        .unwrap();
    }
    (dir, db)
}

/// `biblia` com diretórios isolados.
fn biblia(home: &std::path::Path) -> Command {
    let mut cmd = Command::cargo_bin("biblia").unwrap();
    cmd.env("BIBLIA_CONFIG", home.join("config.toml"));
    cmd.env("BIBLIA_SECRETS", home.join("secrets.toml"));
    cmd.env("BIBLIA_DATA_DIR", home.join("data"));
    cmd
}

#[test]
fn study_with_mock_cites_text_and_interpretation() {
    let (dir, db) = fixture();
    biblia(dir.path())
        .args([
            "study",
            "Ef 2.8-9",
            "--lens",
            "luterana",
            "--provider",
            "mock",
            "--version",
            "alm",
            "--db",
        ])
        .arg(&db)
        .assert()
        .success()
        .stdout(contains("# Estudo — Efésios 2.8-9"))
        .stdout(contains("Lente: Luterana"))
        .stdout(contains("## Texto citado"))
        .stdout(contains("Porque pela graça sois salvos"))
        .stdout(contains("## Interpretação"))
        .stdout(contains("simulada")) // resposta do mock
        .stdout(contains("tokens")); // estimativa de custo
}

#[test]
fn study_compare_multiple_lenses() {
    let (dir, db) = fixture();
    biblia(dir.path())
        .args([
            "study",
            "Ef 2.8-9",
            "--lens",
            "batista,católica",
            "--provider",
            "mock",
            "--version",
            "alm",
            "--db",
        ])
        .arg(&db)
        .assert()
        .success()
        .stdout(contains("Lente: Batista"))
        .stdout(contains("Lente: Católica"))
        .stdout(contains("———")); // separador entre lentes
}

#[test]
fn study_save_writes_markdown_file() {
    let (dir, db) = fixture();
    biblia(dir.path())
        .args([
            "study",
            "Ef 2.8",
            "--lens",
            "presbiteriana",
            "--provider",
            "mock",
            "--save",
            "--version",
            "alm",
            "--db",
        ])
        .arg(&db)
        .assert()
        .success()
        .stdout(contains("Salvo em"));
    let studies = dir.path().join("data").join("studies");
    let any_md = std::fs::read_dir(&studies)
        .unwrap()
        .filter_map(|e| e.ok())
        .any(|e| e.path().extension().is_some_and(|x| x == "md"));
    assert!(any_md, "deveria ter gravado um .md em {studies:?}");
}

#[test]
fn study_unknown_lens_is_usage_error() {
    let (dir, db) = fixture();
    biblia(dir.path())
        .args([
            "study",
            "Ef 2.8",
            "--lens",
            "jedi",
            "--provider",
            "mock",
            "--db",
        ])
        .arg(&db)
        .assert()
        .failure()
        .code(2)
        .stderr(contains("lente desconhecida"));
}

#[test]
fn study_without_provider_is_friendly_usage_error() {
    let (dir, db) = fixture();
    // Sem provedor configurado e sem --provider: erro amigável (saída 2).
    biblia(dir.path())
        .args([
            "study",
            "Ef 2.8",
            "--lens",
            "batista",
            "--version",
            "alm",
            "--db",
        ])
        .arg(&db)
        .assert()
        .failure()
        .code(2)
        .stderr(contains("nenhum provedor"));
}

#[test]
fn study_anthropic_without_key_is_friendly_error() {
    let (dir, db) = fixture();
    biblia(dir.path())
        .args([
            "study",
            "Ef 2.8",
            "--lens",
            "batista",
            "--provider",
            "anthropic",
            "--version",
            "alm",
            "--db",
        ])
        .arg(&db)
        .assert()
        .failure()
        .code(2)
        .stderr(contains("nenhuma chave para `anthropic`"));
}

#[test]
fn ask_with_mock_and_ref() {
    let (dir, db) = fixture();
    biblia(dir.path())
        .args([
            "ask",
            "O que é a graça?",
            "--ref",
            "Ef 2",
            "--provider",
            "mock",
            "--version",
            "alm",
            "--db",
        ])
        .arg(&db)
        .assert()
        .success()
        .stdout(contains("simulada").or(contains("teste")));
}

#[test]
fn ask_without_ref_still_runs_with_mock() {
    let (dir, _db) = fixture();
    biblia(dir.path())
        .args(["ask", "Resuma o evangelho", "--provider", "mock"])
        .assert()
        .success();
}
