//! Estado da TUI e tratamento de teclas (lógica pura, testável).

use anyhow::{anyhow, Result};
use ratatui::crossterm::event::{KeyCode, KeyEvent};

use biblia_core::model::{Lang, Reference, TranslationId};
use biblia_core::reference::{parse_reference, BOOKS};
use biblia_core::source::{BibleSource, EmbeddedSource};
use biblia_core::store::Store;
use biblia_core::userdata::{Highlight, HighlightStore, Note, NoteStore};
use biblia_core::xref::{self, CrossRef};

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
    /// Marcações do usuário (carregadas do disco).
    pub highlights: Vec<Highlight>,
    /// Notas do usuário (carregadas do disco).
    pub notes: Vec<Note>,
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
            highlights: Vec::new(),
            notes: Vec::new(),
            should_quit: false,
        };
        app.reload()?;
        Ok(app)
    }

    /// Carrega marcações e notas do disco (tolerante a erros). Chamado pela
    /// `run`; os testes injetam os dados diretamente.
    pub fn load_userdata(&mut self) {
        self.highlights = HighlightStore::load_default()
            .map(|s| s.list().to_vec())
            .unwrap_or_default();
        self.notes = NoteStore::open_default()
            .and_then(|s| s.list())
            .unwrap_or_default();
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

    /// Nome do livro selecionado no idioma de exibição.
    pub fn book_name(&self) -> &'static str {
        let b = &BOOKS[self.book_idx];
        match self.lang() {
            Lang::Pt => b.name_pt,
            Lang::En => b.name_en,
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

    fn reload(&mut self) -> Result<()> {
        let book = self.book_number();
        let version = self.version().clone();
        let (count, chap, verses) = {
            let src = EmbeddedSource::new(&self.store);
            let count = src.chapter_count(book, &version)?;
            let chap = self.chapter.clamp(1, count.max(1));
            let passage = src.passage(&Reference::whole_chapter(book, chap), &version)?;
            let verses: Vec<(u16, String)> = passage
                .verses
                .into_iter()
                .map(|v| (v.reference.verses.start().unwrap_or(0), v.text))
                .collect();
            (count, chap, verses)
        };
        self.chapter = chap;
        self.chapter_count = count;
        self.verses = verses;
        self.selected = 0;
        Ok(())
    }

    /// Seleciona um livro pelo índice (0..66) e carrega seu capítulo 1.
    pub fn select_book(&mut self, idx: usize) {
        if idx >= BOOKS.len() {
            return;
        }
        self.book_idx = idx;
        self.chapter = 1;
        let _ = self.reload();
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
            self.chapter += 1;
            let _ = self.reload();
        } else if self.book_idx + 1 < BOOKS.len() {
            self.select_book(self.book_idx + 1);
        }
    }

    /// Capítulo anterior (ou último do livro anterior).
    pub fn prev_chapter(&mut self) {
        if self.chapter > 1 {
            self.chapter -= 1;
            let _ = self.reload();
        } else if self.book_idx > 0 {
            self.book_idx -= 1;
            self.chapter = u16::MAX;
            let _ = self.reload();
        }
    }

    /// Alterna para a próxima versão, mantendo livro/capítulo (recarrega texto).
    pub fn cycle_version(&mut self) {
        if self.versions.len() < 2 {
            return;
        }
        let keep = self.selected;
        self.version_idx = (self.version_idx + 1) % self.versions.len();
        let _ = self.reload();
        // Mantém o versículo selecionado, se ainda existir.
        self.selected = keep.min(self.verses.len().saturating_sub(1));
    }

    /// Salta para uma referência (livro/capítulo/versículo), focando o leitor.
    pub fn go_to(&mut self, reference: &Reference) {
        self.book_idx = reference.book.saturating_sub(1) as usize;
        self.chapter = reference.chapter;
        let _ = self.reload();
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

    /// Processa uma tecla pressionada.
    pub fn handle_key(&mut self, key: KeyEvent) {
        if self.input.is_some() {
            self.handle_input_key(key);
            return;
        }
        if self.xref_nav.is_some() {
            self.handle_xref_key(key);
            return;
        }
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            KeyCode::Tab => {
                self.focus = match self.focus {
                    Focus::Books => Focus::Reader,
                    Focus::Reader => Focus::Books,
                };
            }
            KeyCode::Char('v') => self.cycle_version(),
            KeyCode::Char('x') => self.open_xrefs(),
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
        match key.code {
            KeyCode::Esc => self.input = None,
            KeyCode::Backspace => {
                input.buffer.pop();
                input.error = None;
            }
            KeyCode::Char(c) => {
                input.buffer.push(c);
                input.error = None;
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
}
