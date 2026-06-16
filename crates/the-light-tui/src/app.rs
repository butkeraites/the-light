//! Estado da TUI e tratamento de teclas (lógica pura, testável).

use std::sync::mpsc::Receiver;

use anyhow::{anyhow, Result};
use ratatui::crossterm::event::{KeyCode, KeyEvent};

use the_light_core::ai::{self, build_provider, KeyStore, LlmProvider, PROVIDERS};
use the_light_core::config::Config;
use the_light_core::model::{Lang, Reference, SearchHit, TranslationId};
use the_light_core::reference::{format_reference, parse_reference, scan_references, BOOKS};
use the_light_core::search::{self, SearchOptions};
use the_light_core::source::{BibleSource, EmbeddedSource};
use the_light_core::store::Store;
use the_light_core::userdata::{Highlight, HighlightStore, Note, NoteStore};
use the_light_core::xref::{self, CrossRef};

/// Temas disponíveis, ciclados pela tecla `t` e persistidos em `config.toml`.
pub const THEMES: &[&str] = &["dark", "light", "none"];

/// Resultado de uma leitura de capítulo: `(num_capítulos, capítulo, versículos)`.
type ChapterData = (u16, u16, Vec<(u16, String)>);

/// Qual painel tem o foco do teclado.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    /// Lista de livros.
    Books,
    /// Viewport de leitura.
    Reader,
}

/// Tipo de entrada em curso na barra inferior.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputKind {
    /// Ir para uma referência.
    GoTo,
    /// Busca full-text incremental.
    Search,
    /// Pergunta livre à IA sobre o capítulo atual.
    Ask,
}

/// Estado de uma entrada de texto (prompt inferior).
#[derive(Debug, Clone)]
pub struct Input {
    /// O que está sendo digitado.
    pub kind: InputKind,
    /// Texto digitado até agora.
    pub buffer: String,
    /// Mensagem de erro (ex.: referência inválida).
    pub error: Option<String>,
}

/// Navegação na lista de referências cruzadas do versículo atual.
#[derive(Debug, Clone)]
pub struct XrefNav {
    /// Referências cruzadas do versículo, da mais votada para a menos.
    pub items: Vec<CrossRef>,
    /// Item selecionado.
    pub selected: usize,
}

/// Corpo do painel de pergunta à IA.
#[derive(Debug, Clone)]
pub enum AiBody {
    /// Consulta em andamento (thread de background).
    Pending,
    /// Resposta recebida.
    Answer(String),
    /// Erro (sem provedor/chave, rede, etc.).
    Error(String),
}

/// Overlay de pergunta à IA sobre o texto atual.
#[derive(Debug, Clone)]
pub struct AiPanel {
    /// Rótulo da âncora (ex.: "Romanos 3").
    pub reference_label: String,
    /// Pergunta feita.
    pub question: String,
    /// Modelo usado (para rodapé/estimativa de custo).
    pub model: String,
    /// Estimativa de tokens de entrada (contexto + pergunta).
    pub input_tokens: usize,
    /// Estado/conteúdo do painel.
    pub body: AiBody,
    /// Referências bíblicas citadas na resposta (para "viagem rápida").
    pub refs: Vec<Reference>,
    /// Referência selecionada na lista de saltos.
    pub ref_selected: usize,
    /// Rolagem vertical da resposta.
    pub scroll: u16,
}

impl AiPanel {
    fn pending(label: String, question: String, model: String, input_tokens: usize) -> Self {
        AiPanel {
            reference_label: label,
            question,
            model,
            input_tokens,
            body: AiBody::Pending,
            refs: Vec::new(),
            ref_selected: 0,
            scroll: 0,
        }
    }

    fn error(label: String, question: String, msg: String) -> Self {
        AiPanel {
            reference_label: label,
            question,
            model: String::new(),
            input_tokens: 0,
            body: AiBody::Error(msg),
            refs: Vec::new(),
            ref_selected: 0,
            scroll: 0,
        }
    }

    fn with_outcome(
        label: String,
        question: String,
        model: String,
        input_tokens: usize,
        outcome: Result<String, String>,
    ) -> Self {
        let (body, refs) = match outcome {
            Ok(answer) => {
                let refs = cited_refs(&answer);
                (AiBody::Answer(answer), refs)
            }
            Err(e) => (AiBody::Error(e), Vec::new()),
        };
        AiPanel {
            reference_label: label,
            question,
            model,
            input_tokens,
            body,
            refs,
            ref_selected: 0,
            scroll: 0,
        }
    }
}

/// Extrai as referências citadas num texto, únicas e na ordem de aparição.
pub(crate) fn cited_refs(answer: &str) -> Vec<Reference> {
    let mut out: Vec<Reference> = Vec::new();
    for sr in scan_references(answer) {
        if !out.contains(&sr.reference) {
            out.push(sr.reference);
        }
    }
    out
}

/// Modo do modal de configuração de IA.
#[derive(Debug, Clone)]
pub enum SettingsMode {
    /// Lista de provedores (navegação).
    List,
    /// Digitando a chave do provedor selecionado (buffer mascarado na tela).
    EditKey(String),
}

/// Estado do modal de configuração de provedor/chaves (tecla `c`).
#[derive(Debug, Clone)]
pub struct Settings {
    /// Provedor selecionado (índice em [`PROVIDERS`]).
    pub selected: usize,
    /// Para cada provedor de [`PROVIDERS`], se há chave no cofre.
    pub has_key: Vec<bool>,
    /// Modo atual (lista ou edição de chave).
    pub mode: SettingsMode,
}

/// Estimativa grosseira de tokens (≈ 4 caracteres por token).
fn est_tokens(text: &str) -> usize {
    text.chars().count().div_ceil(4)
}

/// Executa a consulta de IA (usada pela thread e pelos testes). Estringe o erro
/// para poder cruzar a fronteira do canal (`AiError` não é `Send`-amigável aqui).
fn run_query(
    provider: &dyn LlmProvider,
    question: &str,
    context: &str,
    lang: Lang,
) -> Result<String, String> {
    ai::ask(provider, question, context, lang).map_err(|e| e.to_string())
}

/// Metadados de uma versão disponível.
#[derive(Debug, Clone)]
struct VersionMeta {
    id: TranslationId,
    label: String,
    lang: Lang,
}

