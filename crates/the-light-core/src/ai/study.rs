//! Orquestração de estudo e perguntas: monta o prompt (system da lente + texto
//! local + xrefs como RAG leve), chama o provedor e separa **texto citado** (do
//! banco) de **interpretação** (do modelo).

use crate::model::{Lang, Passage};

use super::{prompts, ChatMessage, ChatRole, Denomination, LlmProvider, Result, StudyMode};

// Superfície PESADA (deep-study, Fase 3/4): `StudyRequest`/`StudyResult`/`study()`
// e os renders dependem de `WebSource` (chrono via `research`), do `lexicon` de
// banco e do aparato de `citation` → só sob `embedded`. A superfície pura da
// Fase 2 (numeração, RAG, `ask`, refinamento) não referencia nada disto.
#[cfg(feature = "embedded")]
use super::citation::{self, Citation, CitationCollector, CitationKind};
#[cfg(feature = "embedded")]
use super::lexicon::{self, VerifiedLexicon};
#[cfg(feature = "embedded")]
use super::research::WebSource;
#[cfg(feature = "embedded")]
use super::StudyDepth;
#[cfg(feature = "embedded")]
use crate::model::Reference;
#[cfg(feature = "embedded")]
use std::collections::HashSet;

/// Pedido de estudo de uma passagem.
#[cfg(feature = "embedded")]
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
    /// Passagem (texto do banco local). `None` = estudo **temático** (sem
    /// passagem única) — o assunto vem de `brief` e o modelo cita as referências
    /// que discutir.
    pub passage: Option<&'a Passage>,
    /// Referências cruzadas locais (rótulos), usadas como contexto.
    pub cross_references: Vec<String>,
    /// Dados léxicos verificados (línguas originais + Strong) — injetados no
    /// prompt apenas quando há passagem e `mode.wants_lexical()`. Vazio quando não
    /// há cobertura ou o modo não usa léxico.
    pub verified_lexicon: VerifiedLexicon,
    /// Fontes secundárias da web (pesquisa opt-in). Vazio = offline (padrão).
    /// Quando presentes, são injetadas no prompt e citáveis por `[W:n]`.
    pub web_sources: Vec<WebSource>,
    /// Foco do estudo definido pelo usuário (prompt + refinamento da TUI).
    /// Injetado no prompt quando presente; é o assunto de um estudo temático.
    pub brief: Option<String>,
}

/// Uma seção estruturada da interpretação (cabeçalho `## ` + corpo).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StudySection {
    /// Cabeçalho da seção (sem o `## `).
    pub heading: String,
    /// Corpo da seção (texto entre este cabeçalho e o próximo).
    pub body: String,
}

/// Uma rodada de refinamento de escopo: pergunta + opções de múltipla escolha.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refinement {
    /// Pergunta de refinamento proposta pelo modelo.
    pub question: String,
    /// Opções sugeridas (o usuário também pode digitar a sua própria).
    pub options: Vec<String>,
}

/// Resultado de um estudo: separa o texto citado (banco) da interpretação (LLM).
#[cfg(feature = "embedded")]
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
    /// Avisos de verificação (ex.: Strong citado fora do acervo). Vazio quando
    /// nada foi sinalizado — preserva a saída histórica byte a byte.
    pub warnings: Vec<String>,
    /// Citações verificáveis (léxico + fontes) para o aparato acadêmico.
    /// Construídas do banco, NUNCA pelo modelo. Vazio fora do modo acadêmico.
    pub citations: Vec<Citation>,
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

