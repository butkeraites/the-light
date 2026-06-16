//! Subcomando `read` — imprime uma passagem em uma ou mais versões.

use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Args;

use biblia_core::config::Config;
use biblia_core::model::{Lang, Reference};
use biblia_core::reference::{format_reference, parse_reference};
use biblia_core::source::{BibleSource, EmbeddedSource};
use biblia_core::store::Store;
use biblia_core::userdata::HighlightStore;

use crate::render::{self, VersionColumn};
use crate::theme::Style;

/// Carrega a config tolerando erro (avisa e usa padrões).
fn load_config() -> Config {
    Config::load().unwrap_or_else(|e| {
        eprintln!("aviso: configuração inválida ({e}); usando padrões.");
        Config::default()
    })
}

/// Argumentos do subcomando `read`.
#[derive(Args)]
pub struct ReadArgs {
    /// Referência bíblica (PT ou EN): "John 3:16", "Gn 1.1-3", "Sl 23".
    pub reference: String,

    /// Versões a ler (slugs separados por vírgula), ex.: `kjv,alm1911`.
    /// Se omitido, usa `versions` do `config.toml`.
    #[arg(short, long)]
    pub version: Option<String>,

    /// Caminho do banco (padrão: diretório de dados do usuário).
    #[arg(long)]
    pub db: Option<PathBuf>,

    /// Saída sem cor (para pipes/scripts). Cor também desliga em não-TTY e NO_COLOR.
    #[arg(long)]
    pub plain: bool,
}

// Códigos de saída.
const EXIT_OK: u8 = 0;
const EXIT_NOT_FOUND: u8 = 1;
const EXIT_USAGE: u8 = 2;

/// Largura padrão quando o terminal não informa (ou em pipes).
const DEFAULT_WIDTH: usize = 100;

/// Executa o comando `read`, devolvendo o código de saída.
pub fn run(args: ReadArgs) -> ExitCode {
    let reference = match parse_reference(&args.reference) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Referência inválida: {e}");
            eprintln!("Exemplos válidos: \"John 3:16\", \"Gn 1.1-3\", \"Sl 23\", \"1Co 13.4-7\".");
            return ExitCode::from(EXIT_USAGE);
        }
    };

    let store = match open_store(args.db.as_deref()) {
        Ok(s) => s,
        Err(code) => return code,
    };
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

    // Versão da CLI tem prioridade; senão, usa `versions` do config.toml.
    let config = load_config();
    let style = Style::resolve(args.plain, &config.theme);
    let version_spec = args
        .version
        .clone()
        .unwrap_or_else(|| config.versions.join(","));

    // Versões pedidas, sem duplicatas, preservando a ordem informada.
    let mut seen = std::collections::HashSet::new();
    let mut requested: Vec<String> = version_spec
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .filter(|s| seen.insert(s.clone()))
        .collect();
    // Nada especificado nem configurado → usa "kjv" como padrão sensato.
    if requested.is_empty() {
        requested.push("kjv".to_string());
    }

    let mut columns: Vec<VersionColumn> = Vec::new();
    let mut had_error = false;

    for slug in &requested {
        let tid = biblia_core::model::TranslationId::new(slug.clone());
        let Some(meta) = translations.iter().find(|t| t.id == tid) else {
            eprintln!(
                "Versão desconhecida: `{slug}`. Disponíveis: {}",
                translations
                    .iter()
                    .map(|t| t.id.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            had_error = true;
            continue;
        };

        let passage = match src.passage(&reference, &tid) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("Erro ao ler `{slug}`: {e}");
                had_error = true;
                continue;
            }
        };

        let verses = passage
            .verses
            .iter()
            .map(|v| (v.reference.verses.start().unwrap_or(0), v.text.clone()))
            .collect();
        columns.push(VersionColumn {
            label: meta.abbrev.clone(),
            reference: format_reference(&passage.reference, meta.language),
            verses,
        });
    }

    if columns.is_empty() {
        // Todas as versões pedidas eram desconhecidas/erro.
        return ExitCode::from(EXIT_USAGE);
    }

    let found_any = columns.iter().any(|c| !c.verses.is_empty());
    print!("{}", render_output(&columns, &style));
    print_highlight_footer(&reference, &columns, config.language);

    if had_error {
        ExitCode::from(EXIT_USAGE)
    } else if found_any {
        ExitCode::from(EXIT_OK)
    } else {
        ExitCode::from(EXIT_NOT_FOUND)
    }
}

/// Escolhe o modo de renderização: única, colunas lado a lado (TTY largo) ou
/// blocos intercalados (pipe / terminal estreito).
fn render_output(columns: &[VersionColumn], style: &Style) -> String {
    if columns.len() == 1 {
        return render::render_single(&columns[0], style);
    }
    if std::io::stdout().is_terminal() {
        if let Some(out) = render::render_columns(columns, terminal_width(), style) {
            return out;
        }
    }
    render::render_interleaved(columns, style)
}

/// Imprime um rodapé com as marcações que cobrem os versículos exibidos.
fn print_highlight_footer(reference: &Reference, columns: &[VersionColumn], lang: Lang) {
    let Ok(store) = HighlightStore::load_default() else {
        return;
    };
    // Versículos exibidos (união entre as versões).
    let mut shown: Vec<u16> = columns
        .iter()
        .flat_map(|c| c.verses.iter().map(|(n, _)| *n))
        .collect();
    shown.sort_unstable();
    shown.dedup();

    let mut seen = std::collections::HashSet::new();
    let mut found = Vec::new();
    for v in shown {
        for h in store.covering(reference.book, reference.chapter, v) {
            if seen.insert(h.reference) {
                found.push(h);
            }
        }
    }
    if found.is_empty() {
        return;
    }
    println!();
    println!("Marcações:");
    for h in found {
        let tag = h
            .tag
            .as_deref()
            .map(|t| format!("  [{t}]"))
            .unwrap_or_default();
        println!(
            "  {}  {}{}",
            format_reference(&h.reference, lang),
            h.color,
            tag
        );
    }
}

/// Largura atual do terminal, com piso e fallback sensatos.
fn terminal_width() -> usize {
    terminal_size::terminal_size()
        .map(|(w, _)| w.0 as usize)
        .filter(|w| *w >= 2 * render::MIN_COL_WIDTH)
        .unwrap_or(DEFAULT_WIDTH)
}

/// Abre o banco no caminho dado, ou no padrão do usuário.
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
