//! System prompts por lente denominacional e profundidade.
//!
//! O prompt embutido pode ser **sobrescrito** colocando um arquivo
//! `prompts/<slug>.md` no diretório de config (ou em `LIGHT_PROMPTS`); se
//! presente, seu conteúdo substitui integralmente o system prompt daquela lente.

use std::path::Path;
// `PathBuf`/`AiError`/`Result` só entram em `prompts_dir`/`system_prompt`, que
// resolvem o diretório de override via `directories` → só `embedded`. A montagem
// dos prompts (base + override por `std::fs`) é pura e compila no wasm.
#[cfg(feature = "embedded")]
use std::path::PathBuf;

use crate::model::Lang;

#[cfg(feature = "embedded")]
use super::{AiError, Result};
use super::{Denomination, StudyDepth, StudyMode};

/// Uma seção do *blueprint* de um modo (cabeçalho que o modelo deve produzir).
pub struct SectionSpec {
    /// Slug estável da seção.
    pub key: &'static str,
    /// Cabeçalho em português (vira `## <heading>` na saída).
    pub heading_pt: &'static str,
}

/// Seções que o **modelo** deve produzir para cada modo, na ordem. As seções de
/// aparato (Notas/Bibliografia, no modo acadêmico) NÃO entram aqui — são geradas
/// de forma determinística a partir das citações (fase 3), nunca pelo modelo.
pub fn section_blueprint(mode: StudyMode) -> &'static [SectionSpec] {
    match mode {
        StudyMode::Academic => &[
            SectionSpec {
                key: "texto-traducao",
                heading_pt: "Texto e tradução",
            },
            SectionSpec {
                key: "critica-textual",
                heading_pt: "Crítica textual",
            },
            SectionSpec {
                key: "analise-lexical",
                heading_pt: "Análise lexical",
            },
            SectionSpec {
                key: "contexto",
                heading_pt: "Contexto histórico-literário",
            },
            SectionSpec {
                key: "estrutura",
                heading_pt: "Estrutura e argumento",
            },
            SectionSpec {
                key: "interpretacao",
                heading_pt: "História da interpretação",
            },
            SectionSpec {
                key: "sintese",
                heading_pt: "Síntese teológica",
            },
        ],
        StudyMode::Devotional => &[
            SectionSpec {
                key: "mensagem",
                heading_pt: "Mensagem central",
            },
            SectionSpec {
                key: "contexto",
                heading_pt: "Contexto breve",
            },
            SectionSpec {
                key: "aplicacao",
                heading_pt: "Aplicação para a vida",
            },
            SectionSpec {
                key: "meditacao",
                heading_pt: "Perguntas para meditação",
            },
            SectionSpec {
                key: "oracao",
                heading_pt: "Oração",
            },
        ],
        StudyMode::Introductory => &[
            SectionSpec {
                key: "do-que-trata",
                heading_pt: "Do que se trata",
            },
            SectionSpec {
                key: "quem-quando",
                heading_pt: "Quem, quando, para quem",
            },
            SectionSpec {
                key: "termos",
                heading_pt: "Termos explicados",
            },
            SectionSpec {
                key: "ideia",
                heading_pt: "Ideia principal",
            },
        ],
        StudyMode::Sermon => &[
            SectionSpec {
                key: "tema",
                heading_pt: "Tema e proposição",
            },
            SectionSpec {
                key: "esboco",
                heading_pt: "Esboço (pontos principais)",
            },
            SectionSpec {
                key: "ilustracoes",
                heading_pt: "Ilustrações e aplicação",
            },
            SectionSpec {
                key: "discussao",
                heading_pt: "Perguntas para discussão",
            },
            SectionSpec {
                key: "apelo",
                heading_pt: "Apelo e conclusão",
            },
        ],
    }
}

