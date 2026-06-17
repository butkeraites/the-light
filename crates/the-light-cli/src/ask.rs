//! Subcomando `ask` — pergunta livre ancorada numa referência (RAG leve).
//!
//! `light ask "Como Paulo define a graça?" --ref "Rm 3"`.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Args;

use the_light_core::ai;
use the_light_core::config::Config;
use the_light_core::reference::{format_reference, parse_reference};
use the_light_core::xref;

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
            let resolved = match ai_common::resolve_passage(
                &store,
                &config,
                &reference,
                args.version.as_deref(),
            ) {
                Ok(p) => p,
                Err(code) => return code,
            };
            if !resolved.embeddable {
                eprintln!(
                    "(Versão protegida via conector — uso pessoal; o texto será enviado \
                     ao provedor de IA escolhido.)"
                );
            }
            let passage = resolved.passage;
            // Contexto RAG montado pelo núcleo (mesmo bloco usado pela TUI):
            // rótulo + versículos numerados + refs cruzadas de toda a passagem.
            let label = format_reference(&reference, lang);
            let numbered = ai::numbered_passage(&passage);
            let related =
                xref::passage_labels(store.conn(), &reference, &passage.verse_numbers(), lang, 8);
            ai::ask_context(&label, &numbered, &related)
        }
        None => {
            // `--db` só é usado com `--ref`; avisa se foi passado em vão.
            if args.db.is_some() {
                eprintln!("Aviso: `--db` é ignorado sem `--ref` (nenhum contexto a carregar).");
            }
            "(nenhuma referência fornecida)".to_string()
        }
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
            eprintln!("Dica: verifique a chave (`light config keys`) e a conexão.");
            ExitCode::from(EXIT_NOT_FOUND)
        }
    }
}
