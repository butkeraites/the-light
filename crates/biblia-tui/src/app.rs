//! Estado da TUI e tratamento de teclas (lógica pura, testável).

use anyhow::{anyhow, Result};
use ratatui::crossterm::event::{KeyCode, KeyEvent};

use biblia_core::model::{Lang, Reference, TranslationId};
use biblia_core::reference::{parse_reference, BOOKS};
use biblia_core::source::{BibleSource, EmbeddedSource};
use biblia_core::store::Store;

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
    /// Painel focado.
    pub focus: Focus,
    /// Deslocamento vertical do viewport (em linhas).
    pub scroll: u16,
    /// Entrada de texto ativa (prompt), se houver.
    pub input: Option<Input>,
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
            focus: Focus::Books,
            scroll: 0,
            input: None,
            should_quit: false,
        };
        app.reload()?;
        Ok(app)
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

    /// Recarrega capítulo/versículos do banco para o livro/capítulo atuais.
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
        self.scroll = 0;
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
        self.version_idx = (self.version_idx + 1) % self.versions.len();
        let _ = self.reload();
    }

    /// Salta para uma referência (livro/capítulo), focando o leitor.
    pub fn go_to(&mut self, reference: &Reference) {
        self.book_idx = (reference.book.saturating_sub(1)) as usize;
        self.chapter = reference.chapter;
        let _ = self.reload();
        self.focus = Focus::Reader;
    }

    /// Rola o viewport para baixo `n` linhas.
    pub fn scroll_down(&mut self, n: u16) {
        self.scroll = self.scroll.saturating_add(n);
    }

    /// Rola o viewport para cima `n` linhas.
    pub fn scroll_up(&mut self, n: u16) {
        self.scroll = self.scroll.saturating_sub(n);
    }

    /// Processa uma tecla pressionada.
    pub fn handle_key(&mut self, key: KeyEvent) {
        if self.input.is_some() {
            self.handle_input_key(key);
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
            KeyCode::Down | KeyCode::Char('j') => self.scroll_down(1),
            KeyCode::Up | KeyCode::Char('k') => self.scroll_up(1),
            KeyCode::PageDown | KeyCode::Char(' ') => self.scroll_down(10),
            KeyCode::PageUp => self.scroll_up(10),
            KeyCode::Char('n') | KeyCode::Right | KeyCode::Char('l') => self.next_chapter(),
            KeyCode::Char('p') | KeyCode::Char('h') => self.prev_chapter(),
            KeyCode::Left => self.focus = Focus::Books,
            KeyCode::Home => self.scroll = 0,
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
                ("kjv", 1, 1, 1, "In the beginning God created"),
                ("kjv", 1, 1, 2, "And the earth was without form"),
                ("kjv", 1, 2, 1, "Thus the heavens were finished"),
                ("kjv", 43, 3, 16, "For God so loved the world"),
                ("alm", 1, 1, 1, "No princípio criou Deus"),
                ("alm", 43, 3, 16, "Porque Deus amou o mundo"),
            ];
            for (t, b, c, v, txt) in rows {
                conn.execute(
                    "INSERT INTO verses(translation_id,book_number,chapter,verse,text) \
                     VALUES (?1,?2,?3,?4,?5)",
                    params![t, b, c, v, txt],
                )
                .unwrap();
            }
        }
        App::new(store, "kjv".into()).unwrap()
    }

    #[test]
    fn starts_on_genesis_chapter_one_in_kjv() {
        let app = seeded_app();
        assert_eq!(app.version_label(), "KJV");
        assert_eq!(app.lang(), Lang::En);
        assert_eq!(app.book_name(), "Genesis");
        assert_eq!(app.verses.len(), 2);
    }

    #[test]
    fn cycle_version_keeps_passage() {
        let mut app = seeded_app();
        app.go_to(&parse_reference("John 3:16").unwrap());
        assert_eq!(app.book_number(), 43);
        assert_eq!(app.chapter, 3);
        app.handle_key(key(KeyCode::Char('v'))); // → alm
        assert_eq!(app.version_label(), "ALM");
        assert_eq!(app.lang(), Lang::Pt);
        // Mesma passagem, texto em português.
        assert_eq!(app.book_number(), 43);
        assert_eq!(app.chapter, 3);
        assert_eq!(
            app.verses,
            vec![(16, "Porque Deus amou o mundo".to_string())]
        );
        assert_eq!(app.book_name(), "João");
        app.handle_key(key(KeyCode::Char('v'))); // volta a kjv
        assert_eq!(app.version_label(), "KJV");
    }

    #[test]
    fn go_to_via_input_prompt() {
        let mut app = seeded_app();
        app.handle_key(key(KeyCode::Char('g')));
        assert!(app.input.is_some());
        type_str(&mut app, "John 3");
        app.handle_key(key(KeyCode::Enter));
        assert!(app.input.is_none());
        assert_eq!(app.book_number(), 43);
        assert_eq!(app.chapter, 3);
        assert_eq!(app.focus, Focus::Reader);
    }

    #[test]
    fn go_to_invalid_keeps_prompt_with_error() {
        let mut app = seeded_app();
        app.handle_key(key(KeyCode::Char('g')));
        type_str(&mut app, "Xyz 9");
        app.handle_key(key(KeyCode::Enter));
        let input = app.input.as_ref().expect("prompt continua aberto");
        assert!(input.error.is_some());
        assert_eq!(input.buffer, "Xyz 9");
        // Esc cancela.
        app.handle_key(key(KeyCode::Esc));
        assert!(app.input.is_none());
    }

    #[test]
    fn input_backspace_edits_buffer() {
        let mut app = seeded_app();
        app.handle_key(key(KeyCode::Char('g')));
        type_str(&mut app, "Jox");
        app.handle_key(key(KeyCode::Backspace));
        app.handle_key(key(KeyCode::Char(' ')));
        type_str(&mut app, "3.16");
        app.handle_key(key(KeyCode::Enter));
        assert_eq!(app.book_number(), 43);
        assert_eq!(app.chapter, 3);
    }

    #[test]
    fn quit_and_chapter_nav_still_work() {
        let mut app = seeded_app();
        app.next_chapter();
        assert_eq!(app.chapter, 2);
        app.handle_key(key(KeyCode::Char('q')));
        assert!(app.should_quit);
    }
}
