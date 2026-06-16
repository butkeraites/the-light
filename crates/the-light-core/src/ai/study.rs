//! Orquestração de estudo e perguntas: monta o prompt (system da lente + texto
//! local + xrefs como RAG leve), chama o provedor e separa **texto citado** (do
//! banco) de **interpretação** (do modelo).

use crate::model::{Lang, Passage, Reference};

use super::{prompts, ChatMessage, ChatRole, Denomination, LlmProvider, Result, StudyDepth};

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

/// Monta o corpo da mensagem de usuário de uma pergunta ancorada num contexto.
pub(crate) fn ask_user_prompt(question: &str, context: &str) -> String {
    format!(
        "Pergunta: {question}\n\n\
         CONTEXTO (referências fornecidas — ancore a resposta nelas):\n{context}\n\n\
         Responda citando os versículos pertinentes; se o contexto não bastar, diga isso.",
    )
}

/// Pergunta livre ancorada num contexto de referências (RAG leve).
pub fn ask(
    provider: &dyn LlmProvider,
    question: &str,
    context: &str,
    lang: Lang,
) -> Result<String> {
    let system = prompts::ask_system_prompt(lang);
    provider.complete(&system, &ask_user_prompt(question, context))
}

/// Conversa multi-turno ancorada num contexto. `turns` é o histórico completo
/// (alternando usuário/assistente) terminando na pergunta mais recente do
/// usuário. O `context` (capítulo + refs) é embutido **apenas na primeira**
/// mensagem de usuário; os turnos seguintes ficam como puro diálogo, e a
/// memória da conversa carrega o contexto inicial.
pub fn ask_session(
    provider: &dyn LlmProvider,
    lang: Lang,
    context: &str,
    turns: &[ChatMessage],
) -> Result<String> {
    let system = prompts::ask_system_prompt(lang);
    let mut wrapped_first = false;
    let messages: Vec<ChatMessage> = turns
        .iter()
        .map(|m| {
            if m.role == ChatRole::User && !wrapped_first {
                wrapped_first = true;
                ChatMessage {
                    role: ChatRole::User,
                    content: ask_user_prompt(&m.content, context),
                }
            } else {
                m.clone()
            }
        })
        .collect();
    provider.chat(&system, &messages)
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

    /// Provedor de teste que devolve os conteúdos das mensagens unidos — permite
    /// inspecionar o que `ask_session` montou (sem rede).
    struct EchoProvider;
    impl LlmProvider for EchoProvider {
        fn name(&self) -> &str {
            "echo"
        }
        fn model(&self) -> &str {
            "echo-1"
        }
        fn complete(&self, _system: &str, user: &str) -> Result<String> {
            Ok(user.to_string())
        }
        fn chat(&self, _system: &str, messages: &[ChatMessage]) -> Result<String> {
            Ok(messages
                .iter()
                .map(|m| format!("[{}] {}", m.role.as_str(), m.content))
                .collect::<Vec<_>>()
                .join("\n---\n"))
        }
    }

    fn user(c: &str) -> ChatMessage {
        ChatMessage {
            role: ChatRole::User,
            content: c.to_string(),
        }
    }
    fn assistant(c: &str) -> ChatMessage {
        ChatMessage {
            role: ChatRole::Assistant,
            content: c.to_string(),
        }
    }

    #[test]
    fn ask_session_wraps_context_only_in_first_user_turn() {
        let turns = [user("q1"), assistant("a1"), user("q2")];
        let out = ask_session(&EchoProvider, Lang::Pt, "CTX-UNICO", &turns).unwrap();
        // O contexto aparece uma única vez (só no 1º turno de usuário).
        assert_eq!(out.matches("CTX-UNICO").count(), 1, "{out}");
        assert_eq!(out.matches("CONTEXTO").count(), 1, "{out}");
        // O follow-up "q2" chega cru (sem novo bloco de contexto).
        assert!(out.contains("[user] q2"), "{out}");
        // A resposta intermediária do assistente é preservada.
        assert!(out.contains("[assistant] a1"), "{out}");
    }

    #[test]
    fn chat_default_impl_folds_history_for_mock() {
        // MockLlmProvider não sobrescreve `chat`: usa o fold padrão e devolve fixo.
        let provider = MockLlmProvider::new("ok");
        let out = provider
            .chat("sys", &[user("oi"), assistant("olá"), user("e agora?")])
            .unwrap();
        assert_eq!(out, "ok");
    }

    #[test]
    fn chat_role_serde_roundtrip() {
        assert_eq!(serde_json::to_string(&ChatRole::User).unwrap(), "\"user\"");
        assert_eq!(
            serde_json::to_string(&ChatRole::Assistant).unwrap(),
            "\"assistant\""
        );
        let r: ChatRole = serde_json::from_str("\"assistant\"").unwrap();
        assert_eq!(r, ChatRole::Assistant);
    }
}
