//! Testes de integração do comando `biblia plan`.

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
fn list_shows_plans() {
    let dir = TempDir::new().unwrap();
    biblia(dir.path())
        .args(["plan", "list"])
        .assert()
        .success()
        .stdout(contains("annual"))
        .stdout(contains("365 dias"))
        .stdout(contains("gospels"));
}

#[test]
fn today_without_active_plan_is_not_found() {
    let dir = TempDir::new().unwrap();
    biblia(dir.path())
        .args(["plan", "today"])
        .assert()
        .failure()
        .code(1)
        .stderr(contains("Nenhum plano ativo"));
}

#[test]
fn start_today_status_mark_reset_flow() {
    let dir = TempDir::new().unwrap();
    // Inicia o plano anual a partir de 2026-01-01.
    biblia(dir.path())
        .args(["plan", "start", "annual", "--date", "2026-01-01"])
        .assert()
        .success()
        .stdout(contains("Plano iniciado"))
        .stdout(contains("365 dias"));

    // Dia conforme a data: 11/01/2026 → dia 11.
    biblia(dir.path())
        .args(["plan", "today", "--date", "2026-01-11"])
        .assert()
        .success()
        .stdout(contains("dia 11/365"))
        .stdout(contains("Gênesis")); // leitura inicial é em Gênesis

    // Marca um dia concluído → progresso avança.
    biblia(dir.path())
        .args(["plan", "mark"])
        .assert()
        .success()
        .stdout(contains("Concluídos: 1/365"));

    biblia(dir.path())
        .args(["plan", "status", "--date", "2026-01-11"])
        .assert()
        .success()
        .stdout(contains("dia 11/365"))
        .stdout(contains("Concluídos: 1/365"));

    biblia(dir.path())
        .args(["plan", "reset"])
        .assert()
        .success()
        .stdout(contains("encerrado"));

    biblia(dir.path())
        .args(["plan", "today"])
        .assert()
        .failure()
        .code(1);
}

#[test]
fn start_does_not_silently_overwrite_active_plan() {
    let dir = TempDir::new().unwrap();
    biblia(dir.path())
        .args(["plan", "start", "annual", "--date", "2026-01-01"])
        .assert()
        .success();
    // Sem --force: recusa sobrescrever.
    biblia(dir.path())
        .args(["plan", "start", "nt", "--date", "2026-02-01"])
        .assert()
        .failure()
        .code(2)
        .stderr(contains("Já existe um plano ativo"));
    // Com --force: sobrescreve.
    biblia(dir.path())
        .args(["plan", "start", "nt", "--date", "2026-02-01", "--force"])
        .assert()
        .success()
        .stdout(contains("Novo Testamento"));
}

#[test]
fn status_clamps_corrupted_completed() {
    let dir = TempDir::new().unwrap();
    let active = dir.path().join("reading-plans").join("active.json");
    std::fs::create_dir_all(active.parent().unwrap()).unwrap();
    // active.json editado à mão com completed absurdo.
    std::fs::write(
        &active,
        r#"{"plan_id":"gospels","start_date":"2026-01-01","completed":9999}"#,
    )
    .unwrap();
    biblia(dir.path())
        .args(["plan", "status", "--date", "2026-01-05"])
        .assert()
        .success()
        .stdout(contains("Concluídos: 30/30 (100%)")); // clamp a 30, não 9999
}

#[test]
fn start_unknown_plan_is_usage_error() {
    let dir = TempDir::new().unwrap();
    biblia(dir.path())
        .args(["plan", "start", "nope"])
        .assert()
        .failure()
        .code(2)
        .stderr(contains("Plano desconhecido"));
}

#[test]
fn ics_export_is_valid_icalendar() {
    let dir = TempDir::new().unwrap();
    biblia(dir.path())
        .args(["plan", "start", "gospels", "--date", "2026-01-01"])
        .assert()
        .success();

    let out = dir.path().join("plano.ics");
    biblia(dir.path())
        .args(["plan", "ics", "--output"])
        .arg(&out)
        .assert()
        .success()
        .stdout(contains("30 eventos"));

    let ics = std::fs::read_to_string(&out).unwrap();
    assert!(ics.starts_with("BEGIN:VCALENDAR"));
    assert!(ics.contains("BEGIN:VEVENT"));
    assert!(ics.contains("DTSTART;VALUE=DATE:20260101"));
    assert!(ics.contains("SUMMARY:Leitura"));
    assert!(ics.trim_end().ends_with("END:VCALENDAR"));
    // 30 dias → 30 eventos.
    assert_eq!(ics.matches("BEGIN:VEVENT").count(), 30);
}
