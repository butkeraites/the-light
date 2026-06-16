//! Orquestração de estudo e perguntas: monta o prompt (system da lente + texto
//! local + xrefs como RAG leve), chama o provedor e separa **texto citado** (do
//! banco) de **interpretação** (do modelo).

use crate::model::{Lang, Passage, Reference};

use super::{prompts, Denomination, LlmProvider, Result, StudyDepth};

/// Pedido de estudo de uma passagem.
pub struct StudyRequest<'a> {
    /// Referência da passagem.
    pub reference: Reference,
    /// Referência formatada (ex.: "Ef 2.8-9").
    pub reference_label: String,
    /// Lente denominacional.
    pub lens: Denomination,
    /// Profundidade.
    pub depth: StudyDepth,
    /// Idioma da resposta.
    pub language: Lang,
    /// Passagem (texto do banco local).
    pub passage: &'a Passage,
    /// Referências cruzadas locais (rótulos), usadas como contexto.
    pub cross_references: Vec<String>,
}

/// Resultado de um estudo: separa o texto citado (banco) da interpretação (LLM).
pub struct StudyResult {
    /// Referência estudada.
    pub reference: Reference,
    /// Referência formatada.
    pub reference_label: String,
    /// Lente usada.
    pub lens: Denomination,
    /// Profundidade.
    pub depth: StudyDepth,
    /// Idioma.
    pub language: Lang,
    /// Texto citado (numerado), extraído do banco local — sempre fiel.
    pub passage_text: String,
    /// Interpretação gerada pelo modelo.
    pub interpretation: String,
    /// Provedor usado.
    pub provider: String,
    /// Modelo usado.
    pub model: String,
}

/// Texto da passagem numerado por versículo (uma linha por versículo).
///
/// Sempre vem do acervo local (anti-alucinação). O `unwrap_or(0)` é uma guarda
/// defensiva: na prática os versículos de uma `Passage` carregam sempre um
/// número (`VerseRange::Single`).
pub fn numbered_passage(passage: &Passage) -> String {
    passage
        .verses
        .iter()
        .map(|v| {
            let n = v.reference.verses.start().unwrap_or(0);
            format!("{n} {}", v.text)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn user_prompt(req: &StudyRequest, passage_text: &str) -> String {
    let xrefs = if req.cross_references.is_empty() {
        "(nenhuma)".to_string()
    } else {
        req.cross_references.join("; ")
    };
    format!(
        "Faça um estudo bíblico da passagem {referencia}, pela lente {lente}, \
         profundidade {prof}.\n\n\
         TEXTO DA PASSAGEM (acervo local — cite por número de versículo):\n{texto}\n\n\
         REFERÊNCIAS CRUZADAS LOCAIS (contexto, use se ajudar):\n{xrefs}\n\n\
         Produza a interpretação seguindo as regras do sistema (citar versículos, \
         separar texto de interpretação, marcar a lente, sinalizar divergências).",
        referencia = req.reference_label,
        lente = req.lens.name_pt(),
        prof = req.depth.name_pt(),
        texto = passage_text,
        xrefs = xrefs,
    )
}

/// Executa o estudo: chama o provedor e devolve um [`StudyResult`].
pub fn study(provider: &dyn LlmProvider, req: &StudyRequest) -> Result<StudyResult> {
    let passage_text = numbered_passage(req.passage);
    let system = prompts::system_prompt(req.lens, req.depth, req.language);
    let user = user_prompt(req, &passage_text);
    let interpretation = provider.complete(&system, &user)?;
    Ok(StudyResult {
        reference: req.reference,
        reference_label: req.reference_label.clone(),
        lens: req.lens,
        depth: req.depth,
        language: req.language,
        passage_text,
        interpretation,
        provider: provider.name().to_string(),
        model: provider.model().to_string(),
    })
}

impl StudyResult {
    /// Renderiza o estudo em Markdown (texto citado + interpretação + aviso).
    pub fn to_markdown(&self) -> String {
        format!(
            "# Estudo — {referencia}\n\n\
             - Lente: {lente}\n\
             - Profundidade: {prof}\n\
             - Gerado por: {provider}/{model}\n\n\
             ## Texto citado\n\n{texto}\n\n\
             ## Interpretação ({lente})\n\n{interp}\n\n\
             ---\n\
             _Texto bíblico do acervo local; a interpretação é gerada por IA sob a lente \
             {lente} e pode conter erros — confira sempre as Escrituras._\n",
            referencia = self.reference_label,
            lente = self.lens.name_pt(),
            prof = self.depth.name_pt(),
            provider = self.provider,
            model = self.model,
            texto = self.passage_text,
            interp = self.interpretation,
        )
    }
}

/// Pergunta livre ancorada num contexto de referências (RAG leve).
pub fn ask(
    provider: &dyn LlmProvider,
    question: &str,
    context: &str,
    lang: Lang,
) -> Result<String> {
    let system = prompts::ask_system_prompt(lang);
    let user = format!(
        "Pergunta: {question}\n\n\
         CONTEXTO (referências fornecidas — ancore a resposta nelas):\n{context}\n\n\
         Responda citando os versículos pertinentes; se o contexto não bastar, diga isso.",
    );
    provider.complete(&system, &user)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::MockLlmProvider;
    use crate::model::{TranslationId, Verse, VerseRange};

    fn passage() -> Passage {
        let r = Reference {
            book: 49,
            chapter: 2,
            verses: VerseRange::Range { start: 8, end: 9 },
        };
        Passage {
            reference: r,
            verses: vec![
                Verse {
                    reference: Reference::single(49, 2, 8),
                    text: "Porque pela graça sois salvos".to_string(),
                    translation: TranslationId::new("alm"),
                },
                Verse {
                    reference: Reference::single(49, 2, 9),
                    text: "Não vem das obras".to_string(),
                    translation: TranslationId::new("alm"),
                },
            ],
        }
    }

    #[test]
    fn study_cites_local_text_and_includes_interpretation() {
        let p = passage();
        let req = StudyRequest {
            reference: p.reference,
            reference_label: "Efésios 2.8-9".to_string(),
            lens: Denomination::Lutheran,
            depth: StudyDepth::Exegetical,
            language: Lang::Pt,
            passage: &p,
            cross_references: vec!["Romanos 3.24".to_string()],
        };
        let provider = MockLlmProvider::new("A salvação é dom de Deus (v.8).");
        let result = study(&provider, &req).unwrap();

        // Texto citado vem do banco (numerado), não do modelo.
        assert!(result
            .passage_text
            .contains("8 Porque pela graça sois salvos"));
        assert!(result.passage_text.contains("9 Não vem das obras"));
        assert_eq!(result.interpretation, "A salvação é dom de Deus (v.8).");
        assert_eq!(result.provider, "mock");

        let md = result.to_markdown();
        assert!(md.contains("# Estudo — Efésios 2.8-9"));
        assert!(md.contains("Lente: Luterana"));
        assert!(md.contains("## Texto citado"));
        assert!(md.contains("## Interpretação"));
        assert!(md.contains("A salvação é dom de Deus"));
        assert!(md.contains("confira sempre as Escrituras"));
    }

    #[test]
    fn ask_returns_provider_answer() {
        let provider = MockLlmProvider::new("Resposta ancorada.");
        let answer = ask(&provider, "O que é graça?", "Ef 2.8 ...", Lang::Pt).unwrap();
        assert_eq!(answer, "Resposta ancorada.");
    }
}
