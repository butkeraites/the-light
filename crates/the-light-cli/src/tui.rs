//! Subcomando `tui` — abre a interface de terminal (ratatui).

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Args;

use the_light_core::config::Config;
use the_light_core::model::TranslationId;
use the_light_core::source::{BibleSource, EmbeddedSource};
use the_light_core::store::Store;

/// Argumentos do subcomando `tui`.
#[derive(Args)]
pub struct TuiArgs {
    /// Versão a abrir (slug). Se omitida, usa a primeira de `versions` do config.
    #[arg(short, long)]
    pub version: Option<String>,

    /// Caminho do banco (padrão: diretório de dados do usuário).
    #[arg(long)]
    pub db: Option<PathBuf>,
}

const EXIT_OK: u8 = 0;
const EXIT_NOT_FOUND: u8 = 1;
const EXIT_USAGE: u8 = 2;

/// Executa o comando `tui`.
pub fn run(args: TuiArgs) -> ExitCode {
    let store = match open_store(args.db.as_deref()) {
        Ok(s) => s,
        Err(code) => return code,
    };

    let config = Config::load().unwrap_or_default();

    // Resolve a versão ANTES de mover o `store` para a TUI (a fonte empresta o store).
    let tid = {
        let src = EmbeddedSource::new(&store);
        let translations = match src.translations() {
            Ok(t) => t,
            Err(e) => {
                eprintln!("Erro ao ler o banco: {e}");
                return ExitCode::from(EXIT_NOT_FOUND);
            }
        };
        if translations.is_empty() {
            eprintln!(
                "Nenhuma versão importada. Gere o banco com:\n  \
                 cargo run -p xtask -- import --version kjv,alm1911"
            );
            return ExitCode::from(EXIT_NOT_FOUND);
        }

        let requested = args
            .version
            .clone()
            .or_else(|| config.versions.first().cloned());

        match requested {
            Some(slug) => {
                let tid = TranslationId::new(slug.clone());
                match translations.iter().find(|t| t.id == tid) {
                    Some(t) => t.id.clone(),
                    None if args.version.is_some() => {
                        eprintln!(
                            "Versão desconhecida: `{slug}`. Disponíveis: {}",
                            translations
                                .iter()
                                .map(|t| t.id.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        );
                        return ExitCode::from(EXIT_USAGE);
                    }
                    // Config aponta versão ausente → usa a primeira disponível.
                    None => translations[0].id.clone(),
                }
            }
            None => translations[0].id.clone(),
        }
    };

    match the_light_tui::run(store, tid, args.db.clone()) {
        Ok(()) => ExitCode::from(EXIT_OK),
        Err(e) => {
            eprintln!("Erro na TUI: {e}");
            ExitCode::from(EXIT_NOT_FOUND)
        }
    }
}

fn open_store(db: Option<&Path>) -> Result<Store, ExitCode> {
    let res = match db {
        Some(p) => Store::open(p),
        None => Store::open_default(),
    };
    res.map_err(|e| {
        eprintln!("Erro ao abrir o banco: {e}");
        ExitCode::from(EXIT_NOT_FOUND)
    })
}