#[cfg(feature = "embedded")]
fn user_prompt(req: &StudyRequest, passage_text: &str) -> String {
    let xrefs = if req.cross_references.is_empty() {
        "(nenhuma)".to_string()
    } else {
        req.cross_references.join("; ")
    };
    // Bloco léxico verificado só com passagem e nos modos que o pedem.
    let lexical = if req.passage.is_some() && req.mode.wants_lexical() {
        format!(
            "\nDADOS LÉXICOS:\n{}\n\nAo tratar de termos no original, cite EXCLUSIVAMENTE os \
             Strong do bloco acima usando a marca [V:NÚMERO]; se o bloco indicar que não há \
             dados, declare isso e NÃO invente números, lemas ou sentidos.\n",
            lexicon::format_verified_block(&req.verified_lexicon, req.language),
        )
    } else {
        String::new()
    };
    // Bloco de fontes secundárias (pesquisa web opt-in), citáveis por [W:n].
    let web = if req.web_sources.is_empty() {
        String::new()
    } else {
        let mut b = String::from(
            "\nFONTES SECUNDÁRIAS (recuperadas da web — cite por [W:n]; use SOMENTE estas, \
             NÃO invente URLs nem extrapole além do trecho):\n",
        );
        for (i, ws) in req.web_sources.iter().enumerate() {
            b.push_str(&format!(
                "[W:{}] {} ({}) — {}\n",
                i + 1,
                ws.title,
                ws.site,
                ws.snippet
            ));
        }
        b
    };
    // Foco do estudo (prompt do usuário + refinamento), quando houver.
    let brief = match req.brief.as_deref().map(str::trim) {
        Some(b) if !b.is_empty() => {
            format!("\n\nFOCO DO ESTUDO (definido pelo usuário — atenda exatamente a isto):\n{b}")
        }
        _ => String::new(),
    };

    if req.passage.is_some() {
        format!(
            "Faça um estudo bíblico ({modo}) da passagem {referencia}, pela lente {lente}, \
             profundidade {prof}.{brief}\n\n\
             TEXTO DA PASSAGEM (acervo local — cite por número de versículo):\n{texto}\n\n\
             REFERÊNCIAS CRUZADAS LOCAIS (contexto, use se ajudar):\n{xrefs}\n{lexical}{web}\n\
             Produza a interpretação seguindo as regras do sistema (estrutura de seções do modo, \
             citar versículos, separar texto de interpretação, marcar a lente, sinalizar divergências).",
            modo = req.mode.name_pt(),
            referencia = req.reference_label,
            lente = req.lens.name_pt(),
            prof = req.depth.name_pt(),
            brief = brief,
            texto = passage_text,
            xrefs = xrefs,
            lexical = lexical,
            web = web,
        )
    } else {
        // Estudo temático: sem passagem fixada; o modelo abrange as passagens
        // pertinentes e CITA cada referência que discutir.
        format!(
            "Faça um estudo bíblico ({modo}) TEMÁTICO, pela lente {lente}, profundidade {prof}.{brief}\n\n\
             Não há uma única passagem fixada: trate o tema abrangendo as passagens bíblicas \
             pertinentes e CITE cada referência que discutir (ex.: Ef 2.8). NÃO invente versículos \
             nem referências.\n\
             REFERÊNCIAS RELACIONADAS (contexto, use se ajudar):\n{xrefs}\n{web}\n\
             Produza a interpretação seguindo as regras do sistema (estrutura de seções do modo, \
             citar versículos, separar texto de interpretação, marcar a lente, sinalizar divergências).",
            modo = req.mode.name_pt(),
            lente = req.lens.name_pt(),
            prof = req.depth.name_pt(),
            brief = brief,
            xrefs = xrefs,
            web = web,
        )
    }
}

/// Índices de fontes web citadas (`[W:n]`) num texto, para validar o intervalo.
#[cfg(feature = "embedded")]
fn cited_web_indices(text: &str) -> Vec<usize> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(pos) = rest.find("[W:") {
        let after = &rest[pos + 3..];
        match after.find(']') {
            Some(close) => {
                let tok = &after[..close];
                if !tok.is_empty() && tok.chars().all(|c| c.is_ascii_digit()) {
                    if let Ok(n) = tok.parse::<usize>() {
                        out.push(n);
                    }
                }
                rest = &after[close + 1..];
            }
            None => break,
        }
    }
    out
}

