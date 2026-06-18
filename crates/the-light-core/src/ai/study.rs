//! Orquestração de estudo e perguntas: monta o prompt (system da lente + texto
//! local + xrefs como RAG leve), chama o provedor e separa **texto citado** (do
//! banco) de **interpretação** (do modelo).

use crate::model::{Lang, Passage, Reference};

use super::{
    prompts, ChatMessage, ChatRole, Denomination, LlmProvider, Result, StudyDepth, StudyMode,
};

/// Pedido de estudo de uma passagem.
pub struct StudyRequest<'a> {
    /// Referência da passagem.
    pub reference: Reference,
    /// Referência formatada (ex.: "Ef 2.8-9").
    pub reference_label: String,
    /// Modo de estudo (molda estrutura e tom).
    pub mode: StudyMode,
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

/// Uma seção estruturada da interpretação (cabeçalho `## ` + corpo).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StudySection {
    /// Cabeçalho da seção (sem o `## `).
    pub heading: String,
    /// Corpo da seção (texto entre este cabeçalho e o próximo).
    pub body: String,
}

/// Resultado de um estudo: separa o texto citado (banco) da interpretação (LLM).
pub struct StudyResult {
    /// Referência estudada.
    pub reference: Reference,
    /// Referência formatada.
    pub reference_label: String,
    /// Modo usado.
    pub mode: StudyMode,
    /// Lente usada.
    pub lens: Denomination,
    /// Profundidade.
    pub depth: StudyDepth,
    /// Idioma.
    pub language: Lang,
    /// Texto citado (numerado), extraído do banco local — sempre fiel.
    pub passage_text: String,
    /// Interpretação gerada pelo modelo (texto bruto, completo).
    pub interpretation: String,
    /// Interpretação fatiada por seção (`## `). Vazio quando o modelo não usou
    /// cabeçalhos — nesse caso `to_markdown` cai no formato histórico.
    pub sections: Vec<StudySection>,
    /// Provedor usado.
    pub provider: String,
    /// Modelo usado.
    pub model: String,
}

