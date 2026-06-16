//! Testes de metadados/hardening da CLI: `--version`, `--help`, ausência de
//! subcomando. Garantem que o "rosto" público do binário não regrida.

use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;

fn biblia() -> Command {
    Command::cargo_bin("biblia").unwrap()
}

#[test]
fn version_reports_1_0_0() {
    biblia()
        .arg("--version")
        .assert()
        .success()
        .stdout(contains("biblia"))
        .stdout(contains("1.0.0"));
}

#[test]
fn help_mentions_privacy_and_byok() {
    // O long_about deixa claras as garantias de privacidade/IA opt-in.
    biblia()
        .arg("--help")
        .assert()
        .success()
        .stdout(contains("Offline-first"))
        .stdout(contains("telemetria"))
        .stdout(contains("BYOK"));
}

#[test]
fn help_lists_all_subcommands() {
    let out = biblia()
        .arg("--help")
        .assert()
        .success()
        .get_output()
        .clone();
    let stdout = String::from_utf8(out.stdout).unwrap();
    for cmd in [
        "read",
        "search",
        "config",
        "highlight",
        "note",
        "xref",
        "export",
        "tui",
        "plan",
        "study",
        "ask",
    ] {
        assert!(
            stdout.contains(cmd),
            "ajuda deveria listar `{cmd}`:\n{stdout}"
        );
    }
}

#[test]
fn no_subcommand_is_usage_error() {
    // Sem subcomando, clap mostra ajuda e sai com código 2 (uso).
    biblia().assert().failure().code(2);
}

#[test]
fn unknown_subcommand_is_usage_error() {
    biblia()
        .arg("levitate")
        .assert()
        .failure()
        .code(2)
        .stderr(contains("unrecognized").or(predicates::str::contains("unexpected")));
}