/// Executa o estudo: chama o provedor e devolve um [`StudyResult`].
///
/// Deep-study (Fase 3): usa `system_prompt` (override em disco), léxico de banco
/// e aparato de citação → só `embedded`. O `ask` ancorado da Fase 2 (abaixo) é a
/// superfície pura usada no wasm.
#[cfg(feature = "embedded")]
pub fn study(provider: &dyn LlmProvider, req: &StudyRequest) -> Result<StudyResult> {
    let passage_text = req.passage.map(numbered_passage).unwrap_or_default();
    let system = prompts::system_prompt(req.mode, req.lens, req.depth, req.language);
    let user = user_prompt(req, &passage_text);
    let interpretation = provider.complete(&system, &user)?;
    let sections = split_sections(&interpretation);
    // Verificação anti-alucinação: sinaliza Strong citados fora do acervo
    // (política `flag` — não altera o texto). Só com passagem + léxico.
    let mut warnings = if req.passage.is_some() && req.mode.wants_lexical() {
        lexicon::verify(&interpretation, &req.verified_lexicon).warnings
    } else {
        Vec::new()
    };
    // Fontes web citadas fora do intervalo (anti-fabricação de [W:n]).
    if !req.web_sources.is_empty() {
        for n in cited_web_indices(&interpretation) {
            if n == 0 || n > req.web_sources.len() {
                warnings.push(format!(
                    "Fonte web citada [W:{n}] fora do intervalo (há {} fonte(s))",
                    req.web_sources.len()
                ));
            }
        }
    }
    // Citações verificáveis (só no modo com aparato). Construídas do banco/URLs.
    let citations = if req.mode.emits_apparatus() {
        let mut c = CitationCollector::new();
        c.from_verified_lexicon(&req.verified_lexicon);
        c.from_web_results(&req.web_sources);
        c.into_vec()
    } else {
        Vec::new()
    };
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
        warnings,
        citations,
        provider: provider.name().to_string(),
        model: provider.model().to_string(),
    })
}

/// Parser tolerante de uma rodada: `PERGUNTA: …` (ou a 1ª linha não-opção) vira a
/// pergunta; linhas `- …`/`* …` viram opções (deduplicadas, sem vazias).
pub fn parse_refinement(raw: &str) -> Refinement {
    let mut question = String::new();
    let mut options: Vec<String> = Vec::new();
    for line in raw.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if let Some(q) = t
            .strip_prefix("PERGUNTA:")
            .or_else(|| t.strip_prefix("Pergunta:"))
        {
            question = q.trim().to_string();
        } else if let Some(o) = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")) {
            let o = o.trim();
            if !o.is_empty() && !options.iter().any(|x| x == o) {
                options.push(o.to_string());
            }
        } else if question.is_empty() {
            question = t.to_string();
        }
    }
    Refinement { question, options }
}

fn refine_system_prompt(mode: StudyMode, lang: Lang, round: u8) -> String {
    let last = if round >= 3 {
        " Esta é a ÚLTIMA rodada: faça as opções convergirem para uma PASSAGEM concreta \
         (ex.: \"Efésios 2.8-9\") ou \"estudo temático\"."
    } else {
        ""
    };
    let idioma = match lang {
        Lang::Pt => "Escreva em português.",
        Lang::En => "Write in English.",
    };
    format!(
        "Você ajuda a delimitar o ESCOPO de um estudo bíblico ({modo}). Proponha UMA pergunta \
         curta para refinar o escopo e de 3 a 4 opções de resposta.{last} Responda EXATAMENTE \
         neste formato, sem nada além disso:\n\
         PERGUNTA: <pergunta>\n- <opção 1>\n- <opção 2>\n- <opção 3>\n{idioma}",
        modo = mode.name_pt(),
        last = last,
        idioma = idioma,
    )
}

