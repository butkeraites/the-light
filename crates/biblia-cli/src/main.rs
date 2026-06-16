//! Binário `biblia` — interface de linha de comando.
//!
//! Os subcomandos (`read`, `search`, `study`...) são adicionados tarefa a tarefa.

fn main() {
    println!(
        "biblia {} — use `biblia --help` (subcomandos em construção)",
        biblia_core::VERSION
    );
}
