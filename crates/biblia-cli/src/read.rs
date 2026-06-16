//! Subcomando `read` — imprime uma passagem em uma ou mais versões.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Args;

use biblia_core::model::{Passage, Translation, TranslationId};
use biblia_core::reference::{format_reference, parse_reference};
use biblia_core::source::{BibleSource, EmbeddedSource};
use biblia_core::store::Store;

/// Argumentos do subcomando `read`.
#[derive(Args)]
pub struct ReadArgs {
    /// Referência bíblica (PT ou EN): "John 3:16", "Gn 1.1-3", "Sl 23".
    pub reference: String,

    /// Versões a ler (slugs separados por vírgula), ex.: `kjv,alm1911`.
    #[arg(short, long, default_value = "kjv")]
    pub version: String,

    /// Caminho do banco (padrão: diretório de dados do usuário).
    #[arg(long)]
    pub db: Option<PathBuf>,
}

// Códigos de saída.
const EXIT_OK: u8 = 0;
const EXIT_NOT_FOUND: u8 = 1;
const EXIT_USAGE: u8 = 2;

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

    // Versões pedidas, sem duplicatas, preservando a ordem informada.
    let mut seen = std::collections::HashSet::new();
    let requested: Vec<String> = args
        .version
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .filter(|s| seen.insert(s.clone()))
        .collect();

    let mut printed = 0usize;
    let mut found_any = false;
    let mut had_error = false;

    for slug in &requested {
        let tid = TranslationId::new(slug.clone());
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

        if printed > 0 {
            println!();
        }
        print_passage(&passage, meta);
        printed += 1;
        if !passage.is_empty() {
            found_any = true;
        }
    }

    // Uma versão pedida que não existe / falhou domina o código de saída: o
    // usuário pediu algo que não pôde ser atendido.
    if had_error {
        ExitCode::from(EXIT_USAGE)
    } else if found_any {
        ExitCode::from(EXIT_OK)
    } else {
        ExitCode::from(EXIT_NOT_FOUND)
    }
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

/// Imprime o cabeçalho da referência (no idioma da versão) e os versículos.
fn print_passage(passage: &Passage, meta: &Translation) {
    println!(
        "{} ({})",
        format_reference(&passage.reference, meta.language),
        meta.abbrev
    );
    if passage.is_empty() {
        println!("  (nenhum versículo encontrado)");
        return;
    }
    for v in &passage.verses {
        // Todo `Verse` de uma passagem carrega `VerseRange::Single`; `start()`
        // devolve esse número.
        let n = v.reference.verses.start().unwrap_or(0);
        println!("  {n:>3}  {}", v.text);
    }
}