/// Diretriz de papel/tom específica do modo, anteposta à estrutura.
fn mode_directive(mode: StudyMode) -> &'static str {
    match mode {
        StudyMode::Academic => {
            "MODO ACADÊMICO: produza uma exegese rigorosa e fundamentada, em tom acadêmico e \
             sóbrio. Toda afirmação lexical (palavra no original, número de Strong, sentido) DEVE \
             apoiar-se em dados fornecidos no contexto; se não houver base fornecida, diga \
             explicitamente que não há, e NÃO invente. Sem devocional nem aplicação pastoral."
        }
        StudyMode::Devotional => {
            "MODO DEVOCIONAL: tom caloroso e pessoal. Foque no sentido para a vida e na resposta \
             do coração; evite jargão técnico e línguas originais."
        }
        StudyMode::Introductory => {
            "MODO INTRODUTÓRIO: para quem tem primeiro contato com a passagem (ou com a Bíblia). \
             Linguagem simples, explique todo termo difícil, dê a visão de conjunto; sem jargão."
        }
        StudyMode::Sermon => {
            "MODO PREGAÇÃO/ENSINO: organize a passagem para pregar ou ensinar — proposição clara, \
             pontos principais, ilustrações e aplicação; tom direto e comunicável."
        }
    }
}

/// Diretório de prompts override (`LIGHT_PROMPTS` ou `<config>/prompts`).
#[cfg(feature = "embedded")]
pub fn prompts_dir() -> Result<PathBuf> {
    if let Some(p) = std::env::var_os("LIGHT_PROMPTS") {
        return Ok(PathBuf::from(p));
    }
    let dirs = directories::ProjectDirs::from("", "", "light").ok_or(AiError::NoConfigDir)?;
    Ok(dirs.config_dir().join("prompts"))
}

fn language_line(lang: Lang) -> &'static str {
    match lang {
        Lang::Pt => "Responda inteiramente em português.",
        Lang::En => "Respond entirely in English.",
    }
}

/// Descrição da tradição (distintivos hermenêuticos), em português.
fn lens_description(lens: Denomination) -> &'static str {
    match lens {
        Denomination::Baptist => {
            "Tradição BATISTA: autoridade final das Escrituras (sola scriptura), \
             salvação pela graça mediante a fé, sacerdócio de todos os crentes, \
             batismo de crentes por imersão (não pedobatismo), autonomia da igreja local \
             e separação igreja-estado."
        }
        Denomination::Presbyterian => {
            "Tradição PRESBITERIANA/REFORMADA: teologia da aliança (pactual), soberania \
             de Deus, eleição e graça (cânones de Dort/TULIP), confissão de Westminster, \
             batismo infantil como sinal da aliança e governo presbiteral."
        }
        Denomination::Lutheran => {
            "Tradição LUTERANA: justificação somente pela fé (sola fide), Lei e Evangelho \
             como chave hermenêutica, presença real na Ceia, batismo regenerador, \
             e as confissões do Livro de Concórdia."
        }
        Denomination::Pentecostal => {
            "Tradição PENTECOSTAL: batismo no Espírito Santo com evidência de dons \
             (incl. línguas), atualidade dos dons espirituais (continuísmo), ênfase em \
             cura e experiência, e leitura restauracionista de Atos."
        }
        Denomination::Catholic => {
            "Tradição CATÓLICA ROMANA: Escritura e Tradição sob o Magistério, sete \
             sacramentos, transubstanciação, papel de Maria e dos santos, e a leitura \
             segundo o Catecismo da Igreja Católica."
        }
        Denomination::Orthodox => {
            "Tradição ORTODOXA ORIENTAL: Sagrada Tradição e os sete concílios ecumênicos, \
             teose (divinização), interpretação patrística, mistérios (sacramentos) e a \
             vida litúrgica como contexto de leitura."
        }
    }
}

fn depth_guidance(depth: StudyDepth) -> &'static str {
    match depth {
        StudyDepth::Overview => {
            "Profundidade: VISÃO GERAL. Resuma o sentido principal, o contexto imediato e \
             uma aplicação prática, de forma acessível."
        }
        StudyDepth::Exegetical => {
            "Profundidade: EXEGÉTICA. Analise contexto histórico-literário, estrutura do \
             trecho, gênero, conexões canônicas e termos-chave; fundamente cada afirmação."
        }
        StudyDepth::WordStudy => {
            "Profundidade: ESTUDO DE PALAVRAS. Destaque termos no original (grego/hebraico) \
             com transliteração e sentido, e como informam a interpretação."
        }
    }
}