/// Estado da aplicação TUI.
pub struct App {
    store: Store,
    versions: Vec<VersionMeta>,
    version_idx: usize,
    /// Índice do livro selecionado (0..66).
    pub book_idx: usize,
    /// Capítulo atual (1-based).
    pub chapter: u16,
    /// Número de capítulos do livro atual (0 se ausente da versão).
    pub chapter_count: u16,
    /// Versículos do capítulo atual `(número, texto)`.
    pub verses: Vec<(u16, String)>,
    /// Versículo selecionado (índice em `verses`).
    pub selected: usize,
    /// Painel focado.
    pub focus: Focus,
    /// Entrada de texto ativa (prompt), se houver.
    pub input: Option<Input>,
    /// Navegação de referências cruzadas ativa, se houver.
    pub xref_nav: Option<XrefNav>,
    /// Resultados da busca incremental (quando o prompt de busca está ativo).
    pub search_results: Vec<SearchHit>,
    /// Resultado selecionado na lista de busca.
    pub search_selected: usize,
    /// Marcações do usuário (carregadas do disco).
    pub highlights: Vec<Highlight>,
    /// Notas do usuário (carregadas do disco).
    pub notes: Vec<Note>,
    /// Preferências (tema etc.).
    pub config: Config,
    /// Overlay de ajuda (atalhos) visível.
    pub show_help: bool,
    /// Overlay de pergunta à IA, se houver.
    pub ai: Option<AiPanel>,
    /// Canal de recepção do resultado da consulta de IA em andamento.
    ai_rx: Option<Receiver<Result<String, String>>>,
    /// Quadro do indicador de "consultando…" (avança em [`App::tick`]).
    pub spinner: u8,
    /// Modal de configuração de provedor/chaves, se aberto.
    pub settings: Option<Settings>,
    /// Sai do loop quando `true`.
    pub should_quit: bool,
}

impl App {
    /// Cria a app abrindo a versão dada e carregando Gênesis 1.
    pub fn new(store: Store, initial_version: TranslationId) -> Result<Self> {
        let versions: Vec<VersionMeta> = {
            let src = EmbeddedSource::new(&store);
            src.translations()?
                .into_iter()
                .map(|t| VersionMeta {
                    id: t.id,
                    label: t.abbrev,
                    lang: t.language,
                })
                .collect()
        };
        if versions.is_empty() {
            return Err(anyhow!("nenhuma versão disponível"));
        }
        let version_idx = versions
            .iter()
            .position(|v| v.id == initial_version)
            .unwrap_or(0);

        let mut app = App {
            store,
            versions,
            version_idx,
            book_idx: 0,
            chapter: 1,
            chapter_count: 0,
            verses: Vec::new(),
            selected: 0,
            focus: Focus::Books,
            input: None,
            xref_nav: None,
            search_results: Vec::new(),
            search_selected: 0,
            highlights: Vec::new(),
            notes: Vec::new(),
            config: Config::default(),
            show_help: false,
            ai: None,
            ai_rx: None,
            spinner: 0,
            settings: None,
            should_quit: false,
        };
        app.reload()?;
        Ok(app)
    }

    /// Carrega marcações, notas e preferências do disco (tolerante a erros).
    /// Chamado pela `run`; os testes injetam os dados diretamente.
    pub fn load_userdata(&mut self) {
        self.highlights = HighlightStore::load_default()
            .map(|s| s.list().to_vec())
            .unwrap_or_default();
        self.notes = NoteStore::open_default()
            .and_then(|s| s.list())
            .unwrap_or_default();
        self.config = Config::load().unwrap_or_default();
    }

    /// Tema atual (`dark`/`light`/`none`).
    pub fn theme(&self) -> &str {
        &self.config.theme
    }

    /// Cicla o tema (em memória; persistido por [`App::save_config`] ao sair).
    /// Tema desconhecido (ex.: `auto` padrão) é tratado como o início do ciclo,
    /// então a primeira troca sempre dá feedback visível.
    pub fn cycle_theme(&mut self) {
        let cur = THEMES
            .iter()
            .position(|t| *t == self.config.theme)
            .unwrap_or(0);
        self.config.theme = THEMES[(cur + 1) % THEMES.len()].to_string();
    }

    /// Persiste as preferências em `config.toml` (best-effort).
    pub fn save_config(&self) {
        let _ = self.config.save();
    }

    /// Abre o prompt de busca incremental.
    pub fn open_search(&mut self) {
        self.input = Some(Input {
            kind: InputKind::Search,
            buffer: String::new(),
            error: None,
        });
        self.search_results.clear();
        self.search_selected = 0;
    }

    /// Reexecuta a busca a partir do texto digitado (FTS5 na versão ativa).
    fn run_search(&mut self) {
        let query = match self.input.as_ref() {
            Some(i) if i.kind == InputKind::Search => i.buffer.clone(),
            _ => return,
        };
        let opts = SearchOptions {
            translation: self.version().clone(),
            book: None,
            limit: 100,
        };
        self.search_results = search::search(self.store.conn(), &query, &opts).unwrap_or_default();
        self.search_selected = 0;
    }

    /// Versão ativa.
    pub fn version(&self) -> &TranslationId {
        &self.versions[self.version_idx].id
    }

    /// Rótulo da versão ativa (ex.: "KJV").
    pub fn version_label(&self) -> &str {
        &self.versions[self.version_idx].label
    }

    /// Idioma de exibição da versão ativa.
    pub fn lang(&self) -> Lang {
        self.versions[self.version_idx].lang
    }

    /// Número canônico do livro selecionado (1..=66).
    pub fn book_number(&self) -> u8 {
        (self.book_idx + 1) as u8
    }

