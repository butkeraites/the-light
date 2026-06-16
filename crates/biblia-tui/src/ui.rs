//! Renderização da TUI (widgets ratatui). Recebe `&mut App` para fixar a
//! rolagem dentro dos limites do conteúdo.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Frame;

use biblia_core::model::Lang;
use biblia_core::reference::BOOKS;

use crate::app::{App, Focus, InputKind};

const ACCENT: Color = Color::Cyan;

/// Desenha a interface completa.
pub fn draw(frame: &mut Frame, app: &mut App) {
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .split(frame.area());

    let title = format!(" Bíblia CLI — TUI    [{}]", app.version_label());
    frame.render_widget(
        Paragraph::new(title).style(Style::new().add_modifier(Modifier::BOLD)),
        rows[0],
    );

    let body = Layout::horizontal([Constraint::Length(24), Constraint::Min(10)]).split(rows[1]);
    draw_books(frame, app, body[0]);
    draw_reader(frame, app, body[1]);
    draw_status(frame, app, rows[2]);
}

fn border_style(focused: bool) -> Style {
    if focused {
        Style::new().fg(ACCENT)
    } else {
        Style::new().fg(Color::DarkGray)
    }
}

fn draw_books(frame: &mut Frame, app: &App, area: Rect) {
    let items: Vec<ListItem> = BOOKS
        .iter()
        .map(|b| {
            let name = match app.lang() {
                Lang::Pt => b.name_pt,
                Lang::En => b.name_en,
            };
            ListItem::new(name)
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::bordered()
                .title("Livros")
                .border_style(border_style(app.focus == Focus::Books)),
        )
        .highlight_style(
            Style::new()
                .fg(Color::Black)
                .bg(ACCENT)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("› ");

    let mut state = ListState::default();
    state.select(Some(app.book_idx));
    frame.render_stateful_widget(list, area, &mut state);
}

fn draw_reader(frame: &mut Frame, app: &mut App, area: Rect) {
    let title = format!(
        "{} {}  ({})",
        app.book_name(),
        app.chapter,
        app.version_label()
    );
    let block = Block::bordered()
        .title(title)
        .border_style(border_style(app.focus == Focus::Reader));
    let inner = block.inner(area);

    let lines: Vec<Line> = if app.verses.is_empty() {
        vec![Line::from(Span::styled(
            "(sem texto neste capítulo)",
            Style::new().add_modifier(Modifier::DIM),
        ))]
    } else {
        app.verses
            .iter()
            .map(|(n, t)| {
                Line::from(vec![
                    Span::styled(format!("{n:>3} "), Style::new().fg(ACCENT)),
                    Span::raw(t.as_str()),
                ])
            })
            .collect()
    };

    let paragraph = Paragraph::new(Text::from(lines)).wrap(Wrap { trim: false });
    let total = paragraph.line_count(inner.width) as u16;
    let max_scroll = total.saturating_sub(inner.height);
    app.scroll = app.scroll.min(max_scroll);

    frame.render_widget(paragraph.scroll((app.scroll, 0)).block(block), area);
}

/// Barra inferior: prompt de entrada (se ativo) ou ajuda de teclas.
fn draw_status(frame: &mut Frame, app: &App, area: Rect) {
    let widget = match &app.input {
        Some(input) => {
            let label = match input.kind {
                InputKind::GoTo => "Ir para",
            };
            let text = match &input.error {
                Some(e) => format!(" {label}: {}    ⚠ {e}", input.buffer),
                None => format!(" {label}: {}", input.buffer),
            };
            Paragraph::new(text).style(Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD))
        }
        None => Paragraph::new(
            " q sair · ↑↓ navegar · Enter ler · n/p capítulo · v versão · g ir · Tab foco",
        )
        .style(Style::new().add_modifier(Modifier::DIM)),
    };
    frame.render_widget(widget, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use biblia_core::store::Store;
    use ratatui::backend::TestBackend;
    use ratatui::buffer::Buffer;
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::Terminal;
    use rusqlite::params;

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
            conn.execute(
                "INSERT INTO verses(translation_id,book_number,chapter,verse,text) \
                 VALUES ('kjv',1,1,1,'In the beginning God created the heaven and the earth')",
                params![],
            )
            .unwrap();
        }
        App::new(store, "kjv".into()).unwrap()
    }

    fn to_text(buf: &Buffer) -> String {
        let mut s = String::new();
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                if let Some(cell) = buf.cell((x, y)) {
                    s.push_str(cell.symbol());
                }
            }
            s.push('\n');
        }
        s
    }

    fn render(app: &mut App) -> String {
        let mut terminal = Terminal::new(TestBackend::new(80, 20)).unwrap();
        terminal.draw(|f| draw(f, app)).unwrap();
        to_text(terminal.backend().buffer())
    }

    #[test]
    fn renders_books_title_version_and_chapter_text() {
        let mut app = seeded_app();
        let text = render(&mut app);
        assert!(text.contains("Bíblia CLI — TUI"));
        assert!(text.contains("[KJV]"));
        assert!(text.contains("Livros"));
        assert!(text.contains("Genesis"));
        assert!(text.contains("In the beginning"));
        assert!(text.contains("g ir"));
    }

    #[test]
    fn shows_goto_prompt_when_active() {
        let mut app = seeded_app();
        app.handle_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::empty()));
        for c in "John 3".chars() {
            app.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::empty()));
        }
        let text = render(&mut app);
        assert!(text.contains("Ir para: John 3"), "{text}");
    }

    #[test]
    fn empty_chapter_shows_placeholder() {
        let mut app = seeded_app();
        app.select_book(41); // Lucas (42), sem texto semeado
        let text = render(&mut app);
        assert!(text.contains("sem texto neste capítulo"));
    }
}
