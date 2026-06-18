//! Subcomando `study` — estudo exegético de uma passagem por lente denominacional.
//!
//! Uma lente: `light study "Ef 2.8-9" --lens presbiteriana`.
//! Comparar lentes: `light study "Ef 2.8-9" --lens batista,luterana`.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Args;

use the_light_core::ai::{
    self, Denomination, StudyDepth, StudyMode, StudyRequest, VerifiedLexicon,
};
use the_light_core::config::Config;
use the_light_core::reference::{format_reference, parse_reference};
use the_light_core::userdata;
use the_light_core::xref;

use crate::ai_common;

/// Argumentos do subcomando `study`.
#[derive(Args)]
pub struct StudyArgs {
    /// Passagem (PT/EN), ex.: "Ef 2.8-9".
    pub reference: String,

    /// Modo: academic | devotional | introductory | sermon (padrão: config).
    #[arg(long)]
    pub mode: Option<String>,

    /// Salva o `--mode` informado como o modo padrão (em `config.toml`).
    #[arg(long)]
    pub remember: bool,

    /// Lente(s) denominacional(is), separadas por vírgula (compara se >1).
    #[arg(long, default_value = "presbiteriana")]
    pub lens: String,

    /// Profundidade: geral | exegetico | palavras (padrão: implícita do modo).
    #[arg(long)]
    pub depth: Option<String>,

    /// Versão para o texto citado (CLI > config > kjv).
    #[arg(short, long)]
    pub version: Option<String>,

    /// Provedor de IA (CLI > config). Use `mock` para demonstração offline.
    #[arg(long)]
    pub provider: Option<String>,

    /// Modelo específico (sobrescreve o padrão do provedor).
    #[arg(long)]
    pub model: Option<String>,

    /// Salva o estudo em `studies/` (Markdown + sidecar `.citations.json`).
    #[arg(long)]
    pub save: bool,

    /// Imprime o paper acadêmico (notas SBL + bibliografia) no stdout.
    #[arg(long)]
    pub academic: bool,

    /// Exporta o paper acadêmico para um arquivo (.md/.pdf/.docx via pandoc).
    #[arg(long)]
    pub export: Option<PathBuf>,

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

    let config = Config::load().unwrap_or_default();
    let lang = config.language;

    // Modo: CLI > config. Profundidade: CLI > implícita do modo.
    let mode: StudyMode = match args.mode.as_deref() {
        Some(s) => match s.parse() {
            Ok(m) => m,
            Err(e) => {
                eprintln!("{e}");
                return ExitCode::from(EXIT_USAGE);
            }
        },
        None => config.study_mode,
    };

    let depth: StudyDepth = match args.depth.as_deref() {
        Some(s) => match s.parse() {
            Ok(d) => d,
            Err(e) => {
                eprintln!("{e}");
                return ExitCode::from(EXIT_USAGE);
            }
        },
        None => mode.implied_depth(),
    };

    // `--remember` grava o modo escolhido como padrão (não-fatal se falhar).
    if args.remember {
        let mut cfg = config.clone();
        cfg.study_mode = mode;
        match cfg.save() {
            Ok(()) => eprintln!("Modo padrão salvo: {}", mode.name_pt()),
            Err(e) => eprintln!("Aviso: não foi possível salvar o modo padrão: {e}"),
        }
    }

    let store = match ai_common::open_store(args.db.as_deref()) {
        Ok(s) => s,
        Err(code) => return code,
    };

    let resolved =
        match ai_common::resolve_passage(&store, &config, &reference, args.version.as_deref()) {
            Ok(p) => p,
            Err(code) => return code,
        };
    let passage = resolved.passage;
    let protected = !resolved.embeddable;
    if protected {
        eprintln!(
            "(Versão protegida via conector — uso pessoal; respeite os limites de citação. \
             O texto será enviado ao provedor de IA escolhido e NÃO será salvo em disco.)"
        );
    }