    /// Nome do livro selecionado no idioma de exibição (`?` se fora de faixa).
    pub fn book_name(&self) -> &'static str {
        match (BOOKS.get(self.book_idx), self.lang()) {
            (Some(b), Lang::Pt) => b.name_pt,
            (Some(b), Lang::En) => b.name_en,
            (None, _) => "?",
        }
    }

    /// Número do versículo selecionado, se houver texto.
    pub fn current_verse(&self) -> Option<u16> {
        self.verses.get(self.selected).map(|(n, _)| *n)
    }

    /// Referência do versículo selecionado.
    pub fn current_reference(&self) -> Option<Reference> {
        self.current_verse()
            .map(|v| Reference::single(self.book_number(), self.chapter, v))
    }

    /// Marcações que cobrem o versículo selecionado.
    pub fn current_highlights(&self) -> Vec<&Highlight> {
        let Some(v) = self.current_verse() else {
            return Vec::new();
        };
        self.highlights
            .iter()
            .filter(|h| {
                h.reference.book == self.book_number()
                    && h.reference.chapter == self.chapter
                    && h.reference.verses.contains(v)
            })
            .collect()
    }

    /// Nota que cobre o versículo selecionado (a primeira encontrada).
    pub fn current_note(&self) -> Option<&Note> {
        let v = self.current_verse()?;
        self.notes.iter().find(|n| {
            n.reference.book == self.book_number()
                && n.reference.chapter == self.chapter
                && n.reference.verses.contains(v)
        })
    }

    /// Referências cruzadas do versículo selecionado (consulta ao banco).
    pub fn current_xrefs(&self) -> Vec<CrossRef> {
        let Some(v) = self.current_verse() else {
            return Vec::new();
        };
        xref::for_verse(
            self.store.conn(),
            self.book_number(),
            self.chapter,
            v,
            xref::DEFAULT_MIN_VOTES,
            xref::DEFAULT_LIMIT,
        )
        .unwrap_or_default()
    }

    /// Busca capítulo/versículos do banco (imutável). Devolve
    /// `(num_capítulos, capítulo_efetivo, versículos)`.
    fn fetch(&self, version: &TranslationId, book: u8, chapter: u16) -> Result<ChapterData> {
        let src = EmbeddedSource::new(&self.store);
        let count = src.chapter_count(book, version)?;
        let chap = chapter.clamp(1, count.max(1));
        let passage = src.passage(&Reference::whole_chapter(book, chap), version)?;
        let verses = passage
            .verses
            .into_iter()
            .map(|v| (v.reference.verses.start().unwrap_or(0), v.text))
            .collect();
        Ok((count, chap, verses))
    }

    /// Recarrega o estado atual (usado na construção; propaga erro).
    fn reload(&mut self) -> Result<()> {
        let (count, chap, verses) =
            self.fetch(&self.version().clone(), self.book_number(), self.chapter)?;
        self.chapter = chap;
        self.chapter_count = count;
        self.verses = verses;
        self.selected = 0;
        Ok(())
    }

    /// Carrega `(book_idx, chapter)` **atomicamente**: só altera o estado se a
    /// leitura tiver sucesso (uma falha não deixa o estado inconsistente).
    fn load_into(&mut self, book_idx: usize, chapter: u16) {
        if book_idx >= BOOKS.len() {
            return;
        }
        let version = self.version().clone();
        if let Ok((count, chap, verses)) = self.fetch(&version, (book_idx + 1) as u8, chapter) {
            self.book_idx = book_idx;
            self.chapter = chap;
            self.chapter_count = count;
            self.verses = verses;
            self.selected = 0;
        }
    }

    /// Seleciona um livro pelo índice (0..66) e carrega seu capítulo 1.
    pub fn select_book(&mut self, idx: usize) {
        self.load_into(idx, 1);
    }

    /// Move a seleção de livro por `delta` (com saturação nas pontas).
    pub fn move_book(&mut self, delta: isize) {
        let new = (self.book_idx as isize + delta).clamp(0, (BOOKS.len() - 1) as isize) as usize;
        if new != self.book_idx {
            self.select_book(new);
        }
    }

    /// Próximo capítulo (ou primeiro do próximo livro).
    pub fn next_chapter(&mut self) {
        if self.chapter < self.chapter_count {
            self.load_into(self.book_idx, self.chapter + 1);
        } else if self.book_idx + 1 < BOOKS.len() {
            self.load_into(self.book_idx + 1, 1);
        }
    }

    /// Capítulo anterior (ou último do livro anterior).
    pub fn prev_chapter(&mut self) {
        if self.chapter > 1 {
            self.load_into(self.book_idx, self.chapter - 1);
        } else if self.book_idx > 0 {
            // u16::MAX é fixado para o último capítulo do livro anterior.
            self.load_into(self.book_idx - 1, u16::MAX);
        }
    }

    /// Alterna para a próxima versão, mantendo livro/capítulo (recarrega texto).
    pub fn cycle_version(&mut self) {
        if self.versions.len() < 2 {
            return;
        }
        let keep = self.selected;
        let new_idx = (self.version_idx + 1) % self.versions.len();
        let version = self.versions[new_idx].id.clone();
        // Só troca se a leitura na nova versão funcionar (atômico).
        if let Ok((count, chap, verses)) = self.fetch(&version, self.book_number(), self.chapter) {
            self.version_idx = new_idx;
            self.chapter = chap;
            self.chapter_count = count;
            self.verses = verses;
            self.selected = keep.min(self.verses.len().saturating_sub(1));
        }
    }

    /// Salta para uma referência (livro/capítulo/versículo), focando o leitor.
    pub fn go_to(&mut self, reference: &Reference) {
        let book_idx = (reference.book.saturating_sub(1) as usize).min(BOOKS.len() - 1);
        self.load_into(book_idx, reference.chapter);
        // Posiciona o cursor no versículo da referência, se houver.
        if let Some(v) = reference.verses.start() {
            if let Some(idx) = self.verses.iter().position(|(n, _)| *n == v) {
                self.selected = idx;
            }
        }
        self.focus = Focus::Reader;
    }

    /// Move o cursor de versículo por `delta` (com saturação).
    pub fn move_cursor(&mut self, delta: isize) {
        if self.verses.is_empty() {
            return;
        }
        let max = (self.verses.len() - 1) as isize;
        self.selected = (self.selected as isize + delta).clamp(0, max) as usize;
    }

    /// Abre a navegação de referências cruzadas do versículo atual.
    pub fn open_xrefs(&mut self) {
        let items = self.current_xrefs();
        if !items.is_empty() {
            self.xref_nav = Some(XrefNav { items, selected: 0 });
        }
    }

    /// Abre o prompt de pergunta livre à IA (ancorada no capítulo atual).
    pub fn open_ask(&mut self) {
        self.input = Some(Input {
            kind: InputKind::Ask,
            buffer: String::new(),
            error: None,
        });
    }

    /// Monta o contexto (RAG local) da pergunta: rótulo do capítulo, versículos
    /// numerados e as referências cruzadas do versículo selecionado. Devolve
    /// `(rótulo_da_âncora, contexto)`.
    fn ai_context(&self) -> (String, String) {
        let lang = self.lang();
        let label = format!("{} {}", self.book_name(), self.chapter);
        let mut context = format!("{label}:\n");
        for (n, text) in &self.verses {
            context.push_str(&format!("{n} {text}\n"));
        }
        let xrefs: Vec<String> = self
            .current_xrefs()
            .iter()
            .map(|c| format_reference(&c.reference, lang))
            .collect();
        let related = if xrefs.is_empty() {
            "(nenhuma)".to_string()
        } else {
            xrefs.join("; ")
        };
        context.push_str(&format!("\nReferências relacionadas: {related}"));
        (label, context)
    }

    /// Submete a pergunta digitada: resolve provedor/chave, monta o contexto e
    /// dispara a consulta. `mock` roda inline; os reais rodam em thread.
    fn submit_ask(&mut self) {
        let question = match self.input.as_ref() {
            Some(i) if i.kind == InputKind::Ask => i.buffer.trim().to_string(),
            _ => return,
        };
        self.input = None;
        if question.is_empty() {
            return;
        }
        let (label, context) = self.ai_context();
        let lang = self.lang();
        let name = self.config.provider.trim().to_ascii_lowercase();
        if name.is_empty() {
            self.ai = Some(AiPanel::error(
                label,
                question,
                "nenhum provedor de IA configurado — pressione c para configurar".to_string(),
            ));
            return;
        }
        // anthropic/openai exigem chave; mock/ollama são locais.
        let key = if name == "anthropic" || name == "openai" {
            match KeyStore::open_default()
                .ok()
                .and_then(|ks| ks.get(&name).map(str::to_string))
            {
                Some(k) => Some(k),
                None => {
                    self.ai = Some(AiPanel::error(
                        label,
                        question,
                        format!("sem chave para `{name}` — pressione c para configurar"),
                    ));
                    return;
                }
            }
        } else {
            None
        };
        let model = ai::providers::default_model(&name).to_string();
        let input_tokens = est_tokens(&context) + est_tokens(&question);
        // O provedor mock roda inline (instantâneo, offline): demo e testes.
        if name == "mock" {
            let outcome = match build_provider(&name, None, None) {
                Ok(p) => run_query(p.as_ref(), &question, &context, lang),
                Err(e) => Err(e.to_string()),
            };
            self.ai = Some(AiPanel::with_outcome(
                label,
                question,
                model,
                input_tokens,
                outcome,
            ));
            return;
        }
        // Provedores reais bloqueiam (reqwest::blocking): rodam em thread para
        // não congelar a UI; só `String`/`Lang` cruzam o canal (o `Store` não).
        let (tx, rx) = std::sync::mpsc::channel();
        let (provider_name, q, ctx) = (name.clone(), question.clone(), context);
        std::thread::spawn(move || {
            let outcome = match build_provider(&provider_name, key, None) {
                Ok(p) => run_query(p.as_ref(), &q, &ctx, lang),
                Err(e) => Err(e.to_string()),
            };
            let _ = tx.send(outcome);
        });
        self.ai = Some(AiPanel::pending(label, question, model, input_tokens));
        self.ai_rx = Some(rx);
        self.spinner = 0;
    }

    /// Drena o canal da consulta de IA em andamento (chamado a cada iteração do
    /// loop). Atualiza o painel quando a resposta/erro chega.
    pub fn poll_ai(&mut self) {
        let Some(rx) = self.ai_rx.as_ref() else {
            return;
        };
        match rx.try_recv() {
            Ok(outcome) => {
                self.ai_rx = None;
                if let Some(panel) = self.ai.as_mut() {
                    if matches!(panel.body, AiBody::Pending) {
                        match outcome {
                            Ok(answer) => {
                                panel.refs = cited_refs(&answer);
                                panel.ref_selected = 0;
                                panel.body = AiBody::Answer(answer);
                            }
                            Err(e) => panel.body = AiBody::Error(e),
                        }
                    }
                }
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.ai_rx = None;
                if let Some(panel) = self.ai.as_mut() {
                    if matches!(panel.body, AiBody::Pending) {
                        panel.body = AiBody::Error("consulta interrompida".to_string());
                    }
                }
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
        }
    }

    /// Avança o indicador de "consultando…" (chamado nos ticks ociosos do loop).
    pub fn tick(&mut self) {
        if matches!(self.ai.as_ref().map(|p| &p.body), Some(AiBody::Pending)) {
            self.spinner = self.spinner.wrapping_add(1);
        }
    }

    /// Abre o modal de configuração de provedor/chaves (tecla `c`).
    pub fn open_settings(&mut self) {
        let have: Vec<String> = KeyStore::open_default()
            .map(|ks| ks.list_providers())
            .unwrap_or_default();
        let has_key = PROVIDERS
            .iter()
            .map(|p| have.iter().any(|h| h.as_str() == *p))
            .collect();
        let active = self.config.provider.trim().to_ascii_lowercase();
        let selected = PROVIDERS.iter().position(|p| *p == active).unwrap_or(0);
        self.settings = Some(Settings {
            selected,
            has_key,
            mode: SettingsMode::List,
        });
    }

    fn handle_ai_key(&mut self, key: KeyEvent) {
        // Salto diferido: a referência-alvo é resolvida sob o empréstimo do
        // painel e aplicada depois (fecha o overlay + navega).
        let mut jump: Option<Reference> = None;
        if let Some(panel) = self.ai.as_mut() {
            let n = panel.refs.len();
            match key.code {
                KeyCode::Esc | KeyCode::Char('q') => {
                    self.ai = None;
                    return;
                }
                KeyCode::Down | KeyCode::Char('j') => panel.scroll = panel.scroll.saturating_add(1),
                KeyCode::Up | KeyCode::Char('k') => panel.scroll = panel.scroll.saturating_sub(1),
                KeyCode::PageDown | KeyCode::Char(' ') => {
                    panel.scroll = panel.scroll.saturating_add(10)
                }
                KeyCode::PageUp => panel.scroll = panel.scroll.saturating_sub(10),
                // Seleciona entre as referências citadas.
                KeyCode::Tab if n > 0 => panel.ref_selected = (panel.ref_selected + 1) % n,
                KeyCode::BackTab if n > 0 => panel.ref_selected = (panel.ref_selected + n - 1) % n,
                // Enter salta para a referência selecionada.
                KeyCode::Enter => jump = panel.refs.get(panel.ref_selected).copied(),
                // Dígitos 1–9: salto direto para a n-ésima referência citada.
                KeyCode::Char(c @ '1'..='9') => {
                    jump = panel.refs.get((c as u8 - b'1') as usize).copied();
                }
                _ => {}
            }
        }
        if let Some(reference) = jump {
            self.ai = None;
            self.go_to(&reference);
        }
    }

    fn handle_settings_key(&mut self, key: KeyEvent) {
        // Ação diferida: evita conflito de empréstimo entre `self.settings` (que
        // emprestamos para ler/editar) e `self.config`/`self.settings = None`.
        enum Act {
            None,
            Close,
            SetActive,
            RemoveKey,
            CommitKey(String),
        }
        let mut act = Act::None;
        if let Some(settings) = self.settings.as_mut() {
            // Trata os dois modos separadamente para nunca reatribuir
            // `settings.mode` enquanto o buffer da edição está emprestado.
            if matches!(settings.mode, SettingsMode::EditKey(_)) {
                if let SettingsMode::EditKey(buffer) = &mut settings.mode {
                    match key.code {
                        KeyCode::Backspace => {
                            buffer.pop();
                        }
                        KeyCode::Char(c) => buffer.push(c),
                        KeyCode::Enter => act = Act::CommitKey(std::mem::take(buffer)),
                        _ => {}
                    }
                }
                if key.code == KeyCode::Esc {
                    settings.mode = SettingsMode::List;
                }
            } else {
                match key.code {
                    KeyCode::Esc | KeyCode::Char('c') | KeyCode::Char('q') => act = Act::Close,
                    KeyCode::Down | KeyCode::Char('j') => {
                        if settings.selected + 1 < PROVIDERS.len() {
                            settings.selected += 1;
                        }
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        settings.selected = settings.selected.saturating_sub(1);
                    }
                    KeyCode::Enter => act = Act::SetActive,
                    KeyCode::Char('e') => settings.mode = SettingsMode::EditKey(String::new()),
                    KeyCode::Char('d') => act = Act::RemoveKey,
                    _ => {}
                }
            }
        }
        match act {
            Act::Close => self.settings = None,
            Act::SetActive => {
                if let Some(i) = self.settings.as_ref().map(|s| s.selected) {
                    self.config.provider = PROVIDERS[i].to_string();
                    let _ = self.config.save();
                }
            }
            Act::RemoveKey => {
                if let Some(i) = self.settings.as_ref().map(|s| s.selected) {
                    let name = PROVIDERS[i].to_string();
                    if let Ok(mut ks) = KeyStore::open_default() {
                        let _ = ks.remove(&name);
                    }
                    if let Some(s) = self.settings.as_mut() {
                        s.has_key[i] = false;
                    }
                }
            }
            Act::CommitKey(buf) => {
                if let Some(i) = self.settings.as_ref().map(|s| s.selected) {
                    let name = PROVIDERS[i].to_string();
                    let trimmed = buf.trim();
                    let ok = !trimmed.is_empty()
                        && KeyStore::open_default()
                            .and_then(|mut ks| ks.set(&name, trimmed))
                            .is_ok();
                    if let Some(s) = self.settings.as_mut() {
                        if ok {
                            s.has_key[i] = true;
                        }
                        s.mode = SettingsMode::List;
                    }
                }
            }
            Act::None => {}
        }
    }

    /// Processa uma tecla pressionada.
    pub fn handle_key(&mut self, key: KeyEvent) {
        // Os modais capturam o teclado, em ordem de prioridade.
        if self.settings.is_some() {
            self.handle_settings_key(key);
            return;
        }
        if self.ai.is_some() {
            self.handle_ai_key(key);
            return;
        }
        // O overlay de ajuda captura o teclado: fecha com ?/Esc/q, ignora o resto.
        if self.show_help {
            if matches!(
                key.code,
                KeyCode::Char('?') | KeyCode::Esc | KeyCode::Char('q')
            ) {
                self.show_help = false;
            }
            return;
        }
        if self.input.is_some() {
            self.handle_input_key(key);
            return;
        }
        if self.xref_nav.is_some() {
            self.handle_xref_key(key);
            return;
        }
        match key.code {
            KeyCode::Char('?') => self.show_help = true,
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            KeyCode::Tab => {
                self.focus = match self.focus {
                    Focus::Books => Focus::Reader,
                    Focus::Reader => Focus::Books,
                };
            }
            KeyCode::Char('v') => self.cycle_version(),
            KeyCode::Char('x') => self.open_xrefs(),
            KeyCode::Char('t') => self.cycle_theme(),
            KeyCode::Char('a') => self.open_ask(),
            KeyCode::Char('c') => self.open_settings(),
            KeyCode::Char('/') => self.open_search(),
            KeyCode::Char('g') => {
                self.input = Some(Input {
                    kind: InputKind::GoTo,
                    buffer: String::new(),
                    error: None,
                });
            }
            _ => match self.focus {
                Focus::Books => self.handle_books_key(key),
                Focus::Reader => self.handle_reader_key(key),
            },
        }
    }

    fn handle_books_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Down | KeyCode::Char('j') => self.move_book(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_book(-1),
            KeyCode::PageDown => self.move_book(10),
            KeyCode::PageUp => self.move_book(-10),
            KeyCode::Home => self.select_book(0),
            KeyCode::End => self.select_book(BOOKS.len() - 1),
            KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => self.focus = Focus::Reader,
            _ => {}
        }
    }

    fn handle_reader_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Down | KeyCode::Char('j') => self.move_cursor(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_cursor(-1),
            KeyCode::PageDown | KeyCode::Char(' ') => self.move_cursor(10),
            KeyCode::PageUp => self.move_cursor(-10),
            KeyCode::Char('n') | KeyCode::Right | KeyCode::Char('l') => self.next_chapter(),
            KeyCode::Char('p') | KeyCode::Char('h') => self.prev_chapter(),
            KeyCode::Left => self.focus = Focus::Books,
            KeyCode::Home => self.selected = 0,
            KeyCode::End => self.selected = self.verses.len().saturating_sub(1),
            _ => {}
        }
    }

    fn handle_input_key(&mut self, key: KeyEvent) {
        let Some(input) = self.input.as_mut() else {
            return;
        };
        let is_search = input.kind == InputKind::Search;
        match key.code {
            KeyCode::Esc => {
                self.input = None;
                self.search_results.clear();
            }
            KeyCode::Backspace => {
                input.buffer.pop();
                input.error = None;
                if is_search {
                    self.run_search();
                }
            }
            KeyCode::Char(c) => {
                input.buffer.push(c);
                input.error = None;
                if is_search {
                    self.run_search();
                }
            }
            // Na busca, ↑↓ movem a seleção dos resultados.
            KeyCode::Down if is_search => {
                if self.search_selected + 1 < self.search_results.len() {
                    self.search_selected += 1;
                }
            }
            KeyCode::Up if is_search => {
                self.search_selected = self.search_selected.saturating_sub(1);
            }
            KeyCode::Enter => self.submit_input(),
            _ => {}
        }
    }

    fn submit_input(&mut self) {
        let Some(input) = self.input.as_ref() else {
            return;
        };
        match input.kind {
            InputKind::GoTo => match parse_reference(&input.buffer) {
                Ok(reference) => {
                    self.input = None;
                    self.go_to(&reference);
                }
                Err(e) => {
                    if let Some(input) = self.input.as_mut() {
                        input.error = Some(format!("{e}"));
                    }
                }
            },
            InputKind::Search => {
                if let Some(hit) = self.search_results.get(self.search_selected) {
                    let reference = hit.reference;
                    self.input = None;
                    self.search_results.clear();
                    self.go_to(&reference);
                }
            }
            InputKind::Ask => self.submit_ask(),
        }
    }

    fn handle_xref_key(&mut self, key: KeyEvent) {
        let Some(nav) = self.xref_nav.as_mut() else {
            return;
        };
        match key.code {
            KeyCode::Esc | KeyCode::Char('x') | KeyCode::Char('q') => self.xref_nav = None,
            KeyCode::Down | KeyCode::Char('j') => {
                if nav.selected + 1 < nav.items.len() {
                    nav.selected += 1;
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                nav.selected = nav.selected.saturating_sub(1);
            }
            KeyCode::Enter => {
                let reference = nav.items[nav.selected].reference;
                self.xref_nav = None;
                self.go_to(&reference);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::crossterm::event::KeyModifiers;
    use rusqlite::params;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::empty())
    }

    fn type_str(app: &mut App, s: &str) {
        for c in s.chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
    }

    fn seeded_app() -> App {
        let store = Store::open_in_memory().unwrap();
        {
            let conn = store.conn();
            for (id, abbrev, lang) in [("kjv", "KJV", "en"), ("alm", "ALM", "pt")] {
                conn.execute(
                    "INSERT INTO translations(id,abbrev,name,language,license,embeddable) \
                     VALUES (?1,?2,?2,?3,'public-domain',1)",
                    params![id, abbrev, lang],
                )
                .unwrap();
            }
            let rows = [
                ("kjv", 45, 3, 23, "For all have sinned"),
                ("kjv", 45, 3, 24, "Being justified freely by his grace"),
                ("kjv", 45, 6, 23, "For the wages of sin is death"),
                ("alm", 45, 3, 23, "Porque todos pecaram"),
            ];
            for (t, b, c, v, txt) in rows {
                conn.execute(
                    "INSERT INTO verses(translation_id,book_number,chapter,verse,text) \
                     VALUES (?1,?2,?3,?4,?5)",
                    params![t, b, c, v, txt],
                )
                .unwrap();
            }
            // xref: Rm 3:23 → Rm 6:23 (votos 50).
            conn.execute(
                "INSERT INTO cross_references \
                 (from_book,from_chapter,from_verse,to_book,to_chapter,to_verse_start,to_verse_end,votes) \
                 VALUES (45,3,23,45,6,23,23,50)",
                [],
            )
            .unwrap();
        }
        let mut app = App::new(store, "kjv".into()).unwrap();
        app.go_to(&parse_reference("Rm 3.23").unwrap());
        app
    }

    #[test]
    fn cursor_moves_and_panel_data_follows() {
        let mut app = seeded_app();
        assert_eq!(app.current_verse(), Some(23));
        // Marcação injetada cobrindo Rm 3:24.
        app.highlights = vec![Highlight {
            reference: parse_reference("Rm 3.24").unwrap(),
            color: "yellow".into(),
            tag: Some("graça".into()),
        }];
        // Sem marcação no 23.
        assert!(app.current_highlights().is_empty());
        app.handle_key(key(KeyCode::Down)); // foco está no Reader (go_to focou)
        assert_eq!(app.current_verse(), Some(24));
        assert_eq!(app.current_highlights().len(), 1);
    }

    #[test]
    fn current_note_follows_cursor() {
        let mut app = seeded_app();
        app.notes = vec![Note {
            reference: parse_reference("Rm 3.23").unwrap(),
            body: "Nota de teste".into(),
        }];
        assert_eq!(
            app.current_note().map(|n| n.body.as_str()),
            Some("Nota de teste")
        );
        app.move_cursor(1); // 24 — sem nota
        assert!(app.current_note().is_none());
    }

    #[test]
    fn xref_nav_open_select_and_jump() {
        let mut app = seeded_app();
        assert_eq!(app.current_verse(), Some(23));
        app.handle_key(key(KeyCode::Char('x')));
        let nav = app.xref_nav.as_ref().expect("nav aberta");
        assert_eq!(nav.items.len(), 1);
        assert_eq!(nav.items[0].reference, Reference::single(45, 6, 23));
        // Enter salta para a referência cruzada.
        app.handle_key(key(KeyCode::Enter));
        assert!(app.xref_nav.is_none());
        assert_eq!(app.book_number(), 45);
        assert_eq!(app.chapter, 6);
        assert_eq!(app.current_verse(), Some(23));
    }

    #[test]
    fn xref_nav_esc_closes_without_jump() {
        let mut app = seeded_app();
        app.handle_key(key(KeyCode::Char('x')));
        assert!(app.xref_nav.is_some());
        app.handle_key(key(KeyCode::Esc));
        assert!(app.xref_nav.is_none());
        assert_eq!(app.chapter, 3); // não saltou
    }

    #[test]
    fn open_xrefs_noop_when_none() {
        let mut app = seeded_app();
        app.move_cursor(1); // Rm 3:24 não tem xref semeada
        app.handle_key(key(KeyCode::Char('x')));
        assert!(app.xref_nav.is_none());
    }

    #[test]
    fn goto_positions_cursor_on_verse() {
        let mut app = seeded_app();
        app.handle_key(key(KeyCode::Char('g')));
        type_str(&mut app, "Rm 3.24");
        app.handle_key(key(KeyCode::Enter));
        assert_eq!(app.current_verse(), Some(24));
    }

    #[test]
    fn cycle_version_keeps_cursor_position() {
        let mut app = seeded_app();
        assert_eq!(app.current_verse(), Some(23));
        app.handle_key(key(KeyCode::Char('v'))); // → alm (só tem 3:23)
        assert_eq!(app.version_label(), "ALM");
        assert_eq!(app.current_verse(), Some(23));
    }

    #[test]
    fn interactive_search_filters_and_jumps() {
        let mut app = seeded_app();
        // Precisa do índice FTS para a versão kjv: insere para os versículos.
        {
            let conn = app.store.conn();
            for (id, txt) in [
                (1i64, "For all have sinned"),
                (2, "Being justified freely by his grace"),
            ] {
                conn.execute(
                    "INSERT INTO verses_fts(text, translation_id, verse_id) VALUES (?1,'kjv',?2)",
                    params![txt, id],
                )
                .unwrap();
            }
        }
        app.handle_key(key(KeyCode::Char('/')));
        assert!(matches!(
            app.input.as_ref().map(|i| i.kind),
            Some(InputKind::Search)
        ));
        type_str(&mut app, "sinned");
        assert!(!app.search_results.is_empty(), "deveria filtrar resultados");
        app.handle_key(key(KeyCode::Enter));
        assert!(app.input.is_none());
        // Saltou para o versículo com "sinned" (Rm 3:23, verse_id 1).
        assert_eq!(app.current_verse(), Some(23));
    }

    #[test]
    fn search_no_match_then_esc() {
        let mut app = seeded_app();
        app.handle_key(key(KeyCode::Char('/')));
        type_str(&mut app, "zzqqxx");
        assert!(app.search_results.is_empty());
        app.handle_key(key(KeyCode::Enter)); // sem resultado: não salta, prompt continua
        assert!(app.input.is_some());
        app.handle_key(key(KeyCode::Esc));
        assert!(app.input.is_none());
    }

    #[test]
    fn theme_cycles_in_memory() {
        let mut app = seeded_app();
        app.config.theme = "dark".to_string();
        app.handle_key(key(KeyCode::Char('t')));
        assert_eq!(app.theme(), "light");
        app.handle_key(key(KeyCode::Char('t')));
        assert_eq!(app.theme(), "none");
        app.handle_key(key(KeyCode::Char('t')));
        assert_eq!(app.theme(), "dark");
    }

    #[test]
    fn theme_cycle_from_auto_gives_visible_change() {
        let mut app = seeded_app();
        // Padrão "auto" renderiza como escuro; a 1ª troca vai para "light".
        assert_eq!(app.config.theme, "auto");
        app.handle_key(key(KeyCode::Char('t')));
        assert_eq!(app.theme(), "light");
    }

    #[test]
    fn core_keybindings_still_work() {
        let mut app = seeded_app();
        // go_to focou o Reader em Rm 3.23 (cap 3).
        assert_eq!(app.focus, Focus::Reader);
        // n / p trocam capítulo (incrementa/decrementa em 1).
        app.handle_key(key(KeyCode::Char('n')));
        assert_eq!(app.chapter, 4);
        app.handle_key(key(KeyCode::Char('p')));
        assert_eq!(app.chapter, 3);
        // ↓/↑ movem o cursor de versículo.
        let v0 = app.selected;
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.selected, v0 + 1);
        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.selected, v0);
        // ← volta o foco para Livros; Tab alterna.
        app.handle_key(key(KeyCode::Left));
        assert_eq!(app.focus, Focus::Books);
        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.focus, Focus::Reader);
        // / abre busca, Esc fecha.
        app.handle_key(key(KeyCode::Char('/')));
        assert!(app.input.is_some());
        app.handle_key(key(KeyCode::Esc));
        assert!(app.input.is_none());
        // ? abre a ajuda; q encerra.
        app.handle_key(key(KeyCode::Char('?')));
        assert!(app.show_help);
        app.handle_key(key(KeyCode::Char('?')));
        assert!(!app.show_help);
        app.handle_key(key(KeyCode::Char('q')));
        assert!(app.should_quit);
    }

    #[test]
    fn help_overlay_swallows_navigation_keys() {
        let mut app = seeded_app();
        let (ch, sel) = (app.chapter, app.selected);
        app.handle_key(key(KeyCode::Char('?')));
        // Teclas de navegação não têm efeito enquanto a ajuda está aberta.
        for k in [
            KeyCode::Char('n'),
            KeyCode::Down,
            KeyCode::Char('j'),
            KeyCode::Enter,
        ] {
            app.handle_key(key(k));
        }
        assert!(app.show_help);
        assert_eq!(app.chapter, ch);
        assert_eq!(app.selected, sel);
        assert!(!app.should_quit);
        // Esc fecha a ajuda (sem encerrar a app).
        app.handle_key(key(KeyCode::Esc));
        assert!(!app.show_help);
        assert!(!app.should_quit);
    }

    #[test]
    fn search_results_navigate_with_arrows() {
        let mut app = seeded_app();
        // Índice FTS para os dois versículos que começam com "For" (ids 1 e 3).
        {
            let conn = app.store.conn();
            for (id, txt) in [
                (1i64, "For all have sinned"),
                (3, "For the wages of sin is death"),
            ] {
                conn.execute(
                    "INSERT INTO verses_fts(text, translation_id, verse_id) VALUES (?1,'kjv',?2)",
                    params![txt, id],
                )
                .unwrap();
            }
        }
        app.handle_key(key(KeyCode::Char('/')));
        type_str(&mut app, "for"); // casa 2 versículos (Rm 3:23 e 6:23)
        assert!(app.search_results.len() >= 2, "esperava ≥2 resultados");
        assert_eq!(app.search_selected, 0);
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.search_selected, 1);
        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.search_selected, 0);
    }

    #[test]
    fn xref_nav_closes_with_q() {
        let mut app = seeded_app();
        app.handle_key(key(KeyCode::Char('x')));
        assert!(app.xref_nav.is_some());
        app.handle_key(key(KeyCode::Char('q')));
        assert!(app.xref_nav.is_none());
        assert!(!app.should_quit, "q deve só fechar a lista, não encerrar");
    }

    // --- IA na TUI ---------------------------------------------------------

    use std::sync::Mutex;
    use the_light_core::ai::MockLlmProvider;

    // Serializa os testes que mexem em variáveis de ambiente globais
    // (`LIGHT_SECRETS`/`LIGHT_CONFIG`) para não interferirem entre si.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn ai_context_has_chapter_text_and_cross_refs() {
        let app = seeded_app();
        let (label, context) = app.ai_context();
        assert_eq!(label, "Romans 3");
        assert!(context.contains("23 For all have sinned"), "{context}");
        assert!(context.contains("24 Being justified"), "{context}");
        // A referência cruzada (Rm 3:23 → Rm 6:23) entra no contexto.
        assert!(context.contains("Romans 6:23"), "{context}");
    }

    #[test]
    fn run_query_uses_provider() {
        let p = MockLlmProvider::new("resposta fixa");
        let out = run_query(&p, "pergunta", "contexto", Lang::En);
        assert_eq!(out.unwrap(), "resposta fixa");
    }

    #[test]
    fn ask_with_mock_answers_inline() {
        let mut app = seeded_app();
        app.config.provider = "mock".into();
        app.handle_key(key(KeyCode::Char('a')));
        assert!(
            matches!(app.input.as_ref().map(|i| i.kind), Some(InputKind::Ask)),
            "a abre o prompt de pergunta"
        );
        type_str(&mut app, "o que e pecado?");
        app.handle_key(key(KeyCode::Enter));
        let panel = app.ai.as_ref().expect("painel de IA");
        assert!(
            matches!(panel.body, AiBody::Answer(_)),
            "mock responde inline"
        );
        assert!(app.ai_rx.is_none(), "mock não usa thread/canal");
        assert!(app.input.is_none(), "o prompt fecha ao enviar");
    }

    #[test]
    fn ask_without_provider_shows_friendly_error() {
        let mut app = seeded_app(); // provider vazio (padrão)
        app.handle_key(key(KeyCode::Char('a')));
        type_str(&mut app, "pergunta");
        app.handle_key(key(KeyCode::Enter));
        match &app.ai.as_ref().unwrap().body {
            AiBody::Error(msg) => assert!(msg.contains("provedor"), "{msg}"),
            other => panic!("esperava erro amigável, veio {other:?}"),
        }
        assert!(app.ai_rx.is_none(), "erro local não dispara thread");
    }

    #[test]
    fn ask_with_empty_question_does_nothing() {
        let mut app = seeded_app();
        app.config.provider = "mock".into();
        app.handle_key(key(KeyCode::Char('a')));
        app.handle_key(key(KeyCode::Enter)); // sem digitar
        assert!(app.ai.is_none(), "pergunta vazia é ignorada");
    }

    #[test]
    fn ai_overlay_captures_keys_and_esc_closes() {
        let mut app = seeded_app();
        let before = app.selected;
        app.ai = Some(AiPanel {
            reference_label: "Romans 3".into(),
            question: "q".into(),
            model: "mock-1".into(),
            input_tokens: 0,
            body: AiBody::Answer("linha".into()),
            refs: Vec::new(),
            ref_selected: 0,
            scroll: 0,
        });
        // ↓ rola o overlay; NÃO move o cursor de versículo.
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.selected, before, "o overlay captura a navegação");
        assert_eq!(app.ai.as_ref().unwrap().scroll, 1);
        app.handle_key(key(KeyCode::Esc));
        assert!(app.ai.is_none(), "Esc fecha o overlay");
    }

    #[test]
    fn cited_refs_extracts_unique_in_order() {
        let refs = cited_refs("Veja João 3:16 e Romanos 5:8, e de novo João 3:16.");
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0].book, 43); // João
        assert_eq!(refs[1].book, 45); // Romanos
    }

    #[test]
    fn ai_overlay_fast_travels_to_cited_ref() {
        let mut app = seeded_app(); // começa em Romanos 3
        let answer = "Compare com Romans 6:23.".to_string();
        app.ai = Some(AiPanel {
            reference_label: "Romans 3".into(),
            question: "q".into(),
            model: "mock-1".into(),
            input_tokens: 0,
            refs: cited_refs(&answer),
            ref_selected: 0,
            body: AiBody::Answer(answer),
            scroll: 0,
        });
        assert_eq!(app.ai.as_ref().unwrap().refs.len(), 1);
        app.handle_key(key(KeyCode::Char('1'))); // salta para a 1ª citada
        assert!(app.ai.is_none(), "saltar fecha o overlay");
        assert_eq!(app.book_number(), 45);
        assert_eq!(app.chapter, 6);
        assert_eq!(app.current_verse(), Some(23));
    }

    #[test]
    fn ai_overlay_enter_jumps_to_selected_ref() {
        let mut app = seeded_app();
        let answer = "Romans 3:24 e Romans 6:23.".to_string();
        app.ai = Some(AiPanel {
            reference_label: "Romans 3".into(),
            question: "q".into(),
            model: "mock-1".into(),
            input_tokens: 0,
            refs: cited_refs(&answer),
            ref_selected: 0,
            body: AiBody::Answer(answer),
            scroll: 0,
        });
        assert_eq!(app.ai.as_ref().unwrap().refs.len(), 2);
        app.handle_key(key(KeyCode::Tab)); // seleciona a 2ª (Rm 6:23)
        assert_eq!(app.ai.as_ref().unwrap().ref_selected, 1);
        app.handle_key(key(KeyCode::Enter));
        assert!(app.ai.is_none());
        assert_eq!(app.chapter, 6);
        assert_eq!(app.current_verse(), Some(23));
    }

    #[test]
    fn settings_open_navigate_and_activate_provider() {
        let _guard = ENV_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("LIGHT_CONFIG", dir.path().join("config.toml"));
        std::env::set_var("LIGHT_SECRETS", dir.path().join("secrets.toml"));

        let mut app = seeded_app();
        app.handle_key(key(KeyCode::Char('c')));
        assert!(app.settings.is_some(), "c abre o modal de configuração");
        app.handle_key(key(KeyCode::Down)); // anthropic(0) → openai(1)
        app.handle_key(key(KeyCode::Enter)); // ativa
        assert_eq!(app.config.provider, "openai");
        assert!(app.settings.is_some(), "Enter ativa sem fechar o modal");
        app.handle_key(key(KeyCode::Esc));
        assert!(app.settings.is_none(), "Esc fecha o modal");

        std::env::remove_var("LIGHT_CONFIG");
        std::env::remove_var("LIGHT_SECRETS");
    }

    #[test]
    fn settings_edit_key_saves_to_vault() {
        let _guard = ENV_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let secrets = dir.path().join("secrets.toml");
        std::env::set_var("LIGHT_SECRETS", &secrets);
        std::env::set_var("LIGHT_CONFIG", dir.path().join("config.toml"));

        let mut app = seeded_app();
        app.handle_key(key(KeyCode::Char('c'))); // abre (selected=0 anthropic)
        app.handle_key(key(KeyCode::Char('e'))); // entra em EditKey
        type_str(&mut app, "sk-secret-xyz");
        app.handle_key(key(KeyCode::Enter)); // grava

        let s = app.settings.as_ref().unwrap();
        assert!(matches!(s.mode, SettingsMode::List), "volta para a lista");
        assert!(s.has_key[0], "anthropic passa a ter chave");

        // A chave foi gravada no cofre temporário (e só lá).
        let ks = KeyStore::open(&secrets).unwrap();
        assert_eq!(ks.get("anthropic"), Some("sk-secret-xyz"));

        std::env::remove_var("LIGHT_SECRETS");
        std::env::remove_var("LIGHT_CONFIG");
    }

    #[test]
    fn settings_overlay_captures_navigation() {
        let _guard = ENV_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("LIGHT_SECRETS", dir.path().join("secrets.toml"));

        let mut app = seeded_app();
        let before = app.selected;
        app.handle_key(key(KeyCode::Char('c')));
        app.handle_key(key(KeyCode::Down)); // move o modal, não o cursor
        assert_eq!(app.selected, before);
        assert_eq!(app.settings.as_ref().unwrap().selected, 1);
        app.handle_key(key(KeyCode::Esc));
        assert!(app.settings.is_none());

        std::env::remove_var("LIGHT_SECRETS");
    }
}
