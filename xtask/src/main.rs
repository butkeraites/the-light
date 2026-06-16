//! `xtask` — runner de tarefas de desenvolvimento (importação de datasets, etc.).
//!
//! Uso: `cargo run -p xtask -- <comando>`. Comandos são adicionados em T0.4.

fn main() {
    let cmd = std::env::args().nth(1);
    match cmd.as_deref() {
        Some("help") | None => {
            eprintln!("xtask — comandos disponíveis serão adicionados em T0.4 (import).");
        }
        Some(other) => {
            eprintln!("xtask: comando desconhecido `{other}` (ainda não implementado).");
            std::process::exit(2);
        }
    }
}
