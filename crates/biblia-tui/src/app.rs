//! Estado da TUI e tratamento de teclas (lógica pura, testável).

use anyhow::Result;
use ratatui::crossterm::event::{KeyCode, KeyEvent};

use biblia_core::model::{Lang, Reference, TranslationId};
use biblia_core::reference::BOOKS;
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

/// Estado da aplicação TUI.
pub struct App {
    store: Store,
    /// Versão ativa.
    pub version: TranslationId,
    /// Rótulo da versão (ex.: "KJV").
    pub version_label: String,
    /// Idioma de exibição.
    pub lang: Lang,
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
    /// Sai do loop quando `true`.
    pub should_quit: bool,
}

impl App {
    /// Cria a app e carrega o primeiro capítulo de Gênesis.
    pub fn new(
        store: Store,
        version: TranslationId,
        version_label: String,
        lang: Lang,
    ) -> Result<Self> {
        let mut app = App {
            store,
            version,
            version_label,
            lang,
            book_idx: 0,
            chapter: 1,
            chapter_count: 0,
            verses: Vec::new(),
            focus: Focus::Books,
            scroll: 0,
            should_quit: false,
        };
        app.reload()?;
        Ok(app)
    }

    /// Número canônico do livro selecionado (1..=66).
    pub fn book_number(&self) -> u8 {
        (self.book_idx + 1) as u8
    }

    /// Nome do livro selecionado no idioma de exibição.
    pub fn book_name(&self) -> &'static str {
        let b = &BOOKS[self.book_idx];
        match self.lang {
            Lang::Pt => b.name_pt,
            Lang::En => b.name_en,
        }
    }

    /// Recarrega capítulo/versículos do banco para o livro/capítulo atuais.
    fn reload(&mut self) -> Result<()> {
        let book = self.book_number();
        let (count, chap, verses) = {
            let src = EmbeddedSource::new(&self.store);
            let count = src.chapter_count(book, &self.version)?;
            let chap = self.chapter.clamp(1, count.max(1));
            let passage = src.passage(&Reference::whole_chapter(book, chap), &self.version)?;
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

    /// Vai para o próximo capítulo (ou primeiro do próximo livro).
    pub fn next_chapter(&mut self) {
        if self.chapter < self.chapter_count {
            self.chapter += 1;
            let _ = self.reload();
        } else if self.book_idx + 1 < BOOKS.len() {
            self.select_book(self.book_idx + 1);
        }
    }

    /// Vai para o capítulo anterior (ou último do livro anterior).
    pub fn prev_chapter(&mut self) {
        if self.chapter > 1 {
            self.chapter -= 1;
            let _ = self.reload();
        } else if self.book_idx > 0 {
            self.book_idx -= 1;
            // Vai para o último capítulo do livro anterior.
            self.chapter = u16::MAX;
            let _ = self.reload();
        }
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
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            KeyCode::Tab => {
                self.focus = match self.focus {
                    Focus::Books => Focus::Reader,
                    Focus::Reader => Focus::Books,
                };
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::crossterm::event::KeyModifiers;
    use rusqlite::params;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::empty())
    }

    fn seeded_app() -> App {
        let store = Store::open_in_memory().unwrap();
        {
            let conn = store.conn();
            conn.execute(
                "INSERT INTO translations(id,abbrev,name,language,license,embeddable) \
                 VALUES ('kjv','KJV','King James Version','en','public-domain',1)",
                [],
            )
            .unwrap();
            let rows = [
                (1, 1, 1, "In the beginning God created"),
                (1, 1, 2, "And the earth was without form"),
                (1, 2, 1, "Thus the heavens were finished"),
                (43, 3, 16, "For God so loved the world"),
            ];
            for (b, c, v, t) in rows {
                conn.execute(
                    "INSERT INTO verses(translation_id,book_number,chapter,verse,text) \
                     VALUES ('kjv',?1,?2,?3,?4)",
                    params![b, c, v, t],
                )
                .unwrap();
            }
        }
        App::new(store, "kjv".into(), "KJV".to_string(), Lang::En).unwrap()
    }

    #[test]
    fn starts_on_genesis_chapter_one() {
        let app = seeded_app();
        assert_eq!(app.book_number(), 1);
        assert_eq!(app.book_name(), "Genesis");
        assert_eq!(app.chapter, 1);
        assert_eq!(app.chapter_count, 2);
        assert_eq!(app.verses.len(), 2);
    }

    #[test]
    fn next_and_prev_chapter() {
        let mut app = seeded_app();
        app.next_chapter();
        assert_eq!(app.chapter, 2);
        assert_eq!(
            app.verses,
            vec![(1, "Thus the heavens were finished".to_string())]
        );
        app.prev_chapter();
        assert_eq!(app.chapter, 1);
        assert_eq!(app.verses.len(), 2);
    }

    #[test]
    fn select_book_clamps_chapter_and_loads() {
        let mut app = seeded_app();
        app.select_book(42); // John (43)
        assert_eq!(app.book_number(), 43);
        assert_eq!(app.book_name(), "John");
        assert_eq!(app.chapter, 1); // capítulo 1 não tem texto semeado
        assert_eq!(app.chapter_count, 3); // max chapter semeado = 3
        assert!(app.verses.is_empty());
        // Avança até o capítulo 3 (com texto).
        app.next_chapter();
        app.next_chapter();
        assert_eq!(app.chapter, 3);
        assert_eq!(
            app.verses,
            vec![(16, "For God so loved the world".to_string())]
        );
    }

    #[test]
    fn keys_navigate_books_and_quit() {
        let mut app = seeded_app();
        assert_eq!(app.focus, Focus::Books);
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.book_number(), 2); // Exodus
        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.book_number(), 1);
        // Enter foca o leitor; setas então rolam.
        app.handle_key(key(KeyCode::Enter));
        assert_eq!(app.focus, Focus::Reader);
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.scroll, 1);
        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.scroll, 0);
        // 'q' encerra.
        app.handle_key(key(KeyCode::Char('q')));
        assert!(app.should_quit);
    }

    #[test]
    fn move_book_saturates_at_edges() {
        let mut app = seeded_app();
        app.move_book(-1); // já no primeiro
        assert_eq!(app.book_idx, 0);
        app.select_book(65); // Apocalipse
        app.move_book(1);
        assert_eq!(app.book_idx, 65);
    }
}
