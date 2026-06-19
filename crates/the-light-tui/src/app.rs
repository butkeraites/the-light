//! Estado da TUI e tratamento de teclas (lógica pura, testável).

use std::path::PathBuf;
use std::sync::mpsc::Receiver;

use anyhow::{anyhow, Result};
use ratatui::crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use the_light_core::ai::{
    self, build_provider, ChatMessage, ChatRole, Denomination, KeyStore, LlmProvider, StudyDepth,
    StudyMode, StudyRequest, VerifiedLexicon, PROVIDERS,
};
use the_light_core::config::Config;
use the_light_core::model::{Lang, Reference, SearchHit, TranslationId};
use the_light_core::reference::{format_reference, parse_reference, scan_references, BOOKS};
use the_light_core::scholarly;
use the_light_core::search::{self, SearchOptions};
use the_light_core::source::{BibleSource, EmbeddedSource};
use the_light_core::store::Store;
use the_light_core::userdata::{Highlight, HighlightStore, Note, NoteStore, Session, SessionStore};
use the_light_core::xref::{self, CrossRef};

use crate::scroll::ScrollState;

/// Temas disponíveis, ciclados pela tecla `t` e persistidos em `config.toml`.
pub const THEMES: &[&str] = &["dark", "light", "none"];

/// Duração (em ticks ociosos de ~80ms) da mensagem efêmera de status (toast).
const TOAST_TICKS: u8 = 30;

/// Seleção de texto via mouse, **restrita à área de leitura**. As coordenadas são
/// absolutas no terminal `(coluna, linha)`, sempre grampeadas ao retângulo dos
/// versículos — começar o arrasto fora dele simplesmente não inicia seleção.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    /// Onde a seleção nasceu (botão pressionado).
    pub anchor: (u16, u16),
    /// Ponta atual da seleção (segue o arrasto).
    pub cursor: (u16, u16),
    /// `true` enquanto o botão segue pressionado.
    pub dragging: bool,
}

/// `true` se `(col, row)` está dentro de `r`.
fn rect_contains(r: Rect, col: u16, row: u16) -> bool {
    col >= r.x && col < r.x + r.width && row >= r.y && row < r.y + r.height
}

/// Grampeia `(col, row)` aos limites de `r` (canto inferior-direito inclusivo).
fn clamp_to_rect(r: Rect, col: u16, row: u16) -> (u16, u16) {
    let max_x = r.x + r.width.saturating_sub(1);
    let max_y = r.y + r.height.saturating_sub(1);
    (col.clamp(r.x, max_x), row.clamp(r.y, max_y))
}

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
    /// Prompt inicial de um estudo (assunto a estudar).
    StudyBrief,
    /// Resposta própria (custom) numa rodada de refinamento.
    StudyCustom,
    /// Foco do aprofundamento de um estudo concluído (opcional).
    StudyDeepen,
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

/// Estado da última/atual consulta dentro de uma conversa.
#[derive(Debug, Clone)]
pub enum AiStatus {
    /// Ociosa: mostrando a conversa, pronta para continuar.
    Idle,
    /// Consultando o provedor (thread de background).
    Pending,
    /// Erro na última tentativa (sem provedor/chave, rede, etc.).
    Error(String),
}

/// Um alvo clicável registrado pela renderização (ver [`App::click_targets`]).
/// O `draw` empilha um `(Rect, ClickTarget)` por linha/botão; o clique é
/// resolvido pelo alvo mais ao topo que contém o ponto.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClickTarget {
    /// Linha de livro na lista (índice em [`BOOKS`]).
    Book(usize),
    /// Linha de versículo no leitor (índice em `verses`).
    Verse(usize),
    /// Linha do seletor de modo (índice em [`StudyMode::all`]).
    ModeRow(usize),
    /// Campo da lente na tela de preparo — clicar cicla para a próxima.
    LensCycle,
    /// Linha do navegador de conversas/estudos.
    SessionRow(usize),
    /// Linha de provedor nas configurações de IA.
    SettingsRow(usize),
    /// Opção de refinamento de escopo no assistente de estudo.
    StudyOption(usize),
    /// Botão de instalar os dados acadêmicos.
    ScholarlyInstall,
}

impl ClickTarget {
    /// `true` para alvos que pertencem a um overlay modal (não à tela de base de
    /// livros/leitor). Usado para ignorar cliques "por baixo" de um modal.
    fn is_overlay(self) -> bool {
        !matches!(self, ClickTarget::Book(_) | ClickTarget::Verse(_))
    }
}

/// Estado do **assistente de estudo** (prompt + refinamento em 3 rodadas) que
/// vive dentro de um [`AiPanel`]. Ausente nas conversas de IA comuns.
#[derive(Debug, Clone)]
pub struct StudyFlow {
    /// Modo escolhido no seletor.
    pub mode: StudyMode,
    /// Lente denominacional (do config).
    pub lens: Denomination,
    /// Assunto digitado pelo usuário.
    pub brief: String,
    /// Rodada atual (1..=3); 0 antes de enviar o assunto.
    pub round: u8,
    /// Pergunta da rodada atual (vazia enquanto aguarda a IA).
    pub question: String,
    /// Opções da rodada atual (vazias enquanto não há escolha a fazer).
    pub options: Vec<String>,
    /// Opção selecionada.
    pub selected: usize,
    /// Respostas anteriores `(pergunta, resposta)`.
    pub prior: Vec<(String, String)>,
}

/// Contexto de um estudo **concluído**, retido para permitir aprofundar (`+`) e
/// para responder follow-ups (`a`) na mesma lente/fundamentação.
#[derive(Debug, Clone)]
pub struct StudyContext {
    /// Modo do estudo.
    pub mode: StudyMode,
    /// Lente denominacional aplicada.
    pub lens: Denomination,
    /// Profundidade da última passagem (avança ao aprofundar).
    pub depth: StudyDepth,
    /// Referência resolvida (capítulo âncora se temático).
    pub reference: Reference,
    /// Rótulo da referência (ou título curto do assunto, se temático).
    pub reference_label: String,
    /// `true` se o estudo foi fundamentado numa passagem concreta do acervo.
    pub grounded: bool,
    /// Texto de escopo (assunto + respostas) para focar o aprofundamento.
    pub scope: String,
}

/// Mensagem da thread do estudo/refinamento para a UI.
enum StudyMsg {
    /// Uma rodada de refinamento: pergunta + opções.
    Round {
        question: String,
        options: Vec<String>,
    },
    /// Estudo (ou aprofundamento) pronto: Markdown legível + contexto retido.
    Done {
        markdown: String,
        ctx: Box<StudyContext>,
    },
    /// Erro.
    Error(String),
}

/// Overlay de conversa com a IA — uma **sessão** multi-turno e retomável; também
/// hospeda o assistente de estudo (ver [`StudyFlow`]).
#[derive(Debug, Clone)]
pub struct AiPanel {
    /// A conversa (turnos, contexto, provedor, modelo, timestamps).
    pub session: Session,
    /// Estado da última/atual consulta.
    pub status: AiStatus,
    /// Referências citadas (agregadas das respostas) para viagem rápida.
    pub refs: Vec<Reference>,
    /// Referência selecionada na lista de saltos.
    pub ref_selected: usize,
    /// Rolagem vertical da conversa (offset + clamp em [`ScrollState`]).
    pub scroll: ScrollState,
    /// Assistente de estudo em curso (refinamento de escopo), se houver.
    pub study: Option<StudyFlow>,
    /// Contexto de um estudo **concluído** (para aprofundar/responder no contexto).
    pub study_done: Option<StudyContext>,
}

impl AiPanel {
    /// Abre o painel a partir de uma sessão (recomputa as refs citadas).
    fn from_session(session: Session) -> Self {
        let refs = session_refs(&session);
        AiPanel {
            session,
            status: AiStatus::Idle,
            refs,
            ref_selected: 0,
            scroll: ScrollState::default(),
            study: None,
            study_done: None,
        }
    }

    /// Recalcula as refs citadas (após uma nova resposta).
    fn refresh_refs(&mut self) {
        self.refs = session_refs(&self.session);
        if self.ref_selected >= self.refs.len() {
            self.ref_selected = 0;
        }
    }
}

/// Navegador de conversas salvas (tecla `s`).
#[derive(Debug, Clone)]
pub struct SessionBrowser {
    /// Sessões salvas, da mais recente para a mais antiga.
    pub items: Vec<Session>,
    /// Item selecionado.
    pub selected: usize,
}

/// Tela de **preparo do estudo** (tecla `m`): escolhe o modo **e** a lente
/// denominacional antes de digitar o assunto.
#[derive(Debug, Clone)]
pub struct ModePicker {
    /// Modo selecionado (índice em [`StudyMode::all`]).
    pub selected: usize,
    /// Lente denominacional escolhida (inicia no padrão do config).
    pub lens: Denomination,
}

/// Estado do painel de dados acadêmicos (tecla `d`).
#[derive(Debug, Clone)]
pub enum ScholarlyState {
    /// Não instalado — mostra o convite à instalação.
    Absent,
    /// Instalado — contagens de tokens e entradas de léxico.
    Installed { tokens: i64, lexicon: i64 },
    /// Instalando — mensagem de progresso atual (com spinner).
    Installing(String),
    /// Erro na última tentativa.
    Error(String),
}

/// Painel de instalação dos dados acadêmicos (línguas originais + léxico).
#[derive(Debug, Clone)]
pub struct ScholarlyPanel {
    /// Estado atual.
    pub state: ScholarlyState,
}