/// Numera versículos `(número, texto)` numa linha cada (`"{n} {texto}"`). Base
/// comum da numeração local (anti-alucinação) para a CLI e a TUI.
pub fn numbered_verses<'a>(verses: impl IntoIterator<Item = (u16, &'a str)>) -> String {
    verses
        .into_iter()
        .map(|(n, text)| format!("{n} {text}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Texto da passagem numerado por versículo (uma linha por versículo).
///
/// Sempre vem do acervo local (anti-alucinação). O `unwrap_or(0)` é uma guarda
/// defensiva: na prática os versículos de uma `Passage` carregam sempre um
/// número (`VerseRange::Single`).
pub fn numbered_passage(passage: &Passage) -> String {
    numbered_verses(
        passage
            .verses
            .iter()
            .map(|v| (v.reference.verses.start().unwrap_or(0), v.text.as_str())),
    )
}

/// Monta o **bloco de contexto RAG** de uma pergunta ancorada: rótulo da
/// referência, versículos numerados (acervo local) e referências relacionadas.
/// Função pura — o mesmo bloco serve ao `ask` da CLI e à conversa da TUI. `related`
/// vazio vira "(nenhuma)" (marcação explícita anti-alucinação).
pub fn ask_context(label: &str, numbered_passage: &str, related: &[String]) -> String {
    let related = if related.is_empty() {
        "(nenhuma)".to_string()
    } else {
        related.join("; ")
    };
    format!("{label}:\n{numbered_passage}\n\nReferências relacionadas: {related}")
}

/// Fatia o texto do modelo em seções por cabeçalho `## `. Linhas antes do
/// primeiro cabeçalho são descartadas (o prompt proíbe preâmbulo). Devolve
/// **vazio** quando não há nenhum `## ` — sinal para `to_markdown` usar o
/// formato histórico (compatibilidade byte-a-byte).
pub fn split_sections(raw: &str) -> Vec<StudySection> {
    let mut sections: Vec<StudySection> = Vec::new();
    let mut cur: Option<StudySection> = None;
    for line in raw.lines() {
        if let Some(heading) = line.strip_prefix("## ") {
            if let Some(done) = cur.take() {
                sections.push(done);
            }
            cur = Some(StudySection {
                heading: heading.trim().to_string(),
                body: String::new(),
            });
        } else if let Some(s) = cur.as_mut() {
            // Evita acumular linhas em branco logo após o cabeçalho.
            if !s.body.is_empty() || !line.trim().is_empty() {
                s.body.push_str(line);
                s.body.push('\n');
            }
        }
    }
    if let Some(done) = cur.take() {
        sections.push(done);
    }
    for s in sections.iter_mut() {
        s.body = s.body.trim_end().to_string();
    }
    sections
}

fn user_prompt(req: &StudyRequest, passage_text: &str) -> String {
    let xrefs = if req.cross_references.is_empty() {
        "(nenhuma)".to_string()
    } else {
        req.cross_references.join("; ")
    };
    format!(
        "Faça um estudo bíblico ({modo}) da passagem {referencia}, pela lente {lente}, \
         profundidade {prof}.\n\n\
         TEXTO DA PASSAGEM (acervo local — cite por número de versículo):\n{texto}\n\n\
         REFERÊNCIAS CRUZADAS LOCAIS (contexto, use se ajudar):\n{xrefs}\n\n\
         Produza a interpretação seguindo as regras do sistema (estrutura de seções do modo, \
         citar versículos, separar texto de interpretação, marcar a lente, sinalizar divergências).",
        modo = req.mode.name_pt(),
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
    let system = prompts::system_prompt(req.mode, req.lens, req.depth, req.language);
    let user = user_prompt(req, &passage_text);
    let interpretation = provider.complete(&system, &user)?;
    let sections = split_sections(&interpretation);
    Ok(StudyResult {
        reference: req.reference,
        reference_label: req.reference_label.clone(),
        mode: req.mode,
        lens: req.lens,
        depth: req.depth,
        language: req.language,
        passage_text,
        interpretation,
        sections,
        provider: provider.name().to_string(),
        model: provider.model().to_string(),
    })
}

impl StudyResult {
    /// Renderiza o estudo em Markdown (texto citado + interpretação + aviso).
    ///
    /// Quando o modelo não usou cabeçalhos (`sections` vazio), usa o **formato
    /// histórico, byte a byte** — contrato preservado pelos testes existentes.
    /// Com seções, renderiza a estrutura do modo (e acrescenta a linha `- Modo:`).
    pub fn to_markdown(&self) -> String {
        if self.sections.is_empty() {
            return format!(
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
            );
        }

        let mut out = format!(
            "# Estudo — {referencia}\n\n\
             - Modo: {modo}\n\
             - Lente: {lente}\n\
             - Profundidade: {prof}\n\
             - Gerado por: {provider}/{model}\n\n\
             ## Texto citado\n\n{texto}\n\n",
            referencia = self.reference_label,
            modo = self.mode.name_pt(),
            lente = self.lens.name_pt(),
            prof = self.depth.name_pt(),
            provider = self.provider,
            model = self.model,
            texto = self.passage_text,
        );
        for s in &self.sections {
            out.push_str(&format!("## {}\n\n{}\n\n", s.heading, s.body));
        }
        out.push_str(&format!(
            "---\n_Texto bíblico do acervo local; a interpretação é gerada por IA sob a lente \
             {lente} (modo {modo}) e pode conter erros — confira sempre as Escrituras._\n",
            lente = self.lens.name_pt(),
            modo = self.mode.name_pt(),
        ));
        out
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
            mode: StudyMode::Academic,
            lens: Denomination::Lutheran,
            depth: StudyDepth::Exegetical,
            language: Lang::Pt,
            passage: &p,
            cross_references: vec!["Romanos 3.24".to_string()],
        };
        // Resposta sem cabeçalhos `## ` → caminho histórico (compatibilidade).
        let provider = MockLlmProvider::new("A salvação é dom de Deus (v.8).");
        let result = study(&provider, &req).unwrap();

        // Texto citado vem do banco (numerado), não do modelo.
        assert!(result
            .passage_text
            .contains("8 Porque pela graça sois salvos"));
        assert!(result.passage_text.contains("9 Não vem das obras"));
        assert_eq!(result.interpretation, "A salvação é dom de Deus (v.8).");
        assert_eq!(result.provider, "mock");
        assert!(result.sections.is_empty(), "sem `## ` → sem seções");

        let md = result.to_markdown();
        assert!(md.contains("# Estudo — Efésios 2.8-9"));
        assert!(md.contains("Lente: Luterana"));
        assert!(md.contains("## Texto citado"));
        assert!(md.contains("## Interpretação"));
        assert!(md.contains("A salvação é dom de Deus"));
        assert!(md.contains("confira sempre as Escrituras"));
        // Invariante: o caminho histórico NÃO inclui a linha de modo.
        assert!(
            !md.contains("- Modo:"),
            "caminho histórico deve ser byte-idêntico"
        );
    }

    #[test]
    fn structured_sections_render_mode_and_headings() {
        let p = passage();
        let req = StudyRequest {
            reference: p.reference,
            reference_label: "Efésios 2.8-9".to_string(),
            mode: StudyMode::Academic,
            lens: Denomination::Presbyterian,
            depth: StudyDepth::Exegetical,
            language: Lang::Pt,
            passage: &p,
            cross_references: vec![],
        };
        let provider = MockLlmProvider::new(
            "## Texto e tradução\nO texto fala da graça.\n\n## Síntese teológica\nA fé é dom.",
        );
        let result = study(&provider, &req).unwrap();

        assert_eq!(result.sections.len(), 2);
        assert_eq!(result.sections[0].heading, "Texto e tradução");
        assert_eq!(result.sections[0].body, "O texto fala da graça.");
        assert_eq!(result.sections[1].heading, "Síntese teológica");

        let md = result.to_markdown();
        assert!(md.contains("- Modo: Acadêmico"));
        assert!(md.contains("## Texto citado"));
        assert!(md.contains("## Texto e tradução"));
        assert!(md.contains("## Síntese teológica"));
        assert!(md.contains("confira sempre as Escrituras"));
    }

    #[test]
    fn split_sections_drops_preamble_and_trims() {
        let raw = "preâmbulo ignorado\n## Um\nlinha a\nlinha b\n\n## Dois\nsó isto\n";
        let s = split_sections(raw);
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].heading, "Um");
        assert_eq!(s[0].body, "linha a\nlinha b");
        assert_eq!(s[1].heading, "Dois");
        assert_eq!(s[1].body, "só isto");
        // Sem cabeçalhos → vazio (gatilho do formato histórico).
        assert!(split_sections("sem cabeçalho nenhum").is_empty());
    }

    #[test]
    fn ask_returns_provider_answer() {
        let provider = MockLlmProvider::new("Resposta ancorada.");
        let answer = ask(&provider, "O que é graça?", "Ef 2.8 ...", Lang::Pt).unwrap();
        assert_eq!(answer, "Resposta ancorada.");
    }

    #[test]
    fn numbered_verses_and_passage_agree() {
        // A base comum numera tuplas; `numbered_passage` apenas a alimenta com os
        // versículos da `Passage` — os dois devem coincidir byte a byte.
        let expected = "8 Porque pela graça sois salvos\n9 Não vem das obras";
        assert_eq!(numbered_passage(&passage()), expected);
        let from_tuples = numbered_verses([
            (8u16, "Porque pela graça sois salvos"),
            (9, "Não vem das obras"),
        ]);
        assert_eq!(from_tuples, expected);
    }

    #[test]
    fn ask_context_assembles_the_rag_block() {
        let numbered = "23 For all have sinned\n24 Being justified";
        let ctx = ask_context("Romans 3", numbered, &["Romans 6:23".to_string()]);
        assert_eq!(
            ctx,
            "Romans 3:\n23 For all have sinned\n24 Being justified\n\n\
             Referências relacionadas: Romans 6:23"
        );
        // Sem refs relacionadas: marcação explícita "(nenhuma)".
        let empty = ask_context("Romans 3", numbered, &[]);
        assert!(
            empty.ends_with("Referências relacionadas: (nenhuma)"),
            "{empty}"
        );
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