    // Modos acadêmico/pregação puxam mais xrefs e dados léxicos verificados.
    let xref_limit = if mode.wants_lexical() { 16 } else { 8 };
    let cross_references = xref::passage_labels(
        store.conn(),
        &reference,
        &passage.verse_numbers(),
        lang,
        xref_limit,
    );
    let verified_lexicon = if mode.wants_lexical() {
        ai::verified_lexicon(store.conn(), &reference, &passage.verse_numbers(), lang, 16)
    } else {
        VerifiedLexicon::default()
    };

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
    let mut export_parts: Vec<String> = Vec::new();

    for (i, lens) in lenses.iter().enumerate() {
        let req = StudyRequest {
            reference,
            reference_label: label.clone(),
            mode,
            lens: *lens,
            depth,
            language: lang,
            passage: &passage,
            cross_references: cross_references.clone(),
            verified_lexicon: verified_lexicon.clone(),
        };
        match ai::study(provider.as_ref(), &req) {
            Ok(result) => {
                if i > 0 {
                    println!("\n———\n");
                }
                // stdout: paper acadêmico se pedido, senão o estudo legível.
                if args.academic {
                    print!("{}", result.to_academic_markdown(lang));
                } else {
                    print!("{}", result.to_markdown());
                }
                if args.export.is_some() {
                    export_parts.push(result.to_academic_markdown(lang));
                }
                for w in &result.warnings {
                    eprintln!("⚠ {w}");
                }
                print_cost(provider.as_ref(), &result);
                succeeded += 1;
                if args.save {
                    if protected {
                        // Nunca persistir texto protegido (efêmero — SPEC §5.2).
                        eprintln!(
                            "Não salvo: estudos de versões protegidas não são gravados \
                             (texto efêmero). Use uma versão livre para salvar."
                        );
                    } else if save_study(&result).is_err() {
                        save_failed = true;
                    }
                }
            }
            Err(e) => {
                eprintln!("Falha no estudo ({}): {e}", lens.name_pt());
                eprintln!("Dica: verifique a chave (`light config keys`) e a conexão.");
                failed += 1;
            }
        }
    }

    // Exporta o paper acadêmico (junta as lentes com separador, se houver mais de uma).
    let mut export_failed = false;
    if let Some(path) = &args.export {
        if export_parts.is_empty() {
            eprintln!("Nada a exportar (nenhuma lente produziu resultado).");
            export_failed = true;
        } else {
            let doc = export_parts.join("\n\n---\n\n");
            match crate::export::export_document(&doc, path) {
                Ok(()) => println!("Paper exportado para {}", path.display()),
                Err(msg) => {
                    eprintln!("{msg}");
                    export_failed = true;
                }
            }
        }
    }

    // Em comparação de lentes, deixa claro o resultado parcial; saída != 0 se algo
    // falhou (convenção Unix), mesmo que parte tenha sido impressa.
    if failed > 0 && succeeded > 0 {
        eprintln!("Atenção: {failed} de {total} lentes falharam (as demais foram impressas).");
    }
    let exit = if failed > 0 || save_failed || export_failed {
        EXIT_NOT_FOUND
    } else {
        EXIT_OK
    };
    ExitCode::from(exit)
}

/// Imprime uma estimativa de tokens/custo (não-fatal).
fn print_cost(provider: &dyn the_light_core::ai::LlmProvider, result: &ai::StudyResult) {
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
        "{}-{}-{}",
        result.reference_label,
        result.mode.slug(),
        result.lens.slug()
    ));
    let path = dir.join(format!("{slug}.md"));
    if let Err(e) = the_light_core::util::atomic_write(&path, result.to_markdown().as_bytes()) {
        eprintln!("Erro ao gravar o estudo: {e}");
        return Err(EXIT_NOT_FOUND);
    }
    // Sidecar com as citações verificáveis (permite re-exportar sem re-rodar a IA).
    if !result.citations.is_empty() {
        if let Ok(json) = the_light_core::ai::citation::to_json(&result.citations) {
            let sidecar = dir.join(format!("{slug}.citations.json"));
            let _ = the_light_core::util::atomic_write(&sidecar, json.as_bytes());
        }
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
