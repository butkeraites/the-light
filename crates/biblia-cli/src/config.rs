//! Subcomando `config` — set/get/list das preferências do usuário.

use std::process::ExitCode;

use clap::{Args, Subcommand};

use biblia_core::ai::KeyStore;
use biblia_core::config::{Config, Connector};

/// Provedores válidos para `set-key`: IA + conectores de versões protegidas.
const KEY_PROVIDERS: &[&str] = &["anthropic", "openai", "ollama", "apibible", "esv"];

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
    /// Grava a chave de API de um provedor (em armazenamento fora do git).
    SetKey {
        /// Provedor (anthropic, openai, ollama).
        provider: String,
        /// Chave de API.
        key: String,
    },
    /// Remove a chave de um provedor.
    RemoveKey {
        /// Provedor.
        provider: String,
    },
    /// Lista os provedores que têm chave (sem mostrar as chaves).
    Keys,
    /// Conectores de versões protegidas (lidas ao vivo via API).
    Connector {
        #[command(subcommand)]
        action: ConnectorAction,
    },
}

#[derive(Subcommand)]
enum ConnectorAction {
    /// Adiciona/atualiza um conector: `config connector add ara --kind apibible --bible-id <id> --name "..." --abbrev ARA`.
    Add {
        /// Slug usado em `--version` (ex.: ara, esv).
        slug: String,
        /// Tipo: apibible | esv.
        #[arg(long)]
        kind: String,
        /// Nome de exibição.
        #[arg(long)]
        name: String,
        /// Abreviação de exibição.
        #[arg(long)]
        abbrev: String,
        /// Idioma do texto (pt | en).
        #[arg(long, default_value = "pt")]
        lang: String,
        /// Id da Bíblia na API.Bible (obrigatório p/ kind=apibible).
        #[arg(long)]
        bible_id: Option<String>,
    },
    /// Lista os conectores configurados.
    List,
    /// Remove um conector pelo slug.
    Remove {
        /// Slug a remover.
        slug: String,
    },
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
        ConfigAction::SetKey { provider, key } => set_key(&provider, &key),
        ConfigAction::RemoveKey { provider } => remove_key(&provider),
        ConfigAction::Keys => keys(),
        ConfigAction::Connector { action } => connector(action),
    }
}

fn connector(action: ConnectorAction) -> ExitCode {
    match action {
        ConnectorAction::Add {
            slug,
            kind,
            name,
            abbrev,
            lang,
            bible_id,
        } => connector_add(slug, kind, name, abbrev, lang, bible_id),
        ConnectorAction::List => connector_list(),
        ConnectorAction::Remove { slug } => connector_remove(&slug),
    }
}

fn connector_add(
    slug: String,
    kind: String,
    name: String,
    abbrev: String,
    lang: String,
    bible_id: Option<String>,
) -> ExitCode {
    use std::str::FromStr;

    let kind = kind.trim().to_ascii_lowercase();
    if kind != "apibible" && kind != "esv" {
        eprintln!("Tipo de conector inválido: `{kind}` (use: apibible, esv).");
        return ExitCode::from(EXIT_USAGE);
    }
    if kind == "apibible" && bible_id.as_deref().unwrap_or("").trim().is_empty() {
        eprintln!("`--bible-id` é obrigatório para kind=apibible.");
        return ExitCode::from(EXIT_USAGE);
    }
    let language = match biblia_core::model::Lang::from_str(&lang) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Idioma inválido: {e}");
            return ExitCode::from(EXIT_USAGE);
        }
    };
    let slug = slug.trim().to_ascii_lowercase();
    let conn = Connector {
        slug: slug.clone(),
        kind: kind.clone(),
        bible_id: bible_id.filter(|s| !s.trim().is_empty()),
        name,
        abbrev,
        language,
    };

    let mut cfg = match load() {
        Ok(c) => c,
        Err(code) => return code,
    };
    // Substitui se o slug já existir.
    cfg.connectors.retain(|c| c.slug != slug);
    cfg.connectors.push(conn);
    if let Err(e) = cfg.save() {
        eprintln!("Erro ao gravar a configuração: {e}");
        return ExitCode::from(EXIT_NOT_FOUND);
    }
    println!("ok: conector `{slug}` ({kind}) configurado.");
    println!("Defina a chave com: biblia config set-key {kind} <chave>");
    ExitCode::from(EXIT_OK)
}

