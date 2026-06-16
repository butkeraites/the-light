//! Subcomando `study` — estudo exegético de uma passagem por lente denominacional.
//!
//! Uma lente: `biblia study "Ef 2.8-9" --lens presbiteriana`.
//! Comparar lentes: `biblia study "Ef 2.8-9" --lens batista,luterana`.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Args;

use biblia_core::ai::{self, Denomination, StudyDepth, StudyRequest};
use biblia_core::config::Config;
use biblia_core::reference::{format_reference, parse_reference};
use biblia_core::userdata;

use crate::ai_common;

/// Argumentos do subcomando `study`.
#[derive(Args)]
pub struct StudyArgs {
    /// Passagem (PT/EN), ex.: "Ef 2.8-9".
    pub reference: String,

    /// Lente(s) denominacional(is), separadas por vírgula (compara se >1).
    #[arg(long, default_value = "presbiteriana")]
    pub lens: String,

    /// Profundidade: geral | exegetico | palavras.
    #[arg(long, default_value = "geral")]
    pub depth: String,

    /// Versão para o texto citado (CLI > config > kjv).
    #[arg(short, long)]
    pub version: Option<String>,

    /// Provedor de IA (CLI > config). Use `mock` para demonstração offline.
    #[arg(long)]
    pub provider: Option<String>,

    /// Modelo específico (sobrescreve o padrão do provedor).
    #[arg(long)]
    pub model: Option<String>,

    /// Salva o estudo em `studies/` (Markdown).
    #[arg(long)]
    pub save: bool,

    /// Caminho do banco (padrão: diretório de dados do usuário).
    #[arg(long)]
    pub db: Option<PathBuf>,
}

const EXIT_OK: u8 = 0;
const EXIT_NOT_FOUND: u8 = 1;
const EXIT_USAGE: u8 = 2;

/// Executa o comando `study`.
pub fn run(args: StudyArgs) -> ExitCode {
    let reference = match parse_reference(&args.reference) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Referência inválida: {e}");
            return ExitCode::from(EXIT_USAGE);
        }
    };

    let lenses: Vec<Denomination> = match args
        .lens
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.parse::<Denomination>())
        .collect()
    {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(EXIT_USAGE);
        }
    };
    if lenses.is_empty() {
        eprintln!("Informe ao menos uma lente em `--lens`.");
        return ExitCode::from(EXIT_USAGE);
    }

    let depth: StudyDepth = match args.depth.parse() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(EXIT_USAGE);
        }
    };

    let config = Config::load().unwrap_or_default();
    let lang = config.language;

    let store = match ai_common::open_store(args.db.as_deref()) {
        Ok(s) => s,
        Err(code) => return code,
    };

    let passage =
        match ai_common::resolve_passage(&store, &config, &reference, args.version.as_deref()) {
            Ok(p) => p,
            Err(code) => return code,
        };

    let cross_references = ai_common::xref_labels(&store, &reference, &passage, lang, 8);

    let provider =
        match ai_common::resolve_provider(args.provider.clone(), args.model.clone(), &config) {
            Ok(p) => p,
            Err(code) => return code,
        };

    let label = format_reference(&reference, lang);
    let total = lenses.len();
    let mut succeeded = 0usize;
    let mut failed = 0usize;
    let mut save_failed = false;

    for (i, lens) in lenses.iter().enumerate() {
        let req = StudyRequest {
            reference,
            reference_label: label.clone(),
            lens: *lens,
            depth,
            language: lang,
            passage: &passage,
            cross_references: cross_references.clone(),
        };
        match ai::study(provider.as_ref(), &req) {
            Ok(result) => {
                if i > 0 {
                    println!("\n———\n");
                }
                print!("{}", result.to_markdown());
                print_cost(provider.as_ref(), &result);
                succeeded += 1;
                if args.save && save_study(&result).is_err() {
                    save_failed = true;
                }
            }
            Err(e) => {
                eprintln!("Falha no estudo ({}): {e}", lens.name_pt());
                eprintln!("Dica: verifique a chave (`biblia config keys`) e a conexão.");
                failed += 1;
            }
        }
    }

    // Em comparação de lentes, deixa claro o resultado parcial; saída != 0 se algo
    // falhou (convenção Unix), mesmo que parte tenha sido impressa.
    if failed > 0 && succeeded > 0 {
        eprintln!("Atenção: {failed} de {total} lentes falharam (as demais foram impressas).");
    }
    let exit = if failed > 0 || save_failed {
        EXIT_NOT_FOUND
    } else {
        EXIT_OK
    };
    ExitCode::from(exit)
}

/// Imprime uma estimativa de tokens/custo (não-fatal).
fn print_cost(provider: &dyn biblia_core::ai::LlmProvider, result: &ai::StudyResult) {
    let out = provider.estimate_tokens(&result.interpretation);
    let inp = provider.estimate_tokens(&result.passage_text) + 400; // overhead do prompt
    match ai::estimate_cost_usd(&result.model, inp, out) {
        Some(c) => println!("\n_≈ {inp}+{out} tokens · ~${c:.4} (estimativa)_"),
        None => println!("\n_≈ {inp}+{out} tokens · custo n/d_"),
    }
}

/// Salva o estudo em `studies/<ref>-<lente>.md`.
fn save_study(result: &ai::StudyResult) -> Result<(), u8> {
    let dir = userdata::studies_dir().map_err(|e| {
        eprintln!("Erro ao localizar `studies/`: {e}");
        EXIT_NOT_FOUND
    })?;
    if let Err(e) = std::fs::create_dir_all(&dir) {
        eprintln!("Erro ao criar `studies/`: {e}");
        return Err(EXIT_NOT_FOUND);
    }
    let slug = slugify(&format!(
        "{}-{}",
        result.reference_label,
        result.lens.slug()
    ));
    let path = dir.join(format!("{slug}.md"));
    if let Err(e) = biblia_core::util::atomic_write(&path, result.to_markdown().as_bytes()) {
        eprintln!("Erro ao gravar o estudo: {e}");
        return Err(EXIT_NOT_FOUND);
    }
    println!("_Salvo em {}_", path.display());
    Ok(())
}

/// Slug simples para nome de arquivo (minúsculas, alfanumérico → `-`).
fn slugify(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_dash = false;
    for ch in s.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}
