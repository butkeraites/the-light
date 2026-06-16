//! Subcomando `ask` — pergunta livre ancorada numa referência (RAG leve).
//!
//! `biblia ask "Como Paulo define a graça?" --ref "Rm 3"`.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Args;

use biblia_core::ai;
use biblia_core::config::Config;
use biblia_core::reference::{format_reference, parse_reference};
use biblia_core::source::EmbeddedSource;

use crate::ai_common;

/// Argumentos do subcomando `ask`.
#[derive(Args)]
pub struct AskArgs {
    /// Pergunta.
    pub question: String,

    /// Referência de contexto (PT/EN), ex.: "Rm 3".
    #[arg(long)]
    pub r#ref: Option<String>,

    /// Versão para o contexto (CLI > config > kjv).
    #[arg(short, long)]
    pub version: Option<String>,

    /// Provedor de IA (CLI > config). Use `mock` para demonstração offline.
    #[arg(long)]
    pub provider: Option<String>,

    /// Modelo específico (sobrescreve o padrão do provedor).
    #[arg(long)]
    pub model: Option<String>,

    /// Caminho do banco (padrão: diretório de dados do usuário).
    #[arg(long)]
    pub db: Option<PathBuf>,
}

const EXIT_OK: u8 = 0;
const EXIT_NOT_FOUND: u8 = 1;
const EXIT_USAGE: u8 = 2;

/// Executa o comando `ask`.
pub fn run(args: AskArgs) -> ExitCode {
    let config = Config::load().unwrap_or_default();
    let lang = config.language;

    // Monta o contexto a partir da referência (se houver).
    let context = match args.r#ref.as_deref() {
        Some(ref_str) => {
            let reference = match parse_reference(ref_str) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("Referência inválida: {e}");
                    return ExitCode::from(EXIT_USAGE);
                }
            };
            let store = match ai_common::open_store(args.db.as_deref()) {
                Ok(s) => s,
                Err(code) => return code,
            };
            let src = EmbeddedSource::new(&store);
            let passage = match ai_common::resolve_passage(
                &src,
                &reference,
                args.version.as_deref(),
                &config,
            ) {
                Ok(p) => p,
                Err(code) => return code,
            };
            let label = format_reference(&reference, lang);
            let mut ctx = format!("{label}:\n");
            for v in &passage.verses {
                let n = match v.reference.verses {
                    biblia_core::model::VerseRange::Single(n) => n,
                    biblia_core::model::VerseRange::Range { start, .. } => start,
                    biblia_core::model::VerseRange::WholeChapter => 0,
                };
                ctx.push_str(&format!("{n} {}\n", v.text));
            }
            let xrefs = ai_common::xref_labels(&store, &reference, lang, 8);
            if !xrefs.is_empty() {
                ctx.push_str(&format!("\nReferências relacionadas: {}", xrefs.join("; ")));
            }
            ctx
        }
        None => "(nenhuma referência fornecida)".to_string(),
    };

    let provider =
        match ai_common::resolve_provider(args.provider.clone(), args.model.clone(), &config) {
            Ok(p) => p,
            Err(code) => return code,
        };

    match ai::ask(provider.as_ref(), &args.question, &context, lang) {
        Ok(answer) => {
            println!("{answer}");
            ExitCode::from(EXIT_OK)
        }
        Err(e) => {
            eprintln!("Falha na pergunta: {e}");
            eprintln!("Dica: verifique a chave (`biblia config keys`) e a conexão.");
            ExitCode::from(EXIT_NOT_FOUND)
        }
    }
}