fn connector_list() -> ExitCode {
    let cfg = match load() {
        Ok(c) => c,
        Err(code) => return code,
    };
    if cfg.connectors.is_empty() {
        println!("Nenhum conector configurado.");
        return ExitCode::from(EXIT_OK);
    }
    for c in &cfg.connectors {
        let bid = c
            .bible_id
            .as_deref()
            .map(|b| format!("  bible_id={b}"))
            .unwrap_or_default();
        println!(
            "{}  [{}]  {} ({})  lang={}{bid}",
            c.slug, c.kind, c.name, c.abbrev, c.language
        );
    }
    ExitCode::from(EXIT_OK)
}

fn connector_remove(slug: &str) -> ExitCode {
    let slug = slug.trim().to_ascii_lowercase();
    let mut cfg = match load() {
        Ok(c) => c,
        Err(code) => return code,
    };
    let before = cfg.connectors.len();
    cfg.connectors.retain(|c| c.slug != slug);
    if cfg.connectors.len() == before {
        println!("Nenhum conector `{slug}`.");
        return ExitCode::from(EXIT_NOT_FOUND);
    }
    if let Err(e) = cfg.save() {
        eprintln!("Erro ao gravar a configuração: {e}");
        return ExitCode::from(EXIT_NOT_FOUND);
    }
    println!("Conector `{slug}` removido.");
    ExitCode::from(EXIT_OK)
}

fn set_key(provider: &str, key: &str) -> ExitCode {
    let provider = provider.to_ascii_lowercase();
    if !KEY_PROVIDERS.contains(&provider.as_str()) {
        eprintln!(
            "Provedor desconhecido: `{provider}` (use: {}).",
            KEY_PROVIDERS.join(", ")
        );
        return ExitCode::from(EXIT_USAGE);
    }
    if key.trim().is_empty() {
        eprintln!("A chave não pode ser vazia.");
        return ExitCode::from(EXIT_USAGE);
    }
    let mut ks = match KeyStore::open_default() {
        Ok(k) => k,
        Err(e) => {
            eprintln!("Erro ao abrir o cofre de chaves: {e}");
            return ExitCode::from(EXIT_NOT_FOUND);
        }
    };
    if let Err(e) = ks.set(&provider, key) {
        eprintln!("Erro ao gravar a chave: {e}");
        return ExitCode::from(EXIT_NOT_FOUND);
    }
    // Nunca ecoa a chave.
    let where_ = KeyStore::secrets_path()
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    println!("Chave de `{provider}` gravada em {where_} (fora do git).");
    ExitCode::from(EXIT_OK)
}

fn remove_key(provider: &str) -> ExitCode {
    let provider = provider.to_ascii_lowercase();
    let mut ks = match KeyStore::open_default() {
        Ok(k) => k,
        Err(e) => {
            eprintln!("Erro ao abrir o cofre de chaves: {e}");
            return ExitCode::from(EXIT_NOT_FOUND);
        }
    };
    match ks.remove(&provider) {
        Ok(true) => {
            println!("Chave de `{provider}` removida.");
            ExitCode::from(EXIT_OK)
        }
        Ok(false) => {
            println!("Nenhuma chave para `{provider}`.");
            ExitCode::from(EXIT_NOT_FOUND)
        }
        Err(e) => {
            eprintln!("Erro ao remover a chave: {e}");
            ExitCode::from(EXIT_NOT_FOUND)
        }
    }
}

fn keys() -> ExitCode {
    let ks = match KeyStore::open_default() {
        Ok(k) => k,
        Err(e) => {
            eprintln!("Erro ao abrir o cofre de chaves: {e}");
            return ExitCode::from(EXIT_NOT_FOUND);
        }
    };
    let providers = ks.list_providers();
    if providers.is_empty() {
        println!("Nenhuma chave configurada.");
    } else {
        println!("Provedores com chave: {}", providers.join(", "));
    }
    ExitCode::from(EXIT_OK)
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
                "Chave desconhecida: `{key}` \
                 (válidas: versions, language, theme, font-size, provider)"
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
