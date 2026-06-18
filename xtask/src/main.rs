//! `xtask` — runner de tarefas de desenvolvimento (importação de datasets, etc.).
//!
//! Uso: `cargo run -p xtask -- <comando>`.
//!
//! Comandos:
//! - `import --version kjv,alm1911 [--db PATH] [--seed-dir DIR] [--force] [--offline]`

mod import;
mod scholarly_import;
mod xref_import;

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("xtask: erro: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[String]) -> anyhow::Result<()> {
    match args.first().map(String::as_str) {
        Some("import") => import::run_import(&args[1..]),
        Some("import-xref") => xref_import::run(&args[1..]),
        Some("import-scholarly") => scholarly_import::run(&args[1..]),
        Some("help") | Some("--help") | Some("-h") | None => {
            print_help();
            Ok(())
        }
        Some(other) => anyhow::bail!("comando desconhecido `{other}` (tente `help`)"),
    }
}

fn print_help() {
    println!(
        "xtask — tarefas de build/import da The Light\n\n\
         USO:\n  cargo run -p xtask -- <comando>\n\n\
         COMANDOS:\n  \
         import --version <ids>   Importa versões livres para o banco\n  \
         import-xref              Importa referências cruzadas (OpenBible, CC-BY)\n  \
         import-scholarly         Importa línguas originais + léxicos (STEPBible, CC-BY)\n  \
         help                     Mostra esta ajuda\n\n\
         OPÇÕES de `import`:\n  \
         --version <a,b>   Versões a importar (obrigatório)\n  \
         --db <path>       Caminho do banco (padrão: diretório de dados XDG)\n  \
         --seed-dir <dir>  Onde guardar/ler os datasets brutos (padrão: data/seed)\n  \
         --force           Rebaixa o dataset mesmo se já existir em cache\n  \
         --offline         Falha em vez de baixar (usa só arquivos em cache)\n\n\
         Versões livres disponíveis: {}",
        import::available_versions()
    );
}