fn refine_user_prompt(brief: &str, prior: &[(String, String)], round: u8) -> String {
    let history = if prior.is_empty() {
        "(nenhuma)".to_string()
    } else {
        prior
            .iter()
            .map(|(q, a)| format!("- {q} → {a}"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    format!(
        "ASSUNTO DO ESTUDO: {brief}\n\nRESPOSTAS ANTERIORES:\n{history}\n\n\
         Proponha a próxima pergunta de refinamento (rodada {round} de 3).",
    )
}

/// Pede ao modelo UMA rodada de refinamento de escopo: uma pergunta + opções,
/// dado o assunto (`brief`) e as respostas anteriores. `round` em 1..=3.
pub fn refine_scope(
    provider: &dyn LlmProvider,
    mode: StudyMode,
    lang: Lang,
    brief: &str,
    prior: &[(String, String)],
    round: u8,
) -> Result<Refinement> {
    let system = refine_system_prompt(mode, lang, round);
    let user = refine_user_prompt(brief, prior, round);
    let raw = provider.complete(&system, &user)?;
    Ok(parse_refinement(&raw))
}

#[cfg(feature = "embedded")]
impl StudyResult {
    /// Renderiza o estudo em Markdown (texto citado + interpretação + aviso).
    ///
    /// Quando o modelo não usou cabeçalhos (`sections` vazio), usa o **formato
    /// histórico, byte a byte** — contrato preservado pelos testes existentes.
    /// Com seções, renderiza a estrutura do modo (e acrescenta a linha `- Modo:`).
    pub fn to_markdown(&self) -> String {
        // Bloco de texto citado — ausente nos estudos temáticos (sem passagem).
        let texto_block = if self.passage_text.is_empty() {
            String::new()
        } else {
            format!("## Texto citado\n\n{}\n\n", self.passage_text)
        };

        if self.sections.is_empty() {
            let mut out = format!(
                "# Estudo — {referencia}\n\n\
                 - Lente: {lente}\n\
                 - Profundidade: {prof}\n\
                 - Gerado por: {provider}/{model}\n\n\
                 {texto}## Interpretação ({lente})\n\n{interp}\n\n\
                 ---\n\
                 _Texto bíblico do acervo local; a interpretação é gerada por IA sob a lente \
                 {lente} e pode conter erros — confira sempre as Escrituras._\n",
                referencia = self.reference_label,
                lente = self.lens.name_pt(),
                prof = self.depth.name_pt(),
                provider = self.provider,
                model = self.model,
                texto = texto_block,
                interp = self.interpretation,
            );
            out.push_str(&self.warnings_block());
            return out;
        }

        let mut out = format!(
            "# Estudo — {referencia}\n\n\
             - Modo: {modo}\n\
             - Lente: {lente}\n\
             - Profundidade: {prof}\n\
             - Gerado por: {provider}/{model}\n\n\
             {texto}",
            referencia = self.reference_label,
            modo = self.mode.name_pt(),
            lente = self.lens.name_pt(),
            prof = self.depth.name_pt(),
            provider = self.provider,
            model = self.model,
            texto = texto_block,
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
        out.push_str(&self.warnings_block());
        out
    }

    /// Bloco Markdown de avisos de verificação (vazio quando não há avisos —
    /// mantém a saída byte a byte no caminho histórico).
    fn warnings_block(&self) -> String {
        if self.warnings.is_empty() {
            return String::new();
        }
        let mut s = String::from("\n## Avisos de verificação\n\n");
        for w in &self.warnings {
            s.push_str(&format!("- {w}\n"));
        }
        s
    }

    /// Renderiza o estudo como um **paper acadêmico**: metadados (YAML p/ pandoc),
    /// texto citado, análise com notas de rodapé SBL, bibliografia e rodapé de
    /// procedência. As notas são ancoradas de forma **determinística** a partir
    /// das citações verificáveis — o modelo só emite âncoras `[V:Strong]`; aqui
    /// validamos e trocamos por marcadores `[^chave]`, descartando as inválidas.
    pub fn to_academic_markdown(&self, lang: Lang) -> String {
        let valid: HashSet<String> = self
            .citations
            .iter()
            .filter(|c| matches!(c.kind, CitationKind::Lexicon | CitationKind::Web))
            .map(|c| c.key.clone())
            .collect();

        // Análise: seções (com âncoras reescritas) ou interpretação única.
        let analysis = if self.sections.is_empty() {
            format!(
                "## Análise\n\n{}\n\n",
                citation::rewrite_anchors(&self.interpretation, &valid)
            )
        } else {
            let mut a = String::new();
            for s in &self.sections {
                a.push_str(&format!(
                    "## {}\n\n{}\n\n",
                    s.heading,
                    citation::rewrite_anchors(&s.body, &valid)
                ));
            }
            a
        };

        // Texto citado — ausente nos estudos temáticos (sem passagem única).
        let texto_block = if self.passage_text.is_empty() {
            String::new()
        } else {
            format!("## Texto (acervo local)\n\n{}\n\n", self.passage_text)
        };
        let mut out = format!(
            "---\n\
             title: \"Estudo Exegético — {referencia}\"\n\
             author: \"The Light — {provider}/{model}\"\n\
             lang: \"{langcode}\"\n\
             ---\n\n\
             {texto}{analysis}",
            referencia = self.reference_label,
            provider = self.provider,
            model = self.model,
            langcode = lang.code(),
            texto = texto_block,
            analysis = analysis,
        );

        // Notas: somente as citações (léxico + web) referenciadas no texto.
        let notes: Vec<&Citation> = self
            .citations
            .iter()
            .filter(|c| matches!(c.kind, CitationKind::Lexicon | CitationKind::Web))
            .filter(|c| analysis.contains(&format!("[^{}]", c.key)))
            .collect();
        if !notes.is_empty() {
            out.push_str("## Notas\n\n");
            for c in &notes {
                out.push_str(&format!(
                    "[^{}]: {}\n\n",
                    c.key,
                    citation::sbl_footnote(c, lang)
                ));
            }
        }

        // Bibliografia (obras de referência + web), alfabética.
        let mut bib: Vec<String> = self
            .citations
            .iter()
            .filter(|c| c.kind.in_bibliography())
            .map(citation::sbl_bibliography_entry)
            .collect();
        bib.sort();
        bib.dedup();
        if !bib.is_empty() {
            out.push_str("## Bibliografia\n\n");
            for b in &bib {
                out.push_str(&format!("- {b}\n"));
            }
            out.push('\n');
        }

        out.push_str(&citation::provenance_footer(
            &self.citations,
            &self.provider,
            &self.model,
            lang,
        ));
        out.push_str(&self.warnings_block());
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
    study: Option<(StudyMode, Denomination)>,
) -> Result<String> {
    // Follow-up de um estudo usa um system prompt ciente do modo/lente.
    let system = match study {
        Some((mode, lens)) => prompts::study_followup_system_prompt(mode, lens, lang),
        None => prompts::ask_system_prompt(lang),
    };
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
            passage: Some(&p),
            cross_references: vec!["Romanos 3.24".to_string()],
            verified_lexicon: VerifiedLexicon::default(),
            web_sources: vec![],
            brief: None,
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
            passage: Some(&p),
            cross_references: vec![],
            verified_lexicon: VerifiedLexicon::default(),
            web_sources: vec![],
            brief: None,
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
    fn academic_mode_injects_lexical_block_and_flags_invented_strongs() {
        let p = passage();
        let vl = VerifiedLexicon {
            entries: vec![lexicon::LexicalEntry {
                strongs: "H7225".into(),
                lemma: Some("rēʾšît".into()),
                translit: None,
                gloss: Some("beginning".into()),
                occurrences: 1,
                testament: "OT".into(),
            }],
            sources: vec!["STEP Bible".into()],
        };
        let mk = |mode: StudyMode, vl: VerifiedLexicon| StudyRequest {
            reference: p.reference,
            reference_label: "Efésios 2.8-9".to_string(),
            mode,
            lens: Denomination::Presbyterian,
            depth: StudyDepth::Exegetical,
            language: Lang::Pt,
            passage: Some(&p),
            cross_references: vec![],
            verified_lexicon: vl,
            web_sources: vec![],
            brief: None,
        };

        // Echo devolve o user_prompt: o bloco léxico é injetado no modo acadêmico.
        let echoed = study(&EchoProvider, &mk(StudyMode::Academic, vl.clone())).unwrap();
        assert!(echoed.interpretation.contains("DADOS LÉXICOS"));
        assert!(echoed.interpretation.contains("[V:H7225]"));

        // Modo devocional NÃO injeta léxico nem verifica.
        let devo = study(&EchoProvider, &mk(StudyMode::Devotional, vl.clone())).unwrap();
        assert!(!devo.interpretation.contains("DADOS LÉXICOS"));
        assert!(devo.warnings.is_empty());

        // Um Strong inventado fora do acervo é sinalizado e aparece no Markdown.
        let inv = MockLlmProvider::new("Veja [V:H7225] (ok) e [V:G9999] (inventado).");
        let flagged = study(&inv, &mk(StudyMode::Academic, vl)).unwrap();
        assert_eq!(flagged.warnings.len(), 1, "{:?}", flagged.warnings);
        assert!(flagged.warnings[0].contains("G9999"));
        assert!(flagged.to_markdown().contains("## Avisos de verificação"));
    }

    #[test]
    fn academic_markdown_has_footnotes_bibliography_and_provenance() {
        let p = passage();
        let vl = VerifiedLexicon {
            entries: vec![lexicon::LexicalEntry {
                strongs: "H7225".into(),
                lemma: Some("rēʾšît".into()),
                translit: None,
                gloss: Some("beginning".into()),
                occurrences: 1,
                testament: "OT".into(),
            }],
            sources: vec!["STEP Bible (CC BY 4.0)".into()],
        };
        let req = StudyRequest {
            reference: p.reference,
            reference_label: "Gênesis 1.1".to_string(),
            mode: StudyMode::Academic,
            lens: Denomination::Presbyterian,
            depth: StudyDepth::Exegetical,
            language: Lang::Pt,
            passage: Some(&p),
            cross_references: vec![],
            verified_lexicon: vl,
            web_sources: vec![],
            brief: None,
        };
        let provider = MockLlmProvider::new("## Análise lexical\nO termo [V:H7225] abre o relato.");
        let r = study(&provider, &req).unwrap();
        let md = r.to_academic_markdown(Lang::Pt);

        assert!(md.contains("title: \"Estudo Exegético — Gênesis 1.1\""));
        assert!(md.contains("## Texto (acervo local)"));
        assert!(md.contains("[^H7225]"), "âncora deve virar nota");
        assert!(!md.contains("[V:H7225]"), "âncora original some");
        assert!(md.contains("## Notas"));
        assert!(md.contains("s.v."));
        assert!(md.contains("## Bibliografia"));
        assert!(md.contains("Tyndale House"));
        assert!(md.contains("Gerado por IA"));
    }

    #[test]
    fn research_injects_web_block_cites_and_flags_out_of_range() {
        use crate::ai::research::{MockResearchProvider, ResearchProvider};
        let p = passage();
        let web = MockResearchProvider::canned().search("x", 5).unwrap(); // 2 fontes
        let mk = |prov: &dyn LlmProvider| {
            let req = StudyRequest {
                reference: p.reference,
                reference_label: "Ef 2.8".to_string(),
                mode: StudyMode::Academic,
                lens: Denomination::Presbyterian,
                depth: StudyDepth::Exegetical,
                language: Lang::Pt,
                passage: Some(&p),
                cross_references: vec![],
                verified_lexicon: VerifiedLexicon::default(),
                web_sources: web.clone(),
                brief: None,
            };
            study(prov, &req).unwrap()
        };

        // O bloco de fontes secundárias entra no prompt (Echo devolve o prompt).
        let echoed = mk(&EchoProvider);
        assert!(echoed.interpretation.contains("FONTES SECUNDÁRIAS"));
        assert!(echoed.interpretation.contains("[W:1]"));
        let web_cites = echoed
            .citations
            .iter()
            .filter(|c| c.kind == CitationKind::Web)
            .count();
        assert_eq!(web_cites, 2);

        // Cita [W:1] (ok) e [W:3] (fora do intervalo — só há 2 fontes).
        let inv = MockLlmProvider::new("## Análise\nVer [W:1] e também [W:3].");
        let flagged = mk(&inv);
        assert!(flagged.warnings.iter().any(|w| w.contains("[W:3]")));
        let md = flagged.to_academic_markdown(Lang::Pt);
        // A análise foi reescrita: [W:1]→nota, [W:3] removido (o aviso, à parte,
        // ainda menciona [W:3] — é o relatório de verificação).
        assert!(md.contains("Ver [^W1] e também"), "{md}");
        // Trecho verbatim aparece na nota da fonte web.
        assert!(md.contains("unmerited favor"));
    }

    #[test]
    fn topical_study_has_no_cited_text_block_and_injects_brief() {
        // Estudo temático: passagem None → sem "Texto citado"; o brief entra no prompt.
        let p = passage();
        let req = StudyRequest {
            reference: p.reference,
            reference_label: "a graça em Paulo".to_string(),
            mode: StudyMode::Introductory,
            lens: Denomination::Presbyterian,
            depth: StudyDepth::Overview,
            language: Lang::Pt,
            passage: None,
            cross_references: vec![],
            verified_lexicon: VerifiedLexicon::default(),
            web_sources: vec![],
            brief: Some("a graça em Paulo".to_string()),
        };
        // Echo devolve o user_prompt → confirma TEMÁTICO + FOCO.
        let echoed = study(&EchoProvider, &req).unwrap();
        assert!(echoed.interpretation.contains("TEMÁTICO"));
        assert!(echoed.interpretation.contains("FOCO DO ESTUDO"));
        assert!(echoed.passage_text.is_empty());

        // Com seções, o Markdown NÃO inclui "## Texto citado".
        let provider = MockLlmProvider::new("## Ideia principal\nA graça é dom.");
        let r = study(&provider, &req).unwrap();
        let md = r.to_markdown();
        assert!(!md.contains("## Texto citado"), "{md}");
        assert!(md.contains("## Ideia principal"));
    }

    #[test]
    fn parse_refinement_reads_question_and_options() {
        let raw = "PERGUNTA: Qual o foco?\n- exegese de uma passagem\n- tema geral\n- estudo de palavra\n- estudo de palavra";
        let r = parse_refinement(raw);
        assert_eq!(r.question, "Qual o foco?");
        assert_eq!(r.options.len(), 3, "dedup: {:?}", r.options); // duplicata removida
        assert_eq!(r.options[0], "exegese de uma passagem");
        // Sem cabeçalho PERGUNTA: a 1ª linha não-opção vira a pergunta.
        let r2 = parse_refinement("Escolha o ângulo\n* histórico\n* teológico");
        assert_eq!(r2.question, "Escolha o ângulo");
        assert_eq!(r2.options, vec!["histórico", "teológico"]);
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
        let out = ask_session(&EchoProvider, Lang::Pt, "CTX-UNICO", &turns, None).unwrap();
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