/// Mensagem da thread de instalação para a UI.
enum ScholarlyMsg {
    /// Mensagem de progresso de fase.
    Progress(String),
    /// Concluído: `(id, registros)` por conjunto.
    Done(Vec<(String, usize)>),
    /// Erro (mensagem).
    Error(String),
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

/// Agrega as referências citadas em todas as respostas do assistente.
fn session_refs(session: &Session) -> Vec<Reference> {
    let mut out: Vec<Reference> = Vec::new();
    for m in &session.messages {
        if m.role == ChatRole::Assistant {
            for r in cited_refs(&m.content) {
                if !out.contains(&r) {
                    out.push(r);
                }
            }
        }
    }
    out
}

/// Título curto de uma conversa, derivado da primeira pergunta.
fn session_title(question: &str) -> String {
    let t = question.trim();
    let head: String = t.chars().take(40).collect();
    if t.chars().count() > 40 {
        format!("{head}…")
    } else {
        head
    }
}

/// Próxima/anterior lente denominacional, circular (`dir` = +1 ou -1).
fn next_lens(cur: Denomination, dir: isize) -> Denomination {
    let all = Denomination::all();
    let n = all.len() as isize;
    let i = all.iter().position(|d| *d == cur).unwrap_or(0) as isize;
    all[(((i + dir) % n + n) % n) as usize]
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

/// Executa um turno da conversa (usada pela thread e pelos testes). Estringe o
/// erro para cruzar a fronteira do canal (`AiError` não é `Send`-amigável aqui).
fn run_session_query(
    provider: &dyn LlmProvider,
    lang: Lang,
    context: &str,
    turns: &[ChatMessage],
    study: Option<(StudyMode, Denomination)>,
) -> Result<String, String> {
    ai::ask_session(provider, lang, context, turns, study).map_err(|e| e.to_string())
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
    /// Navegador de conversas salvas, se aberto.
    pub sessions: Option<SessionBrowser>,
    /// Seletor de modo de estudo padrão, se aberto.
    pub mode_picker: Option<ModePicker>,
    /// Painel de dados acadêmicos (instalação), se aberto.
    pub scholarly: Option<ScholarlyPanel>,
    /// Canal de progresso da instalação dos dados acadêmicos em andamento.
    scholarly_rx: Option<Receiver<ScholarlyMsg>>,
    /// Canal do assistente de estudo (rodada de refinamento ou estudo final).
    study_rx: Option<Receiver<StudyMsg>>,
    /// Caminho do banco (para reabrir numa thread de instalação). `None` = padrão.
    db_path: Option<PathBuf>,
    /// Retângulo interno da área de leitura no último frame (origem da seleção).
    pub reader_inner: Option<Rect>,
    /// Alvos clicáveis do último frame `(retângulo, alvo)`, do fundo ao topo
    /// (overlays empilham por último). Remontado a cada `draw`; ver
    /// [`App::hit_test`].
    pub click_targets: Vec<(Rect, ClickTarget)>,
    /// Seleção de texto via mouse em curso/fixada, se houver.
    pub selection: Option<Selection>,
    /// Texto da seleção atual, remontado a cada `draw` a partir do buffer visível.
    pub selection_text: String,
    /// Pedido de cópia pendente: o runtime copia `selection_text` após o próximo
    /// `draw` (que o remonta com a citação a partir do buffer fresco).
    copy_requested: bool,
    /// Mensagem efêmera de status (ex.: confirmação de cópia).
    pub toast: Option<String>,
    /// Ticks restantes até o toast desaparecer.
    toast_ticks: u8,
    /// Sai do loop quando `true`.
    pub should_quit: bool,
}

impl App {
    /// Cria a app abrindo a versão dada e carregando Gênesis 1. `db_path` é o
    /// caminho do banco (`None` = padrão), usado para reabrir numa thread de
    /// instalação dos dados acadêmicos.
    pub fn new(
        store: Store,
        initial_version: TranslationId,
        db_path: Option<PathBuf>,
    ) -> Result<Self> {
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
            sessions: None,
            mode_picker: None,
            scholarly: None,
            scholarly_rx: None,
            study_rx: None,
            db_path,
            reader_inner: None,
            click_targets: Vec::new(),
            selection: None,
            selection_text: String::new(),
            copy_requested: false,
            toast: None,
            toast_ticks: 0,
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

    /// Monta o contexto (RAG local) da pergunta pelo **mesmo** montador do núcleo
    /// usado pela CLi (`ask`): rótulo do capítulo, versículos numerados e as
    /// referências cruzadas agregadas por **todo o capítulo**. Devolve
    /// `(rótulo_da_âncora, contexto)`.
    fn ai_context(&self) -> (String, String) {
        let lang = self.lang();
        let reference = Reference::whole_chapter(self.book_number(), self.chapter);
        let label = format_reference(&reference, lang);
        let numbered = ai::numbered_verses(self.verses.iter().map(|(n, text)| (*n, text.as_str())));
        let nums: Vec<u16> = self.verses.iter().map(|(n, _)| *n).collect();
        let related = xref::passage_labels(self.store.conn(), &reference, &nums, lang, 8);
        let context = ai::ask_context(&label, &numbered, &related);
        (label, context)
    }

    /// Submete a pergunta digitada. Cria uma **nova conversa** (se não houver
    /// overlay aberto) ou **continua** a existente (follow-up); `mock` roda
    /// inline, os reais em thread. Provedor/chave são resolvidos do `config`.
    fn submit_ask(&mut self) {
        let question = match self.input.as_ref() {
            Some(i) if i.kind == InputKind::Ask => i.buffer.trim().to_string(),
            _ => return,
        };
        self.input = None;
        if question.is_empty() {
            return;
        }

        // Nova conversa: ancora no capítulo atual.
        if self.ai.is_none() {
            let (label, context) = self.ai_context();
            let session = Session::start(
                Session::generate_id(),
                session_title(&question),
                label,
                context,
                self.lang(),
                String::new(),
                String::new(),
            );
            self.ai = Some(AiPanel::from_session(session));
        }

        // Resolve provedor/chave do config atual (permite configurar e retomar).
        let name = self.config.provider.trim().to_ascii_lowercase();
        let panel = self.ai.as_mut().unwrap();
        panel.session.push(ChatRole::User, question);
        panel.scroll.jump_to_end(); // rola até o fim (clampado no próximo render)
        if name.is_empty() {
            panel.status = AiStatus::Error(
                "nenhum provedor de IA configurado — pressione c para configurar".to_string(),
            );
            return;
        }
        let key = if name == "anthropic" || name == "openai" {
            match KeyStore::open_default()
                .ok()
                .and_then(|ks| ks.get(&name).map(str::to_string))
            {
                Some(k) => Some(k),
                None => {
                    panel.status = AiStatus::Error(format!(
                        "sem chave para `{name}` — pressione c para configurar"
                    ));
                    return;
                }
            }
        } else {
            None
        };
        panel.session.provider = name.clone();
        panel.session.model = ai::providers::default_model(&name).to_string();
        panel.status = AiStatus::Pending;

        // Snapshot do que cruza a thread (o `Store`/`Session` não cruzam).
        let lang = panel.session.lang;
        let context = panel.session.context.clone();
        // Follow-up de um estudo concluído → preserva modo/lente no system prompt.
        let study = panel.study_done.as_ref().map(|c| (c.mode, c.lens));
        let turns: Vec<ChatMessage> = panel
            .session
            .messages
            .iter()
            .map(|m| ChatMessage {
                role: m.role,
                content: m.content.clone(),
            })
            .collect();

        // mock roda inline (instantâneo, offline): demo e testes.
        if name == "mock" {
            let outcome = match build_provider(&name, None, None) {
                Ok(p) => run_session_query(p.as_ref(), lang, &context, &turns, study),
                Err(e) => Err(e.to_string()),
            };
            self.apply_answer(outcome);
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let outcome = match build_provider(&name, key, None) {
                Ok(p) => run_session_query(p.as_ref(), lang, &context, &turns, study),
                Err(e) => Err(e.to_string()),
            };
            let _ = tx.send(outcome);
        });
        self.ai_rx = Some(rx);
        self.spinner = 0;
    }

    /// Aplica o resultado de uma consulta: anexa a resposta, persiste a sessão e
    /// recomputa as refs citadas. Só age se a consulta estava pendente.
    fn apply_answer(&mut self, outcome: Result<String, String>) {
        let Some(panel) = self.ai.as_mut() else {
            return;
        };
        if !matches!(panel.status, AiStatus::Pending) {
            return;
        }
        match outcome {
            Ok(answer) => {
                panel.session.push(ChatRole::Assistant, answer);
                panel.status = AiStatus::Idle;
                panel.refresh_refs();
                // Persiste a conversa (best-effort), tornando-a retomável.
                if let Ok(store) = SessionStore::open_default() {
                    let _ = store.put(&panel.session);
                }
            }
            Err(e) => panel.status = AiStatus::Error(e),
        }
    }

    /// Drena o canal da consulta em andamento (chamado a cada iteração do loop).
    pub fn poll_ai(&mut self) {
        let Some(rx) = self.ai_rx.as_ref() else {
            return;
        };
        match rx.try_recv() {
            Ok(outcome) => {
                self.ai_rx = None;
                self.apply_answer(outcome);
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.ai_rx = None;
                self.apply_answer(Err("consulta interrompida".to_string()));
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
        }
    }

    /// Resolve provedor + chave do config (compartilhado por ask/estudo).
    fn resolve_provider(&self) -> std::result::Result<(String, Option<String>), String> {
        let name = self.config.provider.trim().to_ascii_lowercase();
        if name.is_empty() {
            return Err(
                "nenhum provedor de IA configurado — pressione c para configurar".to_string(),
            );
        }
        let key = if name == "anthropic" || name == "openai" {
            match KeyStore::open_default()
                .ok()
                .and_then(|ks| ks.get(&name).map(str::to_string))
            {
                Some(k) => Some(k),
                None => {
                    return Err(format!(
                        "sem chave para `{name}` — pressione c para configurar"
                    ))
                }
            }
        } else {
            None
        };
        Ok((name, key))
    }

    /// Inicia o assistente de estudo no modo **e** lente dados (Enter na tela de
    /// preparo `m`). A lente escolhida vira o novo padrão lembrado.
    pub fn start_study(&mut self, mode: StudyMode, lens: Denomination) {
        self.mode_picker = None;
        // Modos acadêmico/pregação exigem os dados léxicos instalados.
        if mode.wants_lexical() && !scholarly::is_populated(self.store.conn()) {
            self.open_scholarly();
            self.toast = Some(format!(
                "O modo {} precisa dos dados léxicos — instale aqui (Enter).",
                mode.name_pt()
            ));
            self.toast_ticks = TOAST_TICKS;
            return;
        }
        // O assistente gera as rodadas com a IA: exige provedor/chave já agora.
        if let Err(e) = self.resolve_provider() {
            self.toast = Some(e);
            self.toast_ticks = TOAST_TICKS;
            return;
        }
        // Lembra a lente escolhida como padrão (config) para o próximo estudo.
        if self.config.study_lens != lens {
            self.config.study_lens = lens;
            let _ = self.config.save();
        }
        let lang = self.lang();
        let mut session = Session::start(
            Session::generate_id(),
            String::new(),
            String::new(),
            String::new(),
            lang,
            String::new(),
            String::new(),
        );
        session.study_mode = Some(mode);
        session.study_lens = Some(lens.slug().to_string());
        let mut panel = AiPanel::from_session(session);
        panel.study = Some(StudyFlow {
            mode,
            lens,
            brief: String::new(),
            round: 0,
            question: String::new(),
            options: Vec::new(),
            selected: 0,
            prior: Vec::new(),
        });
        self.ai = Some(panel);
        self.input = Some(Input {
            kind: InputKind::StudyBrief,
            buffer: String::new(),
            error: None,
        });
    }

    /// Envia o assunto (input `StudyBrief`) e dispara a 1ª rodada de refinamento.
    fn submit_brief(&mut self) {
        let brief = match self.input.as_ref() {
            Some(i) if i.kind == InputKind::StudyBrief => i.buffer.trim().to_string(),
            _ => return,
        };
        self.input = None;
        if brief.is_empty() {
            self.ai = None; // cancelou
            return;
        }
        let (mode, lens) = {
            let Some(panel) = self.ai.as_ref() else {
                return;
            };
            let Some(flow) = panel.study.as_ref() else {
                return;
            };
            (flow.mode, flow.lens)
        };
        if let Some(panel) = self.ai.as_mut() {
            if let Some(flow) = panel.study.as_mut() {
                flow.brief = brief.clone();
                flow.round = 1;
            }
            panel.session.title = format!(
                "{} · {} — {}",
                mode.name_pt(),
                lens.name_pt(),
                session_title(&brief)
            );
            panel
                .session
                .push(ChatRole::User, format!("Estudar: {brief}"));
            panel.scroll.jump_to_end();
        }
        self.spawn_refine(1);
    }

    /// Registra a resposta de uma rodada e avança (próxima rodada ou estudo).
    fn answer_round(&mut self, answer: String) {
        let answer = answer.trim().to_string();
        if answer.is_empty() {
            return;
        }
        let (round, question) = {
            let Some(panel) = self.ai.as_ref() else {
                return;
            };
            let Some(flow) = panel.study.as_ref() else {
                return;
            };
            if flow.options.is_empty() {
                return; // nenhuma rodada aguardando resposta
            }
            (flow.round, flow.question.clone())
        };
        if let Some(panel) = self.ai.as_mut() {
            panel.session.push(ChatRole::User, answer.clone());
            panel.scroll.jump_to_end();
            if let Some(flow) = panel.study.as_mut() {
                flow.prior.push((question, answer));
                flow.options.clear();
                if round < 3 {
                    flow.round = round + 1;
                }
            }
        }
        if round < 3 {
            self.spawn_refine(round + 1);
        } else {
            self.spawn_final_study();
        }
    }

    /// Dispara a rodada de refinamento `round` numa thread.
    fn spawn_refine(&mut self, round: u8) {
        let (mode, brief, prior, lang) = {
            let Some(panel) = self.ai.as_ref() else {
                return;
            };
            let Some(flow) = panel.study.as_ref() else {
                return;
            };
            (
                flow.mode,
                flow.brief.clone(),
                flow.prior.clone(),
                panel.session.lang,
            )
        };
        let (name, key) = match self.resolve_provider() {
            Ok(v) => v,
            Err(e) => {
                self.set_study_error(e);
                return;
            }
        };
        if let Some(panel) = self.ai.as_mut() {
            panel.status = AiStatus::Pending;
        }
        self.spinner = 0;
        let f = move |provider: &dyn LlmProvider| -> StudyMsg {
            match ai::refine_scope(provider, mode, lang, &brief, &prior, round) {
                Ok(r) => StudyMsg::Round {
                    question: r.question,
                    options: r.options,
                },
                Err(e) => StudyMsg::Error(e.to_string()),
            }
        };
        self.spawn_study_worker(name, key, f);
    }

    /// Após a 3ª rodada: resolve o escopo (passagem vs temático) e estuda.
    fn spawn_final_study(&mut self) {
        let (mode, lens, brief, prior) = {
            let Some(panel) = self.ai.as_ref() else {
                return;
            };
            let Some(flow) = panel.study.as_ref() else {
                return;
            };
            (flow.mode, flow.lens, flow.brief.clone(), flow.prior.clone())
        };
        let lang = self.lang();
        // Escopo = assunto + respostas (para resolver a referência e focar).
        let mut scope = brief.clone();
        for (_, a) in &prior {
            scope.push_str("; ");
            scope.push_str(a);
        }
        // Resolve uma passagem concreta do acervo a partir do escopo; senão,
        // estudo temático ancorado (informalmente) no capítulo aberto.
        let resolved = scan_references(&scope)
            .into_iter()
            .map(|s| s.reference)
            .find(|r| {
                EmbeddedSource::new(&self.store)
                    .passage(r, self.version())
                    .is_ok()
            });
        let (reference, reference_label, grounded) = match resolved {
            Some(r) => (r, format_reference(&r, lang), true),
            None => (
                Reference::whole_chapter(self.book_number(), self.chapter),
                session_title(&brief),
                false,
            ),
        };
        let ctx = StudyContext {
            mode,
            lens,
            depth: mode.implied_depth(),
            reference,
            reference_label,
            grounded,
            scope: scope.clone(),
        };
        self.run_study(ctx, scope);
    }

    /// Executa um estudo a partir de um [`StudyContext`] (compartilhado pelo
    /// estudo inicial e pelo aprofundamento): refaz a fundamentação (passagem +
    /// léxico + refs) quando há passagem, dispara o worker e retém o contexto.
    fn run_study(&mut self, ctx: StudyContext, request_brief: String) {
        let lang = self.lang();
        let (passage, cross_references, verified_lexicon) = if ctx.grounded {
            match EmbeddedSource::new(&self.store).passage(&ctx.reference, self.version()) {
                Ok(p) => {
                    let nums = p.verse_numbers();
                    let xlimit = if ctx.mode.wants_lexical() { 16 } else { 8 };
                    let xrefs = xref::passage_labels(
                        self.store.conn(),
                        &ctx.reference,
                        &nums,
                        lang,
                        xlimit,
                    );
                    let vl = if ctx.mode.wants_lexical() {
                        ai::verified_lexicon(self.store.conn(), &ctx.reference, &nums, lang, 16)
                    } else {
                        VerifiedLexicon::default()
                    };
                    (Some(p), xrefs, vl)
                }
                Err(_) => (None, Vec::new(), VerifiedLexicon::default()),
            }
        } else {
            (None, Vec::new(), VerifiedLexicon::default())
        };
        let (name, key) = match self.resolve_provider() {
            Ok(v) => v,
            Err(e) => {
                self.set_study_error(e);
                return;
            }
        };
        if let Some(panel) = self.ai.as_mut() {
            panel.session.provider = name.clone();
            panel.session.model = ai::providers::default_model(&name).to_string();
            panel.status = AiStatus::Pending;
        }
        self.spinner = 0;
        let f = move |provider: &dyn LlmProvider| -> StudyMsg {
            let req = StudyRequest {
                reference: ctx.reference,
                reference_label: ctx.reference_label.clone(),
                mode: ctx.mode,
                lens: ctx.lens,
                depth: ctx.depth,
                language: lang,
                passage: passage.as_ref(),
                cross_references,
                verified_lexicon,
                web_sources: Vec::new(),
                brief: Some(request_brief),
            };
            match ai::study(provider, &req) {
                Ok(result) => StudyMsg::Done {
                    markdown: result.to_markdown(),
                    ctx: Box::new(ctx),
                },
                Err(e) => StudyMsg::Error(e.to_string()),
            }
        };
        self.spawn_study_worker(name, key, f);
    }

    /// Roda `f` (refinar ou estudar) inline (mock) ou numa thread; o resultado
    /// vai para `study_rx` (drenado por [`App::poll_study`]).
    fn spawn_study_worker<F>(&mut self, name: String, key: Option<String>, f: F)
    where
        F: FnOnce(&dyn LlmProvider) -> StudyMsg + Send + 'static,
    {
        if name == "mock" {
            let msg = match build_provider(&name, None, None) {
                Ok(p) => f(p.as_ref()),
                Err(e) => StudyMsg::Error(e.to_string()),
            };
            self.apply_study_msg(msg);
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let msg = match build_provider(&name, key, None) {
                Ok(p) => f(p.as_ref()),
                Err(e) => StudyMsg::Error(e.to_string()),
            };
            let _ = tx.send(msg);
        });
        self.study_rx = Some(rx);
    }

    /// Aplica uma mensagem do estudo (rodada, conclusão ou erro).
    fn apply_study_msg(&mut self, msg: StudyMsg) {
        let Some(panel) = self.ai.as_mut() else {
            return;
        };
        match msg {
            StudyMsg::Round { question, options } => {
                panel.session.push(ChatRole::Assistant, question.clone());
                if let Some(flow) = panel.study.as_mut() {
                    flow.question = question;
                    flow.options = options;
                    flow.selected = 0;
                }
                panel.status = AiStatus::Idle;
                panel.scroll.jump_to_end();
            }
            StudyMsg::Done { markdown, ctx } => {
                panel.session.push(ChatRole::Assistant, markdown);
                panel.status = AiStatus::Idle;
                panel.study = None; // assistente concluído; vira estudo/sessão normal
                                    // Semente de contexto p/ follow-ups na mesma lente/fundamentação.
                if panel.session.context.trim().is_empty() {
                    panel.session.context = format!(
                        "Estudo {} sob a lente {}. Escopo: {} — {}.",
                        ctx.mode.name_pt(),
                        ctx.lens.name_pt(),
                        ctx.reference_label,
                        ctx.scope
                    );
                }
                panel.study_done = Some(*ctx); // retém p/ aprofundar (+) e follow-up (a)
                panel.refresh_refs();
                panel.scroll.jump_to_end();
                if let Ok(store) = SessionStore::open_default() {
                    let _ = store.put(&panel.session);
                }
            }
            StudyMsg::Error(e) => panel.status = AiStatus::Error(e),
        }
    }

    /// Define um erro no painel de estudo (sem provedor, etc.).
    fn set_study_error(&mut self, e: String) {
        if let Some(panel) = self.ai.as_mut() {
            panel.status = AiStatus::Error(e);
        }
    }

    /// Drena o canal do assistente de estudo (a cada iteração do loop).
    pub fn poll_study(&mut self) {
        let Some(rx) = self.study_rx.as_ref() else {
            return;
        };
        match rx.try_recv() {
            Ok(msg) => {
                self.study_rx = None;
                self.apply_study_msg(msg);
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.study_rx = None;
                self.apply_study_msg(StudyMsg::Error("estudo interrompido".to_string()));
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
        }
    }

    /// Avança o indicador de "consultando…" (chamado nos ticks ociosos do loop).
    pub fn tick(&mut self) {
        let ai_pending = matches!(self.ai.as_ref().map(|p| &p.status), Some(AiStatus::Pending));
        let installing = matches!(
            self.scholarly.as_ref().map(|p| &p.state),
            Some(ScholarlyState::Installing(_))
        );
        if ai_pending || installing {
            self.spinner = self.spinner.wrapping_add(1);
        }
        // Esvanece o toast de status após alguns ticks ociosos.
        if self.toast_ticks > 0 {
            self.toast_ticks -= 1;
            if self.toast_ticks == 0 {
                self.toast = None;
            }
        }
    }

    /// `true` se algum overlay modal está aberto (captura teclado/mouse).
    fn overlay_open(&self) -> bool {
        self.show_help
            || self.ai.is_some()
            || self.settings.is_some()
            || self.sessions.is_some()
            || self.mode_picker.is_some()
            || self.scholarly.is_some()
    }

    /// Limpa a seleção de texto do mouse e o texto copiável associado.
    fn clear_selection(&mut self) {
        self.selection = None;
        self.selection_text.clear();
    }

    /// Consome o pedido de cópia pendente (o runtime executa o IO da área de
    /// transferência fora desta lógica pura, lendo `selection_text`).
    pub fn take_copy_request(&mut self) -> bool {
        std::mem::take(&mut self.copy_requested)
    }

    /// Registra o desfecho de uma cópia, exibindo um toast de confirmação/erro.
    pub fn notify_copied(&mut self, copied: bool, chars: usize) {
        self.toast = Some(if copied {
            format!("✓ {chars} caracteres copiados")
        } else {
            "⚠ não foi possível copiar".to_string()
        });
        self.toast_ticks = TOAST_TICKS;
    }

    /// Processa um evento de mouse: seleção de texto **restrita à área de
    /// leitura**. Começar o arrasto fora dos versículos não inicia seleção; o
    /// arrasto é grampeado ao retângulo. A cópia é diferida (ver
    /// [`App::take_copy_request`]); a roda rola pelos versículos.
    pub fn handle_mouse(&mut self, m: MouseEvent) {
        // Overlays modais: clique nas linhas/opções/botões e a roda rola o foco.
        if self.overlay_open() {
            self.handle_overlay_mouse(m);
            return;
        }
        match m.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                // Clique numa linha de livro (lista à esquerda) seleciona o livro.
                if let Some(ClickTarget::Book(i)) = self.hit_test(m.column, m.row) {
                    self.clear_selection();
                    self.select_book(i);
                    self.focus = Focus::Books;
                    return;
                }
                self.selection = match self.reader_inner {
                    // Só inicia se o clique nasceu DENTRO da área de versículos.
                    Some(inner) if rect_contains(inner, m.column, m.row) => {
                        let p = (m.column, m.row);
                        Some(Selection {
                            anchor: p,
                            cursor: p,
                            dragging: true,
                        })
                    }
                    _ => None,
                };
                self.selection_text.clear();
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                if let (Some(inner), Some(sel)) = (self.reader_inner, self.selection.as_mut()) {
                    if sel.dragging {
                        sel.cursor = clamp_to_rect(inner, m.column, m.row);
                    }
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                // Finaliza mesmo sem `reader_inner` (ex.: resize no meio do arrasto).
                let inner = self.reader_inner;
                let Some(sel) = self.selection.as_mut() else {
                    return;
                };
                sel.dragging = false;
                // A posição de soltura é a autoritativa (cobre arrastos rápidos
                // em que o terminal não emitiu um Drag final).
                if let Some(inner) = inner {
                    sel.cursor = clamp_to_rect(inner, m.column, m.row);
                }
                // Clique simples (sem arrastar): move o cursor para o versículo
                // clicado, se houver; senão apenas desfaz a seleção.
                if sel.anchor == sel.cursor {
                    let at = sel.anchor;
                    self.clear_selection();
                    if let Some(ClickTarget::Verse(i)) = self.hit_test(at.0, at.1) {
                        if i < self.verses.len() {
                            self.selected = i;
                            self.focus = Focus::Reader;
                        }
                    }
                    return;
                }
                // Pede a cópia: o próximo `draw` remonta `selection_text` (com a
                // citação) a partir do buffer e o runtime copia em seguida.
                self.copy_requested = true;
            }
            // A roda do mouse é ignorada na app (use ↑↓/PgUp/PgDn): evita a
            // enxurrada de eventos que travava o loop sob a captura de mouse.
            _ => {}
        }
    }

    /// Registra um alvo clicável do frame atual (chamado pela renderização).
    pub fn push_click(&mut self, rect: Rect, target: ClickTarget) {
        self.click_targets.push((rect, target));
    }

    /// Alvo clicável mais ao topo que contém `(col, row)`, se houver. Os overlays
    /// empilham por último, então a busca é de trás para frente.
    fn hit_test(&self, col: u16, row: u16) -> Option<ClickTarget> {
        self.click_targets
            .iter()
            .rev()
            .find(|(r, _)| rect_contains(*r, col, row))
            .map(|(_, t)| *t)
    }

    /// Mouse sob um overlay modal: clica linhas/opções/botões. A roda é ignorada
    /// (rolagem por teclado), pois a captura de mouse a torna uma enxurrada.
    fn handle_overlay_mouse(&mut self, m: MouseEvent) {
        if let MouseEventKind::Down(MouseButton::Left) = m.kind {
            // Só alvos de overlay (ignora livros/versículos por baixo do modal).
            let target = self
                .click_targets
                .iter()
                .rev()
                .find(|(r, t)| t.is_overlay() && rect_contains(*r, m.column, m.row))
                .map(|(_, t)| *t);
            if let Some(target) = target {
                self.activate_click(target);
            }
        }
    }

    /// Aplica o efeito de um clique num alvo. Para listas, o 1º clique seleciona a
    /// linha e o clique na linha já selecionada a ativa (abre/inicia/define).
    fn activate_click(&mut self, target: ClickTarget) {
        match target {
            ClickTarget::Book(i) => {
                self.clear_selection();
                self.select_book(i);
                self.focus = Focus::Books;
            }
            ClickTarget::Verse(i) => {
                if i < self.verses.len() {
                    self.selected = i;
                    self.focus = Focus::Reader;
                }
            }
            ClickTarget::ModeRow(i) => {
                let act = matches!(self.mode_picker.as_ref(), Some(p) if p.selected == i);
                let lens = self.mode_picker.as_ref().map(|p| p.lens);
                if let Some(p) = self.mode_picker.as_mut() {
                    p.selected = i;
                }
                if act {
                    if let (Some(mode), Some(lens)) = (StudyMode::all().get(i).copied(), lens) {
                        self.start_study(mode, lens);
                    }
                }
            }
            ClickTarget::LensCycle => {
                if let Some(p) = self.mode_picker.as_mut() {
                    p.lens = next_lens(p.lens, 1);
                }
            }
            ClickTarget::SessionRow(i) => {
                let act = matches!(self.sessions.as_ref(), Some(b) if b.selected == i);
                if let Some(b) = self.sessions.as_mut() {
                    if i < b.items.len() {
                        b.selected = i;
                    }
                }
                if act {
                    let s = self.sessions.as_ref().and_then(|b| b.items.get(i).cloned());
                    if let Some(s) = s {
                        self.sessions = None;
                        self.ai = Some(AiPanel::from_session(s));
                    }
                }
            }
            ClickTarget::SettingsRow(i) => {
                let act = matches!(self.settings.as_ref(), Some(s) if s.selected == i);
                if let Some(s) = self.settings.as_mut() {
                    if i < PROVIDERS.len() {
                        s.selected = i;
                    }
                }
                if act && i < PROVIDERS.len() {
                    self.config.provider = PROVIDERS[i].to_string();
                    let _ = self.config.save();
                }
            }
            ClickTarget::StudyOption(i) => {
                let text = self
                    .ai
                    .as_ref()
                    .and_then(|p| p.study.as_ref())
                    .and_then(|f| f.options.get(i))
                    .cloned();
                if let Some(t) = text {
                    self.answer_round(t);
                }
            }
            ClickTarget::ScholarlyInstall => {
                let can = matches!(
                    self.scholarly.as_ref().map(|p| &p.state),
                    Some(ScholarlyState::Absent) | Some(ScholarlyState::Error(_))
                );
                if can {
                    self.start_scholarly_install();
                }
            }
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
        // Ações diferidas (resolvidas sob o empréstimo do painel, aplicadas depois).
        let mut jump: Option<Reference> = None;
        let mut followup = false;
        let mut pick: Option<usize> = None;
        let mut custom = false;
        let mut export = false;
        let mut cycle_lens = false;
        let mut deepen = false;
        if let Some(panel) = self.ai.as_mut() {
            let n = panel.refs.len();
            let busy = matches!(panel.status, AiStatus::Pending);
            // Refinando o escopo: há opções a escolher → teclas mudam de papel.
            let refining = panel
                .study
                .as_ref()
                .map(|f| f.options.len())
                .filter(|c| *c > 0);
            if let Some(count) = refining {
                let flow = panel.study.as_mut().unwrap();
                match key.code {
                    KeyCode::Esc | KeyCode::Char('q') => {
                        self.ai = None;
                        return;
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        if flow.selected + 1 < count {
                            flow.selected += 1;
                        }
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        flow.selected = flow.selected.saturating_sub(1);
                    }
                    // Enter escolhe a opção destacada; dígitos escolhem direto.
                    KeyCode::Enter => pick = Some(flow.selected),
                    KeyCode::Char(c @ '1'..='9') => {
                        let i = (c as u8 - b'1') as usize;
                        if i < count {
                            pick = Some(i);
                        }
                    }
                    // `c`/`o`: digitar a própria resposta (custom).
                    KeyCode::Char('c') | KeyCode::Char('o') => custom = true,
                    _ => {}
                }
            } else {
                // Sessão de estudo concluída → habilita exportar/lente/aprofundar.
                let has_study = panel.session.study_mode.is_some() && panel.study.is_none();
                let can_deepen = panel.study_done.is_some();
                match key.code {
                    KeyCode::Esc | KeyCode::Char('q') => {
                        self.ai = None;
                        return;
                    }
                    // `a` continua a conversa (abre o campo de pergunta), exceto se ocupado.
                    KeyCode::Char('a') if !busy => followup = true,
                    // `+` aprofunda o estudo (re-roda mais detalhado), exceto se ocupado.
                    KeyCode::Char('+') if can_deepen && !busy => deepen = true,
                    // `e` exporta o estudo (.md); `L` cicla a lente denominacional.
                    KeyCode::Char('e') if has_study && !busy => export = true,
                    KeyCode::Char('L') if has_study && !busy => cycle_lens = true,
                    KeyCode::Down | KeyCode::Char('j') => panel.scroll.down(1),
                    KeyCode::Up | KeyCode::Char('k') => panel.scroll.up(1),
                    KeyCode::PageDown | KeyCode::Char(' ') => panel.scroll.down(10),
                    KeyCode::PageUp => panel.scroll.up(10),
                    // Seleciona entre as referências citadas.
                    KeyCode::Tab if n > 0 => panel.ref_selected = (panel.ref_selected + 1) % n,
                    KeyCode::BackTab if n > 0 => {
                        panel.ref_selected = (panel.ref_selected + n - 1) % n
                    }
                    // Enter salta para a referência selecionada.
                    KeyCode::Enter => jump = panel.refs.get(panel.ref_selected).copied(),
                    // Dígitos 1–9: salto direto para a n-ésima referência citada.
                    KeyCode::Char(c @ '1'..='9') => {
                        jump = panel.refs.get((c as u8 - b'1') as usize).copied();
                    }
                    _ => {}
                }
            }
        }
        if let Some(i) = pick {
            let text = self
                .ai
                .as_ref()
                .and_then(|p| p.study.as_ref())
                .and_then(|f| f.options.get(i))
                .cloned();
            if let Some(t) = text {
                self.answer_round(t);
            }
        } else if custom {
            self.input = Some(Input {
                kind: InputKind::StudyCustom,
                buffer: String::new(),
                error: None,
            });
        } else if export {
            self.export_study();
        } else if cycle_lens {
            self.cycle_lens();
        } else if deepen {
            self.open_deepen(); // abre o input do foco do aprofundamento
        } else if followup {
            self.open_ask(); // abre o input; a precedência leva a digitação para lá
        } else if let Some(reference) = jump {
            self.ai = None;
            self.go_to(&reference);
        }
    }

    /// Exporta o estudo atual (última resposta do assistente) para
    /// `studies/<slug>.md`. Apenas Markdown — sem pandoc (não trava a UI).
    fn export_study(&mut self) {
        let Some(panel) = self.ai.as_ref() else {
            return;
        };
        let md = panel
            .session
            .messages
            .iter()
            .rev()
            .find(|m| m.role == ChatRole::Assistant)
            .map(|m| m.content.clone());
        let Some(md) = md else {
            return;
        };
        let title = if panel.session.title.trim().is_empty() {
            "estudo".to_string()
        } else {
            panel.session.title.clone()
        };
        let result = (|| -> std::result::Result<PathBuf, String> {
            let dir = the_light_core::userdata::studies_dir()
                .map_err(|e| format!("sem diretório de estudos: {e}"))?;
            std::fs::create_dir_all(&dir)
                .map_err(|e| format!("erro ao criar {}: {e}", dir.display()))?;
            let slug = the_light_core::export::slugify(&title);
            let slug = if slug.is_empty() {
                "estudo".to_string()
            } else {
                slug
            };
            let path = dir.join(format!("{slug}.md"));
            the_light_core::export::export_document(&md, &path)?;
            Ok(path)
        })();
        let (msg, ticks) = match result {
            Ok(path) => (
                format!("✓ Estudo exportado: {}", path.display()),
                TOAST_TICKS,
            ),
            Err(e) => (format!("Erro ao exportar: {e}"), TOAST_TICKS),
        };
        self.toast = Some(msg);
        self.toast_ticks = ticks;
    }

    /// Cicla a lente denominacional padrão (config). Estudos já concluídos não
    /// mudam — re-rodar/aprofundar usa a nova lente.
    fn cycle_lens(&mut self) {
        let next = next_lens(self.config.study_lens, 1);
        self.config.study_lens = next;
        let _ = self.config.save();
        if let Some(flow) = self.ai.as_mut().and_then(|p| p.study.as_mut()) {
            flow.lens = next;
        }
        if let Some(ctx) = self.ai.as_mut().and_then(|p| p.study_done.as_mut()) {
            ctx.lens = next;
        }
        self.toast = Some(format!("Lente: {}", next.name_pt()));
        self.toast_ticks = TOAST_TICKS;
    }

    /// Abre o campo de **foco do aprofundamento** (`+`) de um estudo concluído.
    fn open_deepen(&mut self) {
        if self
            .ai
            .as_ref()
            .and_then(|p| p.study_done.as_ref())
            .is_none()
        {
            return;
        }
        self.input = Some(Input {
            kind: InputKind::StudyDeepen,
            buffer: String::new(),
            error: None,
        });
    }

    /// Aprofunda o estudo concluído: re-roda **um nível mais fundo** (mesma
    /// passagem/lente), com o foco dado (ou o escopo anterior), e anexa o
    /// resultado como uma nova seção na mesma sessão.
    fn spawn_deepen(&mut self, focus: String) {
        let Some(prev) = self.ai.as_ref().and_then(|p| p.study_done.clone()) else {
            return;
        };
        let focus = focus.trim().to_string();
        // Brief do pedido = foco do usuário, ou o escopo anterior ("aprofundar").
        let request_brief = if focus.is_empty() {
            format!("Aprofundar: {}", prev.scope)
        } else {
            focus.clone()
        };
        // Registra o pedido como turno do usuário (visível no histórico).
        if let Some(panel) = self.ai.as_mut() {
            let label = if focus.is_empty() {
                "Aprofundar o estudo".to_string()
            } else {
                format!("Aprofundar: {focus}")
            };
            panel.session.push(ChatRole::User, label);
            panel.scroll.jump_to_end();
        }
        // Mesmo contexto, um nível mais profundo (satura em WordStudy).
        let ctx = StudyContext {
            depth: prev.depth.deeper(),
            ..prev
        };
        self.run_study(ctx, request_brief);
    }

    /// Abre o navegador de conversas salvas (tecla `s`).
    pub fn open_sessions(&mut self) {
        let items = SessionStore::open_default()
            .and_then(|s| s.list())
            .unwrap_or_default();
        self.sessions = Some(SessionBrowser { items, selected: 0 });
    }

    fn handle_sessions_key(&mut self, key: KeyEvent) {
        // Ações diferidas para evitar conflito de empréstimo com `self.ai`/store.
        let mut open: Option<Session> = None;
        let mut delete: Option<String> = None;
        if let Some(browser) = self.sessions.as_mut() {
            let n = browser.items.len();
            match key.code {
                KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('s') => {
                    self.sessions = None;
                    return;
                }
                KeyCode::Down | KeyCode::Char('j') if n > 0 => {
                    if browser.selected + 1 < n {
                        browser.selected += 1;
                    }
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    browser.selected = browser.selected.saturating_sub(1);
                }
                KeyCode::Enter => open = browser.items.get(browser.selected).cloned(),
                KeyCode::Char('d') => {
                    delete = browser.items.get(browser.selected).map(|s| s.id.clone());
                }
                _ => {}
            }
        }
        if let Some(session) = open {
            self.sessions = None;
            self.ai = Some(AiPanel::from_session(session));
        } else if let Some(id) = delete {
            if let Ok(store) = SessionStore::open_default() {
                let _ = store.delete(&id);
            }
            if let Some(browser) = self.sessions.as_mut() {
                browser.items.retain(|s| s.id != id);
                if browser.selected >= browser.items.len() {
                    browser.selected = browser.items.len().saturating_sub(1);
                }
            }
        }
    }

    /// Abre a tela de preparo do estudo (tecla `m`): modo + lente.
    pub fn open_mode_picker(&mut self) {
        let selected = StudyMode::all()
            .iter()
            .position(|m| *m == self.config.study_mode)
            .unwrap_or(0);
        self.mode_picker = Some(ModePicker {
            selected,
            lens: self.config.study_lens,
        });
    }

    fn handle_mode_picker_key(&mut self, key: KeyEvent) {
        let modes = StudyMode::all();
        let mut start: Option<(StudyMode, Denomination)> = None;
        let mut set_default: Option<(StudyMode, Denomination)> = None;
        if let Some(picker) = self.mode_picker.as_mut() {
            match key.code {
                KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('m') => {
                    self.mode_picker = None;
                    return;
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if picker.selected + 1 < modes.len() {
                        picker.selected += 1;
                    }
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    picker.selected = picker.selected.saturating_sub(1);
                }
                // ←/→ (ou `l`) ciclam a lente denominacional.
                KeyCode::Right | KeyCode::Char('l') => picker.lens = next_lens(picker.lens, 1),
                KeyCode::Left | KeyCode::Char('h') => picker.lens = next_lens(picker.lens, -1),
                // Enter inicia o estudo com o modo + a lente escolhidos.
                KeyCode::Enter => {
                    start = modes.get(picker.selected).map(|&m| (m, picker.lens));
                }
                // `s` define modo + lente como padrão (sem iniciar o estudo).
                KeyCode::Char('s') => {
                    set_default = modes.get(picker.selected).map(|&m| (m, picker.lens));
                }
                _ => {}
            }
        }
        if let Some((mode, lens)) = start {
            self.start_study(mode, lens);
        } else if let Some((mode, lens)) = set_default {
            // Persiste modo + lente como padrão.
            self.config.study_mode = mode;
            self.config.study_lens = lens;
            let _ = self.config.save();
            self.mode_picker = None;
            self.toast = Some(format!("Padrão: {} · {}", mode.name_pt(), lens.name_pt()));
            self.toast_ticks = TOAST_TICKS;
        }
    }

    /// Abre o painel de dados acadêmicos (tecla `d`), consultando o estado atual.
    pub fn open_scholarly(&mut self) {
        let state = self.scholarly_state();
        self.scholarly = Some(ScholarlyPanel { state });
    }

    /// Estado de instalação atual a partir do banco aberto.
    fn scholarly_state(&self) -> ScholarlyState {
        let conn = self.store.conn();
        if scholarly::is_populated(conn) {
            let tokens = conn
                .query_row("SELECT count(*) FROM original_tokens", [], |r| r.get(0))
                .unwrap_or(0);
            let lexicon = conn
                .query_row("SELECT count(*) FROM lexicon", [], |r| r.get(0))
                .unwrap_or(0);
            ScholarlyState::Installed { tokens, lexicon }
        } else {
            ScholarlyState::Absent
        }
    }

    fn handle_scholarly_key(&mut self, key: KeyEvent) {
        let (installing, installed) = match self.scholarly.as_ref().map(|p| &p.state) {
            Some(ScholarlyState::Installing(_)) => (true, false),
            Some(ScholarlyState::Installed { .. }) => (false, true),
            _ => (false, false),
        };
        // Travado durante a instalação: ignora teclas (a thread segue no fundo).
        if installing {
            return;
        }
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('d') => self.scholarly = None,
            KeyCode::Enter | KeyCode::Char('i') if !installed => self.start_scholarly_install(),
            _ => {}
        }
    }

    /// Dispara a instalação dos dados acadêmicos numa thread de fundo (baixa +
    /// importa), reportando progresso por canal — mesma forma da consulta de IA.
    fn start_scholarly_install(&mut self) {
        let db_path = self.db_path.clone();
        let seed_dir = the_light_core::userdata::data_dir()
            .map(|d| d.join("seed").join("scholarly"))
            .unwrap_or_else(|_| PathBuf::from("data/seed/scholarly"));
        let datasets = scholarly::default_datasets();

        let (tx, rx) = std::sync::mpsc::channel();
        let progress_tx = tx.clone();
        std::thread::spawn(move || {
            let result = (|| -> std::result::Result<Vec<(String, usize)>, String> {
                let mut store = match &db_path {
                    Some(p) => Store::open(p),
                    None => Store::open_default(),
                }
                .map_err(|e| e.to_string())?;
                let mut cb = |msg: &str| {
                    let _ = progress_tx.send(ScholarlyMsg::Progress(msg.to_string()));
                };
                scholarly::import(
                    store.conn_mut(),
                    &datasets,
                    &seed_dir,
                    false,
                    false,
                    &mut cb,
                )
                .map_err(|e| e.to_string())
            })();
            let _ = tx.send(match result {
                Ok(s) => ScholarlyMsg::Done(s),
                Err(e) => ScholarlyMsg::Error(e),
            });
        });
        self.scholarly_rx = Some(rx);
        self.scholarly = Some(ScholarlyPanel {
            state: ScholarlyState::Installing("Iniciando…".to_string()),
        });
        self.spinner = 0;
    }

    /// Drena o canal da instalação dos dados acadêmicos (a cada iteração do loop).
    pub fn poll_scholarly(&mut self) {
        if self.scholarly_rx.is_none() {
            return;
        }
        loop {
            let msg = match self.scholarly_rx.as_ref().unwrap().try_recv() {
                Ok(m) => m,
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.scholarly_rx = None;
                    if let Some(p) = self.scholarly.as_mut() {
                        p.state = ScholarlyState::Error("instalação interrompida".to_string());
                    }
                    break;
                }
            };
            match msg {
                ScholarlyMsg::Progress(m) => {
                    if let Some(p) = self.scholarly.as_mut() {
                        p.state = ScholarlyState::Installing(m);
                    }
                }
                ScholarlyMsg::Done(summary) => {
                    self.scholarly_rx = None;
                    let total: usize = summary.iter().map(|(_, n)| n).sum();
                    // Reabre o estado a partir do banco (a thread já comitou).
                    let state = self.scholarly_state();
                    if let Some(p) = self.scholarly.as_mut() {
                        p.state = state;
                    }
                    self.toast = Some(format!("✓ Dados acadêmicos instalados ({total} registros)"));
                    self.toast_ticks = TOAST_TICKS;
                    break;
                }
                ScholarlyMsg::Error(e) => {
                    self.scholarly_rx = None;
                    if let Some(p) = self.scholarly.as_mut() {
                        p.state = ScholarlyState::Error(e);
                    }
                    break;
                }
            }
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
        // Qualquer tecla cancela a seleção de texto do mouse (como num editor).
        self.clear_selection();
        // Os modais capturam o teclado, em ordem de prioridade. `input` vem antes
        // do overlay de IA: assim a digitação de um follow-up vai para a caixa de
        // texto enquanto a conversa continua visível ao fundo.
        if self.settings.is_some() {
            self.handle_settings_key(key);
            return;
        }
        if self.sessions.is_some() {
            self.handle_sessions_key(key);
            return;
        }
        if self.mode_picker.is_some() {
            self.handle_mode_picker_key(key);
            return;
        }
        if self.scholarly.is_some() {
            self.handle_scholarly_key(key);
            return;
        }
        if self.input.is_some() {
            self.handle_input_key(key);
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
            KeyCode::Char('s') => self.open_sessions(),
            KeyCode::Char('m') => self.open_mode_picker(),
            KeyCode::Char('d') => self.open_scholarly(),
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
        let kind = input.kind;
        match key.code {
            KeyCode::Esc => {
                self.input = None;
                self.search_results.clear();
                // Esc no assunto do estudo abandona o assistente inteiro;
                // numa resposta própria, volta às opções (mantém o painel).
                if kind == InputKind::StudyBrief {
                    self.ai = None;
                }
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
            InputKind::StudyBrief => self.submit_brief(),
            InputKind::StudyCustom => {
                let answer = input.buffer.clone();
                self.input = None;
                self.answer_round(answer);
            }
            InputKind::StudyDeepen => {
                let focus = input.buffer.clone();
                self.input = None;
                self.spawn_deepen(focus);
            }
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

    #[test]
    fn scholarly_panel_opens_reflects_state_and_closes() {
        let mut app = seeded_app(); // banco em memória, sem dados acadêmicos
        app.handle_key(key(KeyCode::Char('d')));
        assert!(
            matches!(
                app.scholarly.as_ref().map(|p| &p.state),
                Some(ScholarlyState::Absent)
            ),
            "base DB → Absent"
        );
        app.handle_key(key(KeyCode::Esc));
        assert!(app.scholarly.is_none());

        // Com tokens no banco, o painel reflete "instalado".
        app.store
            .conn()
            .execute(
                "INSERT INTO scholarly_sources(id,name,license,embeddable,attribution,url,version) \
                 VALUES ('tahot','t','cc-by',1,'a','u','v')",
                [],
            )
            .unwrap();
        app.store
            .conn()
            .execute(
                "INSERT INTO original_tokens(testament,book_number,chapter,verse,word_index,\
                 surface,strongs,source_id) VALUES ('OT',1,1,1,0,'x','H7225','tahot')",
                [],
            )
            .unwrap();
        app.handle_key(key(KeyCode::Char('d')));
        assert!(matches!(
            app.scholarly.as_ref().map(|p| &p.state),
            Some(ScholarlyState::Installed { tokens: 1, .. })
        ));
    }

    #[test]
    fn mode_picker_opens_seeds_navigates_and_closes() {
        let mut app = seeded_app();
        // Padrão é Introdutório (índice 2 em StudyMode::all()).
        assert_eq!(app.config.study_mode, StudyMode::Introductory);
        app.handle_key(key(KeyCode::Char('m')));
        assert_eq!(
            app.mode_picker.as_ref().expect("picker aberto").selected,
            2,
            "seleção semeada a partir do modo atual"
        );
        // Sobe até o topo (Acadêmico).
        app.handle_key(key(KeyCode::Up));
        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.mode_picker.as_ref().unwrap().selected, 0);
        // Esc fecha sem alterar o padrão (não grava disco).
        app.handle_key(key(KeyCode::Esc));
        assert!(app.mode_picker.is_none());
        assert_eq!(app.config.study_mode, StudyMode::Introductory);
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
        let mut app = App::new(store, "kjv".into(), None).unwrap();
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

    // --- Seleção de texto via mouse ---------------------------------------

    fn mouse(kind: MouseEventKind, col: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind,
            column: col,
            row,
            modifiers: KeyModifiers::empty(),
        }
    }

    /// App com uma área de leitura conhecida (x=10, y=5, 20×8) para a geometria.
    fn app_with_reader() -> App {
        let mut app = seeded_app();
        app.reader_inner = Some(Rect {
            x: 10,
            y: 5,
            width: 20,
            height: 8,
        });
        app
    }

    #[test]
    fn mouse_down_outside_reader_does_not_select() {
        let mut app = app_with_reader();
        app.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 0, 0));
        assert!(app.selection.is_none(), "clique fora não deve selecionar");
    }

    #[test]
    fn mouse_down_inside_starts_selection() {
        let mut app = app_with_reader();
        app.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 12, 6));
        let sel = app.selection.expect("seleção iniciada dentro do leitor");
        assert_eq!(sel.anchor, (12, 6));
        assert_eq!(sel.cursor, (12, 6));
        assert!(sel.dragging);
    }

    #[test]
    fn mouse_drag_is_clamped_to_reader_area() {
        let mut app = app_with_reader();
        app.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 12, 6));
        // Arrasta muito além da área: deve grampear no canto inferior-direito.
        app.handle_mouse(mouse(MouseEventKind::Drag(MouseButton::Left), 200, 200));
        let sel = app.selection.expect("seleção ativa");
        assert_eq!(sel.cursor, (29, 12), "grampeado a (x+w-1, y+h-1)");
    }

    #[test]
    fn plain_click_clears_selection_without_copy() {
        let mut app = app_with_reader();
        app.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 12, 6));
        app.handle_mouse(mouse(MouseEventKind::Up(MouseButton::Left), 12, 6));
        assert!(
            app.selection.is_none(),
            "clique sem arrastar desfaz a seleção"
        );
        assert!(!app.take_copy_request(), "clique simples não pede cópia");
    }

    #[test]
    fn drag_release_requests_copy_and_keeps_highlight() {
        let mut app = app_with_reader();
        app.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 12, 6));
        app.handle_mouse(mouse(MouseEventKind::Up(MouseButton::Left), 20, 6));
        // A seleção permanece fixada (realce visível até a próxima ação) e o
        // cursor segue a posição de soltura (mesmo sem um Drag final).
        let sel = app.selection.expect("seleção fixada");
        assert!(!sel.dragging);
        assert_eq!(sel.cursor, (20, 6), "Up usa a posição de soltura");
        assert!(app.take_copy_request(), "soltar após arrastar pede a cópia");
        // O runtime confirma a cópia → toast.
        app.notify_copied(true, 19);
        assert!(app.toast.as_deref().unwrap().contains("19 caracteres"));
    }

    #[test]
    fn wheel_is_a_noop_in_reader() {
        // A roda do mouse é ignorada na app (rolagem por teclado): não mexe no
        // cursor nem cancela uma seleção em curso.
        let mut app = app_with_reader();
        app.selected = 0;
        app.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 12, 6));
        assert!(app.selection.is_some());
        app.handle_mouse(mouse(MouseEventKind::ScrollDown, 12, 6));
        assert_eq!(app.selected, 0, "a roda não move o cursor");
        assert!(app.selection.is_some(), "a roda não cancela a seleção");
        app.handle_mouse(mouse(MouseEventKind::ScrollUp, 12, 6));
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn wheel_is_a_noop_under_overlay() {
        let mut app = app_with_reader();
        app.selected = 0;
        app.ai = Some(panel_with_answer("resposta"));
        app.handle_mouse(mouse(MouseEventKind::ScrollDown, 5, 5));
        assert_eq!(
            app.ai.as_ref().unwrap().scroll.offset(),
            0,
            "a roda não rola o overlay (use ↑↓)"
        );
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn any_key_cancels_selection() {
        let mut app = app_with_reader();
        app.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 12, 6));
        assert!(app.selection.is_some());
        app.handle_key(key(KeyCode::Down));
        assert!(app.selection.is_none(), "tecla cancela a seleção do mouse");
    }

    #[test]
    fn mouse_ignored_while_overlay_open() {
        let mut app = app_with_reader();
        app.show_help = true;
        app.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 12, 6));
        assert!(app.selection.is_none(), "overlay captura o mouse");
    }

    #[test]
    fn toast_fades_after_ticks() {
        let mut app = app_with_reader();
        app.notify_copied(true, 5);
        assert!(app.toast.is_some());
        for _ in 0..TOAST_TICKS {
            app.tick();
        }
        assert!(app.toast.is_none(), "o toast some após {TOAST_TICKS} ticks");
    }

    // --- IA na TUI ---------------------------------------------------------

    use std::sync::Mutex;
    use the_light_core::ai::{ChatMessage, ChatRole, MockLlmProvider};

    // Serializa os testes que mexem em variáveis de ambiente globais
    // (`LIGHT_SECRETS`/`LIGHT_CONFIG`/`LIGHT_DATA_DIR`) para não interferirem.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn lock_env() -> std::sync::MutexGuard<'static, ()> {
        ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Painel de conversa com uma resposta do assistente (para testes de UI/saltos).
    fn panel_with_answer(answer: &str) -> AiPanel {
        let mut s = Session::start(
            "test-id".into(),
            "t".into(),
            "Romans 3".into(),
            "ctx".into(),
            Lang::En,
            "mock".into(),
            "mock-1".into(),
        );
        s.push(ChatRole::User, "q".into());
        s.push(ChatRole::Assistant, answer.into());
        AiPanel::from_session(s)
    }

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
    fn run_session_query_uses_provider() {
        let p = MockLlmProvider::new("resposta fixa");
        let turns = [ChatMessage {
            role: ChatRole::User,
            content: "oi".into(),
        }];
        let out = run_session_query(&p, Lang::En, "ctx", &turns, None);
        assert_eq!(out.unwrap(), "resposta fixa");
    }

    #[test]
    fn ask_with_mock_creates_session_and_persists() {
        let _g = lock_env();
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("LIGHT_DATA_DIR", dir.path());
        std::env::set_var("LIGHT_SECRETS", dir.path().join("secrets.toml"));

        let mut app = seeded_app();
        app.config.provider = "mock".into();
        app.handle_key(key(KeyCode::Char('a')));
        assert!(matches!(
            app.input.as_ref().map(|i| i.kind),
            Some(InputKind::Ask)
        ));
        type_str(&mut app, "o que e pecado?");
        app.handle_key(key(KeyCode::Enter));

        let panel = app.ai.as_ref().expect("painel de IA");
        assert!(
            matches!(panel.status, AiStatus::Idle),
            "mock responde inline"
        );
        assert_eq!(panel.session.messages.len(), 2, "1 user + 1 assistant");
        assert_eq!(panel.session.messages[0].role, ChatRole::User);
        assert_eq!(panel.session.messages[1].role, ChatRole::Assistant);
        assert!(app.ai_rx.is_none(), "mock não usa thread/canal");
        assert!(app.input.is_none(), "o prompt fecha ao enviar");

        // Conversa persistida e retomável.
        let id = panel.session.id.clone();
        let store = SessionStore::open_default().unwrap();
        assert_eq!(store.get(&id).unwrap().expect("salva").messages.len(), 2);

        std::env::remove_var("LIGHT_DATA_DIR");
        std::env::remove_var("LIGHT_SECRETS");
    }

    #[test]
    fn ask_follow_up_appends_turns() {
        let _g = lock_env();
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("LIGHT_DATA_DIR", dir.path());
        std::env::set_var("LIGHT_SECRETS", dir.path().join("secrets.toml"));

        let mut app = seeded_app();
        app.config.provider = "mock".into();
        app.handle_key(key(KeyCode::Char('a')));
        type_str(&mut app, "primeira");
        app.handle_key(key(KeyCode::Enter));
        assert_eq!(app.ai.as_ref().unwrap().session.messages.len(), 2);
        // `a` (overlay aberto) abre o input para continuar.
        app.handle_key(key(KeyCode::Char('a')));
        assert!(
            matches!(app.input.as_ref().map(|i| i.kind), Some(InputKind::Ask)),
            "a continua a conversa"
        );
        type_str(&mut app, "segunda");
        app.handle_key(key(KeyCode::Enter));
        let s = &app.ai.as_ref().unwrap().session;
        assert_eq!(s.messages.len(), 4, "2 user + 2 assistant");
        assert_eq!(s.messages[2].content, "segunda");

        std::env::remove_var("LIGHT_DATA_DIR");
        std::env::remove_var("LIGHT_SECRETS");
    }

    #[test]
    fn follow_up_typing_goes_to_input_not_overlay() {
        let _g = lock_env();
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("LIGHT_DATA_DIR", dir.path());
        std::env::set_var("LIGHT_SECRETS", dir.path().join("secrets.toml"));

        let mut app = seeded_app();
        app.config.provider = "mock".into();
        let cursor = app.selected;
        app.handle_key(key(KeyCode::Char('a')));
        type_str(&mut app, "primeira");
        app.handle_key(key(KeyCode::Enter));
        // Abre follow-up; digitar 'j' vai para o input, não rola/move o leitor.
        app.handle_key(key(KeyCode::Char('a')));
        app.handle_key(key(KeyCode::Char('j')));
        assert_eq!(app.input.as_ref().unwrap().buffer, "j");
        assert_eq!(app.selected, cursor, "digitação não mexe no leitor");

        std::env::remove_var("LIGHT_DATA_DIR");
        std::env::remove_var("LIGHT_SECRETS");
    }

    #[test]
    fn ask_without_provider_shows_friendly_error() {
        let mut app = seeded_app(); // provider vazio (padrão)
        app.handle_key(key(KeyCode::Char('a')));
        type_str(&mut app, "pergunta");
        app.handle_key(key(KeyCode::Enter));
        match &app.ai.as_ref().unwrap().status {
            AiStatus::Error(msg) => assert!(msg.contains("provedor"), "{msg}"),
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
    fn sessions_browser_reopens_and_deletes() {
        let _g = lock_env();
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("LIGHT_DATA_DIR", dir.path());
        std::env::set_var("LIGHT_SECRETS", dir.path().join("secrets.toml"));

        let mut app = seeded_app();
        app.config.provider = "mock".into();
        app.handle_key(key(KeyCode::Char('a')));
        type_str(&mut app, "sobre a graca");
        app.handle_key(key(KeyCode::Enter));
        let id = app.ai.as_ref().unwrap().session.id.clone();
        app.handle_key(key(KeyCode::Esc)); // fecha (já salvo)
        assert!(app.ai.is_none());

        // `s` abre o navegador e lista a conversa.
        app.handle_key(key(KeyCode::Char('s')));
        assert_eq!(app.sessions.as_ref().unwrap().items.len(), 1);
        // Enter reabre com o histórico.
        app.handle_key(key(KeyCode::Enter));
        assert!(app.sessions.is_none());
        let panel = app.ai.as_ref().expect("conversa reaberta");
        assert_eq!(panel.session.id, id);
        assert_eq!(panel.session.messages.len(), 2);

        // Reabre o navegador e apaga.
        app.handle_key(key(KeyCode::Esc));
        app.handle_key(key(KeyCode::Char('s')));
        app.handle_key(key(KeyCode::Char('d')));
        assert!(
            app.sessions.as_ref().unwrap().items.is_empty(),
            "apagou da lista"
        );
        let store = SessionStore::open_default().unwrap();
        assert!(store.get(&id).unwrap().is_none(), "apagou do disco");

        std::env::remove_var("LIGHT_DATA_DIR");
        std::env::remove_var("LIGHT_SECRETS");
    }

    #[test]
    fn ai_overlay_captures_keys_and_esc_closes() {
        let mut app = seeded_app();
        let before = app.selected;
        app.ai = Some(panel_with_answer("linha"));
        // ↓ rola o overlay; NÃO move o cursor de versículo.
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.selected, before, "o overlay captura a navegação");
        assert_eq!(app.ai.as_ref().unwrap().scroll.offset(), 1);
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
        app.ai = Some(panel_with_answer("Compare com Romans 6:23."));
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
        app.ai = Some(panel_with_answer("Romans 3:24 e Romans 6:23."));
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

    // --- Assistente de estudo (prompt → 3 rodadas → estudo) ------------------

    #[test]
    fn study_wizard_runs_three_rounds_then_produces_study() {
        let _g = lock_env();
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("LIGHT_DATA_DIR", dir.path());
        std::env::set_var("LIGHT_SECRETS", dir.path().join("secrets.toml"));

        let mut app = seeded_app();
        app.config.provider = "mock".into();

        // Inicia no modo Introdutório (sem exigir dados léxicos).
        app.start_study(StudyMode::Introductory, Denomination::Presbyterian);
        assert!(
            matches!(
                app.input.as_ref().map(|i| i.kind),
                Some(InputKind::StudyBrief)
            ),
            "abre o campo de assunto"
        );
        assert!(
            app.ai.as_ref().unwrap().study.is_some(),
            "há assistente ativo"
        );

        // Envia o assunto → dispara a rodada 1 (mock responde inline).
        type_str(&mut app, "a graça");
        app.handle_key(key(KeyCode::Enter));
        assert!(app.input.is_none(), "campo de assunto fechou");

        // Três rodadas de refinamento, escolhendo sempre a opção destacada.
        for round in 1..=3u8 {
            let flow = app.ai.as_ref().unwrap().study.as_ref().unwrap();
            assert_eq!(flow.round, round, "rodada {round}");
            assert!(!flow.options.is_empty(), "rodada {round} tem opções");
            app.handle_key(key(KeyCode::Enter)); // escolhe a opção destacada
        }

        // Após a 3ª resposta, o estudo final é produzido e vira sessão normal.
        let panel = app.ai.as_ref().unwrap();
        assert!(panel.study.is_none(), "assistente concluído");
        assert_eq!(panel.session.study_mode, Some(StudyMode::Introductory));
        let last = panel.session.messages.last().unwrap();
        assert_eq!(last.role, ChatRole::Assistant);
        assert!(
            last.content.contains("# Estudo —"),
            "estudo renderizado: {}",
            last.content
        );

        // Persistido como sessão retomável.
        let id = panel.session.id.clone();
        let store = SessionStore::open_default().unwrap();
        assert!(store.get(&id).unwrap().is_some(), "estudo salvo");

        std::env::remove_var("LIGHT_DATA_DIR");
        std::env::remove_var("LIGHT_SECRETS");
    }

    #[test]
    fn study_wizard_requires_provider() {
        let mut app = seeded_app(); // provider vazio (padrão)
        app.start_study(StudyMode::Introductory, Denomination::Presbyterian);
        assert!(app.ai.is_none(), "sem provedor, não abre o assistente");
        assert!(app.input.is_none());
        assert!(
            app.toast
                .as_deref()
                .unwrap_or_default()
                .contains("provedor"),
            "avisa que falta provedor"
        );
    }

    #[test]
    fn mode_picker_enter_starts_study_s_sets_default() {
        let _g = lock_env();
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("LIGHT_CONFIG", dir.path().join("config.toml"));

        let mut app = seeded_app();
        app.config.provider = "mock".into();

        // `s` apenas define o padrão (sem iniciar estudo).
        app.config.study_mode = StudyMode::Academic;
        app.handle_key(key(KeyCode::Char('m')));
        app.handle_key(key(KeyCode::Down)); // Academic → Devotional
        app.handle_key(key(KeyCode::Char('s')));
        assert!(app.mode_picker.is_none());
        assert_eq!(app.config.study_mode, StudyMode::Devotional);
        assert!(app.ai.is_none(), "`s` não inicia estudo");

        // Enter inicia o estudo no modo destacado (Introdutório, sem léxico).
        app.handle_key(key(KeyCode::Char('m')));
        // Posicionado em Devotional (padrão atual); desce até Introdutório.
        app.handle_key(key(KeyCode::Down));
        app.handle_key(key(KeyCode::Enter));
        assert!(app.mode_picker.is_none());
        assert!(
            matches!(
                app.input.as_ref().map(|i| i.kind),
                Some(InputKind::StudyBrief)
            ),
            "Enter abre o assistente de estudo"
        );

        std::env::remove_var("LIGHT_CONFIG");
    }

    #[test]
    fn study_custom_answer_records_and_advances() {
        let _g = lock_env();
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("LIGHT_DATA_DIR", dir.path());
        std::env::set_var("LIGHT_SECRETS", dir.path().join("secrets.toml"));

        let mut app = seeded_app();
        app.config.provider = "mock".into();
        app.start_study(StudyMode::Introductory, Denomination::Presbyterian);
        type_str(&mut app, "a graça");
        app.handle_key(key(KeyCode::Enter)); // rodada 1

        // `c` abre o campo de resposta própria.
        app.handle_key(key(KeyCode::Char('c')));
        assert!(
            matches!(
                app.input.as_ref().map(|i| i.kind),
                Some(InputKind::StudyCustom)
            ),
            "abre resposta própria"
        );
        type_str(&mut app, "foco em Romanos 3");
        app.handle_key(key(KeyCode::Enter));
        let flow = app.ai.as_ref().unwrap().study.as_ref().unwrap();
        assert_eq!(flow.round, 2, "avançou para a rodada 2");
        assert_eq!(
            flow.prior[0].1, "foco em Romanos 3",
            "registrou a resposta própria"
        );

        std::env::remove_var("LIGHT_DATA_DIR");
        std::env::remove_var("LIGHT_SECRETS");
    }

    #[test]
    fn study_export_writes_markdown_and_lens_cycles() {
        let _g = lock_env();
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("LIGHT_DATA_DIR", dir.path());
        std::env::set_var("LIGHT_SECRETS", dir.path().join("secrets.toml"));
        std::env::set_var("LIGHT_CONFIG", dir.path().join("config.toml"));

        let mut app = seeded_app();
        app.config.provider = "mock".into();
        app.start_study(StudyMode::Introductory, Denomination::Presbyterian);
        type_str(&mut app, "a graça");
        app.handle_key(key(KeyCode::Enter));
        for _ in 0..3 {
            app.handle_key(key(KeyCode::Enter));
        }
        assert!(app.ai.as_ref().unwrap().study.is_none(), "estudo pronto");

        // `e` exporta o estudo para studies/*.md.
        app.handle_key(key(KeyCode::Char('e')));
        let studies = the_light_core::userdata::studies_dir().unwrap();
        let mds: Vec<_> = std::fs::read_dir(&studies)
            .unwrap()
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("md"))
            .collect();
        assert_eq!(mds.len(), 1, "um estudo exportado");

        // `L` cicla a lente denominacional do config.
        let before = app.config.study_lens;
        app.handle_key(key(KeyCode::Char('L')));
        assert_ne!(app.config.study_lens, before, "lente mudou");

        std::env::remove_var("LIGHT_DATA_DIR");
        std::env::remove_var("LIGHT_SECRETS");
        std::env::remove_var("LIGHT_CONFIG");
    }

    // --- Mouse: navegação clicável (Fase C) -------------------------------

    fn left_click(app: &mut App, col: u16, row: u16) {
        app.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), col, row));
    }

    #[test]
    fn click_book_row_loads_book() {
        let mut app = seeded_app();
        app.click_targets = vec![(
            Rect {
                x: 0,
                y: 3,
                width: 18,
                height: 1,
            },
            ClickTarget::Book(5),
        )];
        left_click(&mut app, 4, 3);
        assert_eq!(app.book_idx, 5, "clicar a linha do livro o carrega");
        assert_eq!(app.focus, Focus::Books);
    }

    #[test]
    fn click_verse_moves_cursor() {
        let mut app = app_with_reader();
        app.selected = 0;
        // Linha do versículo de índice 1, dentro da área do leitor.
        app.click_targets = vec![(
            Rect {
                x: 10,
                y: 7,
                width: 20,
                height: 1,
            },
            ClickTarget::Verse(1),
        )];
        app.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 12, 7));
        app.handle_mouse(mouse(MouseEventKind::Up(MouseButton::Left), 12, 7));
        assert_eq!(app.selected, 1, "clique simples move o cursor ao versículo");
        assert_eq!(app.focus, Focus::Reader);
    }

    #[test]
    fn click_under_overlay_ignores_base_targets() {
        let mut app = seeded_app();
        let before = app.book_idx;
        app.open_mode_picker();
        // Um alvo de livro por baixo do modal NÃO deve ser ativado.
        app.click_targets = vec![(
            Rect {
                x: 0,
                y: 3,
                width: 18,
                height: 1,
            },
            ClickTarget::Book(5),
        )];
        left_click(&mut app, 4, 3);
        assert_eq!(
            app.book_idx, before,
            "clique sob o modal não seleciona livro"
        );
        assert!(app.mode_picker.is_some(), "o modal segue aberto");
    }

    #[test]
    fn mode_picker_click_selects_then_starts_study() {
        let mut app = seeded_app();
        app.config.provider = "mock".into();
        app.open_mode_picker(); // pré-seleciona o padrão (Introdutório, índice 2)
        let rect = Rect {
            x: 0,
            y: 4,
            width: 40,
            height: 2,
        };
        // Clica o modo Devocional (índice 1, ainda não selecionado, sem léxico).
        app.click_targets = vec![(rect, ClickTarget::ModeRow(1))];
        left_click(&mut app, 2, 4);
        assert_eq!(app.mode_picker.as_ref().unwrap().selected, 1);
        assert!(app.ai.is_none(), "1º clique apenas seleciona a linha");
        left_click(&mut app, 2, 4);
        assert!(
            matches!(
                app.input.as_ref().map(|i| i.kind),
                Some(InputKind::StudyBrief)
            ),
            "2º clique (linha já selecionada) inicia o assistente"
        );
    }

    #[test]
    fn wheel_is_a_noop_in_mode_picker() {
        let mut app = seeded_app();
        app.open_mode_picker();
        app.mode_picker.as_mut().unwrap().selected = 1;
        app.handle_mouse(mouse(MouseEventKind::ScrollDown, 1, 1));
        assert_eq!(
            app.mode_picker.as_ref().unwrap().selected,
            1,
            "a roda não move a seleção (use ↑↓)"
        );
    }

    #[test]
    fn click_study_option_advances_round() {
        let mut app = seeded_app();
        app.config.provider = "mock".into();
        app.start_study(StudyMode::Introductory, Denomination::Presbyterian);
        type_str(&mut app, "a graça");
        app.handle_key(key(KeyCode::Enter)); // rodada 1, com opções do mock
        assert_eq!(app.ai.as_ref().unwrap().study.as_ref().unwrap().round, 1);

        let rect = Rect {
            x: 0,
            y: 10,
            width: 40,
            height: 1,
        };
        app.click_targets = vec![(rect, ClickTarget::StudyOption(1))];
        left_click(&mut app, 2, 10);
        let flow = app.ai.as_ref().unwrap().study.as_ref().unwrap();
        assert_eq!(flow.round, 2, "clicar uma opção avança a rodada");
    }

    // --- Fase D: lente no preparo + aprofundar -----------------------------

    #[test]
    fn study_setup_cycles_lens_then_starts_with_it() {
        let _g = lock_env();
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("LIGHT_DATA_DIR", dir.path());
        std::env::set_var("LIGHT_SECRETS", dir.path().join("secrets.toml"));
        std::env::set_var("LIGHT_CONFIG", dir.path().join("config.toml"));

        let mut app = seeded_app();
        app.config.provider = "mock".into();
        app.config.study_mode = StudyMode::Introductory; // sem dados léxicos
        app.config.study_lens = Denomination::Presbyterian;

        app.open_mode_picker();
        assert_eq!(
            app.mode_picker.as_ref().unwrap().lens,
            Denomination::Presbyterian
        );
        app.handle_key(key(KeyCode::Right)); // cicla a lente
        let chosen = app.mode_picker.as_ref().unwrap().lens;
        assert_ne!(chosen, Denomination::Presbyterian, "←/→ muda a lente");

        app.handle_key(key(KeyCode::Enter)); // inicia com modo + lente
        assert!(matches!(
            app.input.as_ref().map(|i| i.kind),
            Some(InputKind::StudyBrief)
        ));
        let panel = app.ai.as_ref().unwrap();
        assert_eq!(
            panel.study.as_ref().unwrap().lens,
            chosen,
            "lente vai p/ o fluxo"
        );
        assert_eq!(
            panel.session.study_lens.as_deref(),
            Some(chosen.slug()),
            "lente gravada na sessão"
        );
        assert_eq!(app.config.study_lens, chosen, "lente vira o novo padrão");

        std::env::remove_var("LIGHT_DATA_DIR");
        std::env::remove_var("LIGHT_SECRETS");
        std::env::remove_var("LIGHT_CONFIG");
    }

    #[test]
    fn study_setup_lens_click_cycles() {
        let mut app = seeded_app();
        app.open_mode_picker();
        let before = app.mode_picker.as_ref().unwrap().lens;
        app.click_targets = vec![(
            Rect {
                x: 0,
                y: 3,
                width: 40,
                height: 1,
            },
            ClickTarget::LensCycle,
        )];
        left_click(&mut app, 5, 3);
        assert_ne!(
            app.mode_picker.as_ref().unwrap().lens,
            before,
            "clicar a lente cicla"
        );
    }

    /// Roda o assistente de estudo do início ao fim (modo Introdutório, mock).
    fn complete_a_study(app: &mut App) {
        app.config.provider = "mock".into();
        app.start_study(StudyMode::Introductory, Denomination::Presbyterian);
        type_str(app, "a graça");
        app.handle_key(key(KeyCode::Enter)); // rodada 1
        for _ in 0..3 {
            app.handle_key(key(KeyCode::Enter)); // escolhe a opção destacada
        }
    }

    #[test]
    fn deepen_reruns_and_retains_context_deeper() {
        let _g = lock_env();
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("LIGHT_DATA_DIR", dir.path());
        std::env::set_var("LIGHT_SECRETS", dir.path().join("secrets.toml"));

        let mut app = seeded_app();
        complete_a_study(&mut app);
        let panel = app.ai.as_ref().unwrap();
        assert!(panel.study.is_none(), "estudo concluído");
        let ctx = panel.study_done.as_ref().expect("contexto retido");
        assert_eq!(ctx.mode, StudyMode::Introductory);
        let depth0 = ctx.depth;
        let turns_before = panel.session.messages.len();

        // `+` abre o foco do aprofundamento; Enter (vazio) re-roda mais fundo.
        app.handle_key(key(KeyCode::Char('+')));
        assert!(matches!(
            app.input.as_ref().map(|i| i.kind),
            Some(InputKind::StudyDeepen)
        ));
        app.handle_key(key(KeyCode::Enter));

        let panel = app.ai.as_ref().unwrap();
        // Pedido do usuário + nova seção do assistente foram anexados.
        assert!(
            panel.session.messages.len() >= turns_before + 2,
            "aprofundamento anexa turnos (era {turns_before}, agora {})",
            panel.session.messages.len()
        );
        let ctx2 = panel.study_done.as_ref().expect("novo contexto");
        assert_eq!(ctx2.depth, depth0.deeper(), "profundidade avança");
        assert_eq!(ctx2.mode, StudyMode::Introductory, "mesmo modo/lente");

        std::env::remove_var("LIGHT_DATA_DIR");
        std::env::remove_var("LIGHT_SECRETS");
    }

    #[test]
    fn followup_on_study_is_study_aware() {
        let _g = lock_env();
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("LIGHT_DATA_DIR", dir.path());
        std::env::set_var("LIGHT_SECRETS", dir.path().join("secrets.toml"));

        let mut app = seeded_app();
        complete_a_study(&mut app);
        // O follow-up usa o caminho normal de conversa, mas a sessão segue marcada
        // como estudo (mode/lens preservados) e o contexto foi sementeado.
        let panel = app.ai.as_ref().unwrap();
        assert_eq!(panel.session.study_mode, Some(StudyMode::Introductory));
        assert!(
            !panel.session.context.trim().is_empty(),
            "contexto sementeado para o follow-up"
        );
        assert!(panel.study_done.is_some());

        app.handle_key(key(KeyCode::Char('a')));
        type_str(&mut app, "e quanto à fé?");
        app.handle_key(key(KeyCode::Enter));
        let panel = app.ai.as_ref().unwrap();
        assert_eq!(
            panel.session.study_mode,
            Some(StudyMode::Introductory),
            "follow-up preserva o estudo"
        );

        std::env::remove_var("LIGHT_DATA_DIR");
        std::env::remove_var("LIGHT_SECRETS");
    }
}