/// Lista numerada dos cabeçalhos de seção do modo (para a estrutura obrigatória).
fn blueprint_headings(mode: StudyMode) -> String {
    section_blueprint(mode)
        .iter()
        .enumerate()
        .map(|(i, s)| format!("{}. ## {}", i + 1, s.heading_pt))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Constrói o system prompt embutido (base + modo + lente + profundidade + idioma).
fn builtin(mode: StudyMode, lens: Denomination, depth: StudyDepth, lang: Lang) -> String {
    format!(
        "Você é um assistente de estudo bíblico ({modo}).\n\n\
         REGRAS OBRIGATÓRIAS:\n\
         1. Cite os versículos por referência (ex.: Ef 2.8) ao fundamentar afirmações.\n\
         2. Separe claramente o TEXTO BÍBLICO citado da sua INTERPRETAÇÃO.\n\
         3. Deixe explícito que esta é a leitura da tradição {tradicao}; não a apresente \
            como a única visão cristã.\n\
         4. Quando houver divergência relevante entre tradições cristãs, sinalize-a.\n\
         5. NÃO invente versículos, referências, números de Strong ou citações.\n\
         6. {idioma}\n\n\
         {diretriz}\n\n\
         ESTRUTURA OBRIGATÓRIA — produza EXATAMENTE estes cabeçalhos de seção (cada um numa \
         linha começando por `## `), nesta ordem, sem texto antes do primeiro cabeçalho e sem \
         seções extras:\n{headings}\n\n\
         {descricao}\n\n\
         {profundidade}",
        modo = mode.name_pt(),
        tradicao = lens.name_pt(),
        idioma = language_line(lang),
        diretriz = mode_directive(mode),
        headings = blueprint_headings(mode),
        descricao = lens_description(lens),
        profundidade = depth_guidance(depth),
    )
}

/// System prompt para perguntas livres ancoradas em referências (RAG).
pub fn ask_system_prompt(lang: Lang) -> String {
    format!(
        "Você é um assistente de estudo bíblico. Responda à pergunta ancorando-se \
         SOMENTE no texto e nas referências fornecidas; cite os versículos pertinentes; \
         se a resposta não estiver no contexto, diga isso claramente; NÃO invente \
         versículos ou referências. {}",
        language_line(lang)
    )
}

/// System prompt para um **follow-up dentro de um estudo**: preserva o modo e a
/// lente para que as respostas sigam a mesma voz hermenêutica e fundamentação.
pub fn study_followup_system_prompt(mode: StudyMode, lens: Denomination, lang: Lang) -> String {
    format!(
        "Você é um assistente de estudo bíblico continuando um estudo {modo} sob a lente \
         {lente}. Mantenha essa voz hermenêutica e aprofunde a partir do estudo já \
         apresentado no histórico. Ancore-se no texto e nas referências fornecidas; cite \
         os versículos; se algo não estiver no contexto, diga isso; NÃO invente versículos \
         ou referências. {idioma}",
        modo = mode.name_pt(),
        lente = lens.name_pt(),
        idioma = language_line(lang),
    )
}

/// System prompt para um modo/lente/profundidade, honrando override local.
///
/// Resolve o diretório de override via `prompts_dir()` (`directories`) → só
/// `embedded`. No wasm, use [`system_prompt_in`] com um `override_dir` explícito
/// (ou `None`, caindo no prompt embutido).
#[cfg(feature = "embedded")]
pub fn system_prompt(mode: StudyMode, lens: Denomination, depth: StudyDepth, lang: Lang) -> String {
    let dir = prompts_dir().ok();
    system_prompt_in(mode, lens, depth, lang, dir.as_deref())
}

/// Variante com diretório de override explícito (para testes).
///
/// Cadeia de override (o primeiro arquivo encontrado substitui o prompt embutido):
/// `<modo>.<lente>.md` → `<modo>.md` → `<lente>.md` (legado) → embutido.
pub fn system_prompt_in(
    mode: StudyMode,
    lens: Denomination,
    depth: StudyDepth,
    lang: Lang,
    override_dir: Option<&Path>,
) -> String {
    if let Some(dir) = override_dir {
        for name in [
            format!("{}.{}.md", mode.slug(), lens.slug()),
            format!("{}.md", mode.slug()),
            format!("{}.md", lens.slug()),
        ] {
            if let Ok(content) = std::fs::read_to_string(dir.join(&name)) {
                return content;
            }
        }
    }
    builtin(mode, lens, depth, lang)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_has_mandatory_rules_and_lens() {
        let p = system_prompt_in(
            StudyMode::Academic,
            Denomination::Baptist,
            StudyDepth::Exegetical,
            Lang::Pt,
            None,
        );
        assert!(p.contains("Cite os versículos"));
        assert!(p.contains("Separe claramente"));
        assert!(p.contains("NÃO invente"));
        assert!(p.contains("Batista"));
        assert!(p.contains("EXEGÉTICA"));
        assert!(p.contains("português"));
        // A estrutura do modo aparece (cabeçalhos obrigatórios).
        assert!(p.contains("## Análise lexical"));
        assert!(p.contains("ESTRUTURA OBRIGATÓRIA"));
    }

    #[test]
    fn each_lens_is_distinct() {
        let mut seen = std::collections::HashSet::new();
        for lens in Denomination::all() {
            let p = system_prompt_in(
                StudyMode::Academic,
                lens,
                StudyDepth::Overview,
                Lang::Pt,
                None,
            );
            assert!(
                seen.insert(p),
                "lente {} gerou prompt duplicado",
                lens.slug()
            );
        }
    }

    #[test]
    fn each_mode_is_distinct() {
        // Smoke test: todo par modo×lente gera um prompt coerente e único.
        let mut seen = std::collections::HashSet::new();
        for mode in StudyMode::all() {
            for lens in Denomination::all() {
                let p = system_prompt_in(mode, lens, mode.implied_depth(), Lang::Pt, None);
                assert!(
                    p.contains(mode.name_pt()),
                    "modo {} ausente do prompt",
                    mode.slug()
                );
                // Cada par é distinto (modo e lente realmente compõem).
                assert!(
                    seen.insert(p),
                    "par {}×{} gerou prompt duplicado",
                    mode.slug(),
                    lens.slug()
                );
            }
        }
    }

    #[test]
    fn english_language_line() {
        let p = system_prompt_in(
            StudyMode::Devotional,
            Denomination::Lutheran,
            StudyDepth::Overview,
            Lang::En,
            None,
        );
        assert!(p.contains("English"));
    }

    #[test]
    fn override_file_replaces_prompt() {
        let dir = tempfile::tempdir().unwrap();
        // Override legado por lente (`<lente>.md`) ainda funciona (3º na cadeia).
        std::fs::write(
            dir.path().join("presbyterian.md"),
            "PROMPT CUSTOMIZADO DO USUÁRIO",
        )
        .unwrap();
        let p = system_prompt_in(
            StudyMode::Academic,
            Denomination::Presbyterian,
            StudyDepth::Exegetical,
            Lang::Pt,
            Some(dir.path()),
        );
        assert_eq!(p, "PROMPT CUSTOMIZADO DO USUÁRIO");
        // Outra lente sem override continua embutida.
        let q = system_prompt_in(
            StudyMode::Academic,
            Denomination::Baptist,
            StudyDepth::Overview,
            Lang::Pt,
            Some(dir.path()),
        );
        assert!(q.contains("Batista"));
    }

    #[test]
    fn override_mode_lens_takes_precedence() {
        let dir = tempfile::tempdir().unwrap();
        // `<modo>.<lente>.md` vence `<modo>.md` e `<lente>.md`.
        std::fs::write(dir.path().join("academic.presbyterian.md"), "ESPECÍFICO").unwrap();
        std::fs::write(dir.path().join("academic.md"), "SÓ MODO").unwrap();
        std::fs::write(dir.path().join("presbyterian.md"), "SÓ LENTE").unwrap();
        let p = system_prompt_in(
            StudyMode::Academic,
            Denomination::Presbyterian,
            StudyDepth::Exegetical,
            Lang::Pt,
            Some(dir.path()),
        );
        assert_eq!(p, "ESPECÍFICO");
        // Modo diferente cai no `<modo>.md` (devotional) — não existe, então usa `<lente>.md`.
        let q = system_prompt_in(
            StudyMode::Devotional,
            Denomination::Presbyterian,
            StudyDepth::Overview,
            Lang::Pt,
            Some(dir.path()),
        );
        assert_eq!(q, "SÓ LENTE");
    }
}
