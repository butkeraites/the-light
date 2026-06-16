//! System prompts por lente denominacional e profundidade.
//!
//! O prompt embutido pode ser **sobrescrito** colocando um arquivo
//! `prompts/<slug>.md` no diretório de config (ou em `LIGHT_PROMPTS`); se
//! presente, seu conteúdo substitui integralmente o system prompt daquela lente.

use std::path::{Path, PathBuf};

use crate::model::Lang;

use super::{AiError, Denomination, Result, StudyDepth};

/// Diretório de prompts override (`LIGHT_PROMPTS` ou `<config>/prompts`).
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

/// Constrói o system prompt embutido (base + lente + profundidade + idioma).
fn builtin(lens: Denomination, depth: StudyDepth, lang: Lang) -> String {
    format!(
        "Você é um assistente de estudo bíblico exegético.\n\n\
         REGRAS OBRIGATÓRIAS:\n\
         1. Cite os versículos por referência (ex.: Ef 2.8) ao fundamentar afirmações.\n\
         2. Separe claramente o TEXTO BÍBLICO citado da sua INTERPRETAÇÃO.\n\
         3. Deixe explícito que esta é a leitura da tradição {tradicao}; não a apresente \
            como a única visão cristã.\n\
         4. Quando houver divergência relevante entre tradições cristãs, sinalize-a.\n\
         5. NÃO invente versículos, referências, números de Strong ou citações.\n\
         6. {idioma}\n\n\
         {descricao}\n\n\
         {profundidade}",
        tradicao = lens.name_pt(),
        idioma = language_line(lang),
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

/// System prompt para uma lente/profundidade, honrando override local.
pub fn system_prompt(lens: Denomination, depth: StudyDepth, lang: Lang) -> String {
    let dir = prompts_dir().ok();
    system_prompt_in(lens, depth, lang, dir.as_deref())
}

/// Variante com diretório de override explícito (para testes).
pub fn system_prompt_in(
    lens: Denomination,
    depth: StudyDepth,
    lang: Lang,
    override_dir: Option<&Path>,
) -> String {
    if let Some(dir) = override_dir {
        let path = dir.join(format!("{}.md", lens.slug()));
        if let Ok(content) = std::fs::read_to_string(&path) {
            return content;
        }
    }
    builtin(lens, depth, lang)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_has_mandatory_rules_and_lens() {
        let p = system_prompt_in(
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
    }

    #[test]
    fn each_lens_is_distinct() {
        let mut seen = std::collections::HashSet::new();
        for lens in Denomination::all() {
            let p = system_prompt_in(lens, StudyDepth::Overview, Lang::Pt, None);
            assert!(
                seen.insert(p),
                "lente {} gerou prompt duplicado",
                lens.slug()
            );
        }
    }

    #[test]
    fn english_language_line() {
        let p = system_prompt_in(Denomination::Lutheran, StudyDepth::Overview, Lang::En, None);
        assert!(p.contains("English"));
    }

    #[test]
    fn override_file_replaces_prompt() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("presbyterian.md"),
            "PROMPT CUSTOMIZADO DO USUÁRIO",
        )
        .unwrap();
        let p = system_prompt_in(
            Denomination::Presbyterian,
            StudyDepth::Exegetical,
            Lang::Pt,
            Some(dir.path()),
        );
        assert_eq!(p, "PROMPT CUSTOMIZADO DO USUÁRIO");
        // Outra lente sem override continua embutida.
        let q = system_prompt_in(
            Denomination::Baptist,
            StudyDepth::Overview,
            Lang::Pt,
            Some(dir.path()),
        );
        assert!(q.contains("Batista"));
    }
}
