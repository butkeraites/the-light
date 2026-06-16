//! Subcomando `config` — set/get/list das preferências do usuário.

use std::process::ExitCode;

use clap::{Args, Subcommand};

use biblia_core::config::Config;

/// Argumentos do subcomando `config`.
#[derive(Args)]
pub struct ConfigArgs {
    #[command(subcommand)]
    action: ConfigAction,
}

#[derive(Subcommand)]
enum ConfigAction {
    /// Define um valor: `biblia config set versions kjv,alm1911`.
    Set {
        /// Chave (versions, language, theme, font-size).
        key: String,
        /// Valor.
        value: String,
    },
    /// Lê um valor: `biblia config get versions`.
    Get {
        /// Chave a consultar.
        key: String,
    },
    /// Lista todas as configurações e o caminho do arquivo.
    List,
}

const EXIT_OK: u8 = 0;
const EXIT_NOT_FOUND: u8 = 1;
const EXIT_USAGE: u8 = 2;

/// Executa o comando `config`.
pub fn run(args: ConfigArgs) -> ExitCode {
    match args.action {
        ConfigAction::Set { key, value } => set(&key, &value),
        ConfigAction::Get { key } => get(&key),
        ConfigAction::List => list(),
    }
}

fn load() -> std::result::Result<Config, ExitCode> {
    Config::load().map_err(|e| {
        eprintln!("Erro ao ler a configuração: {e}");
        ExitCode::from(EXIT_NOT_FOUND)
    })
}

fn set(key: &str, value: &str) -> ExitCode {
    let mut cfg = match load() {
        Ok(c) => c,
        Err(code) => return code,
    };
    if let Err(e) = cfg.set(key, value) {
        eprintln!("{e}");
        return ExitCode::from(EXIT_USAGE);
    }
    if let Err(e) = cfg.save() {
        eprintln!("Erro ao gravar a configuração: {e}");
        return ExitCode::from(EXIT_NOT_FOUND);
    }
    println!("ok: {key} = {}", cfg.get(key).unwrap_or_default());
    ExitCode::from(EXIT_OK)
}

fn get(key: &str) -> ExitCode {
    let cfg = match load() {
        Ok(c) => c,
        Err(code) => return code,
    };
    match cfg.get(key) {
        Some(v) => {
            println!("{v}");
            ExitCode::from(EXIT_OK)
        }
        None => {
            eprintln!(
                "Chave desconhecida: `{key}` (válidas: versions, language, theme, font-size)"
            );
            ExitCode::from(EXIT_USAGE)
        }
    }
}

fn list() -> ExitCode {
    let cfg = match load() {
        Ok(c) => c,
        Err(code) => return code,
    };
    if let Ok(path) = Config::config_path() {
        println!("# {}", path.display());
    }
    for (k, v) in cfg.entries() {
        println!("{k} = {v}");
    }
    ExitCode::from(EXIT_OK)
}
