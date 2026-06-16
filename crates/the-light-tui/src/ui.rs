//! Renderização da TUI (widgets ratatui).

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Frame;

use the_light_core::model::Lang;
use the_light_core::reference::{format_reference, BOOKS};

use crate::app::{App, Focus, InputKind};

/// Cor de destaque conforme o tema.
fn accent(theme: &str) -> Color {
    match theme {
        "light" => Color::Blue,
        "none" => Color::Reset,
        _ => Color::Cyan,
    }
}

/// Desenha a interface completa.
pub fn draw(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    // Degradação graciosa em janelas minúsculas (evita layout impossível).
    if area.height < 3 || area.width < 24 {
        frame.render_widget(Paragraph::new("janela pequena demais"), area);
        return;
    }
    let acc = accent(app.theme());
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .split(area);

    let title = format!(" The Light — TUI    [{}]", app.version_label());
    frame.render_widget(
        Paragraph::new(title).style(Style::new().add_modifier(Modifier::BOLD)),
        rows[0],
    );

    let body = Layout::horizontal([
        Constraint::Length(20),
        Constraint::Min(20),
        Constraint::Length(34),
    ])
    .split(rows[1]);
    draw_books(frame, app, body[0], acc);
    draw_reader(frame, app, body[1], acc);
    draw_panel(frame, app, body[2], acc);
    draw_status(frame, app, rows[2]);
}

fn border_style(focused: bool, acc: Color) -> Style {
    if focused {
        Style::new().fg(acc)
    } else {
        Style::new().fg(Color::DarkGray)
    }
}

/// Quebra `text` em linhas de no máximo `w` caracteres (gulosa, por palavra).
fn wrap(text: &str, w: usize) -> Vec<String> {
    if w == 0 {
        return vec![text.to_string()];
    }
    let mut lines = Vec::new();
    let mut cur = String::new();
    for word in text.split_whitespace() {
        let extra = if cur.is_empty() {
            word.chars().count()
        } else {
            word.chars().count() + 1
        };
        if cur.chars().count() + extra > w && !cur.is_empty() {
            lines.push(std::mem::take(&mut cur));
        }
        if !cur.is_empty() {
            cur.push(' ');
        }
        cur.push_str(word);
    }
    if !cur.is_empty() || lines.is_empty() {
        lines.push(cur);
    }
    lines
}

fn draw_books(frame: &mut Frame, app: &App, area: Rect, acc: Color) {
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
                .border_style(border_style(app.focus == Focus::Books, acc)),
        )
        .highlight_style(
            Style::new()
                .fg(Color::Black)
                .bg(acc)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("› ");

    let mut state = ListState::default();
    state.select(Some(app.book_idx));
    frame.render_stateful_widget(list, area, &mut state);
}

fn draw_reader(frame: &mut Frame, app: &App, area: Rect, acc: Color) {
    let title = format!(
        "{} {}  ({})",
        app.book_name(),
        app.chapter,
        app.version_label()
    );
    let block = Block::bordered()
        .title(title)
        .border_style(border_style(app.focus == Focus::Reader, acc));
    let inner_width = block.inner(area).width as usize;

    if app.verses.is_empty() {
        let p = Paragraph::new(Span::styled(
            "(sem texto neste capítulo)",
            Style::new().add_modifier(Modifier::DIM),
        ))
        .block(block);
        frame.render_widget(p, area);
        return;
    }

    let numw = app
        .verses
        .iter()
        .map(|(n, _)| n.to_string().len())
        .max()
        .unwrap_or(2)
        .max(2);
    let prefix_w = numw + 2;
    let avail = inner_width.saturating_sub(prefix_w).max(1);

    let items: Vec<ListItem> = app
        .verses
        .iter()
        .map(|(n, text)| {
            let segments = wrap(text, avail);
            let mut lines: Vec<Line> = Vec::new();
            for (i, seg) in segments.iter().enumerate() {
                if i == 0 {
                    lines.push(Line::from(vec![
                        Span::styled(format!("{n:>numw$}  "), Style::new().fg(acc)),
                        Span::raw(seg.clone()),
                    ]));
                } else {
                    lines.push(Line::from(format!("{}{}", " ".repeat(prefix_w), seg)));
                }
            }
            ListItem::new(lines)
        })
        .collect();

    let list = List::new(items)
        .block(block)
        .highlight_style(Style::new().add_modifier(Modifier::REVERSED));

    let mut state = ListState::default();
    state.select(Some(app.selected));
    frame.render_stateful_widget(list, area, &mut state);
}

/// Painel lateral: busca (se ativa), navegação de xref ou estudo do versículo.
fn draw_panel(frame: &mut Frame, app: &App, area: Rect, acc: Color) {
    // Resultados de busca incremental.
    if let Some(input) = &app.input {
        if input.kind == InputKind::Search {
            let items: Vec<ListItem> = app
                .search_results
                .iter()
                .map(|h| {
                    ListItem::new(format!(
                        "{}  {}",
                        format_reference(&h.reference, app.lang()),
                        h.text
                    ))
                })
                .collect();
            let title = format!("Busca ({})", app.search_results.len());
            let list = List::new(items)
                .block(
                    Block::bordered()
                        .title(title)
                        .border_style(Style::new().fg(Color::Yellow)),
                )
                .highlight_style(Style::new().add_modifier(Modifier::REVERSED));
            let mut state = ListState::default();
            if !app.search_results.is_empty() {
                state.select(Some(app.search_selected));
            }
            frame.render_stateful_widget(list, area, &mut state);
            return;
        }
    }

    if let Some(nav) = &app.xref_nav {
        let items: Vec<ListItem> = nav
            .items
            .iter()
            .map(|c| {
                ListItem::new(format!(
                    "{}  ({})",
                    format_reference(&c.reference, app.lang()),
                    c.votes
                ))
            })
            .collect();
        let list = List::new(items)
            .block(
                Block::bordered()
                    .title("Refs cruzadas — Enter salta, Esc fecha")
                    .border_style(Style::new().fg(Color::Yellow)),
            )
            .highlight_style(Style::new().add_modifier(Modifier::REVERSED));
        let mut state = ListState::default();
        state.select(Some(nav.selected));
        frame.render_stateful_widget(list, area, &mut state);
        return;
    }

    let mut lines: Vec<Line> = Vec::new();
    let ref_str = app
        .current_reference()
        .map(|r| format_reference(&r, app.lang()))
        .unwrap_or_else(|| "—".to_string());
    lines.push(Line::from(Span::styled(
        ref_str,
        Style::new().add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(""));

    lines.push(Line::from(Span::styled("Marcações", Style::new().fg(acc))));
    let highlights = app.current_highlights();
    if highlights.is_empty() {
        lines.push(Line::from("  —"));
    } else {
        for h in highlights {
            let tag = h
                .tag
                .as_deref()
                .map(|t| format!(" [{t}]"))
                .unwrap_or_default();
            lines.push(Line::from(format!("  • {}{}", h.color, tag)));
        }
    }
    lines.push(Line::from(""));

    lines.push(Line::from(Span::styled("Nota", Style::new().fg(acc))));
    match app.current_note() {
        Some(note) => {
            let first = note
                .body
                .lines()
                .map(|l| l.trim_start_matches('#').trim())
                .find(|l| !l.is_empty())
                .unwrap_or("");
            lines.push(Line::from(format!("  {first}")));
        }
        None => lines.push(Line::from("  —")),
    }
    lines.push(Line::from(""));

    let xcount = app.current_xrefs().len();
    lines.push(Line::from(Span::styled(
        "Refs cruzadas",
        Style::new().fg(acc),
    )));
    if xcount == 0 {
        lines.push(Line::from("  —"));
    } else {
        lines.push(Line::from(format!("  {xcount} (x para abrir)")));
    }

    let panel = Paragraph::new(Text::from(lines))
        .wrap(Wrap { trim: false })
        .block(
            Block::bordered()
                .title("Estudo")
                .border_style(Style::new().fg(Color::DarkGray)),
        );
    frame.render_widget(panel, area);
}

/// Barra inferior: prompt de entrada (se ativo) ou ajuda de teclas.
fn draw_status(frame: &mut Frame, app: &App, area: Rect) {
    let widget = match &app.input {
        Some(input) => {
            let label = match input.kind {
                InputKind::GoTo => "Ir para",
                InputKind::Search => "Buscar",
            };
            let text = match &input.error {
                Some(e) => format!(" {label}: {}    ⚠ {e}", input.buffer),
                None => format!(" {label}: {}", input.buffer),
            };
            Paragraph::new(text).style(Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD))
        }
        None => Paragraph::new(format!(
            " q sair · ↑↓ versículo · n/p cap · v versão · / buscar · g ir · x refs · t tema [{}] · Tab foco",
            app.theme()
        ))
        .style(Style::new().add_modifier(Modifier::DIM)),
    };
    frame.render_widget(widget, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::buffer::Buffer;
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::Terminal;
    use rusqlite::params;
    use the_light_core::reference::parse_reference;
    use the_light_core::store::Store;
    use the_light_core::userdata::Highlight;

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
            for (id, b, c, v, t) in [
                (1i64, 45, 3, 23, "For all have sinned and come short"),
                (2, 45, 3, 24, "Being justified freely by his grace"),
                (3, 45, 6, 23, "For the wages of sin is death"),
            ] {
                conn.execute(
                    "INSERT INTO verses(id,translation_id,book_number,chapter,verse,text) \
                     VALUES (?1,'kjv',?2,?3,?4,?5)",
                    params![id, b, c, v, t],
                )
                .unwrap();
                conn.execute(
                    "INSERT INTO verses_fts(text, translation_id, verse_id) VALUES (?1,'kjv',?2)",
                    params![t, id],
                )
                .unwrap();
            }
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

    fn render(app: &mut App) -> String {
        let mut terminal = Terminal::new(TestBackend::new(100, 24)).unwrap();
        terminal.draw(|f| draw(f, app)).unwrap();
        to_text(terminal.backend().buffer())
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

    #[test]
    fn panel_shows_study_sections_and_xref_count() {
        let mut app = seeded_app();
        app.highlights = vec![Highlight {
            reference: parse_reference("Rm 3.23").unwrap(),
            color: "yellow".into(),
            tag: Some("pecado".into()),
        }];
        let text = render(&mut app);
        assert!(text.contains("Estudo"));
        assert!(text.contains("Marcações"));
        assert!(text.contains("[pecado]"));
        assert!(text.contains("Refs cruzadas"));
        assert!(text.contains("For all have sinned"));
    }

    #[test]
    fn xref_nav_panel_lists_targets() {
        let mut app = seeded_app();
        app.handle_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::empty()));
        let text = render(&mut app);
        assert!(text.contains("Refs cruzadas — Enter salta"));
        assert!(text.contains("Romans 6:23"));
        assert!(text.contains("(50)"));
    }

    #[test]
    fn search_prompt_and_results_render() {
        let mut app = seeded_app();
        app.handle_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::empty()));
        for c in "sinned".chars() {
            app.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::empty()));
        }
        let text = render(&mut app);
        assert!(text.contains("Buscar: sinned"), "{text}");
        assert!(text.contains("Busca ("), "{text}");
    }

    #[test]
    fn tiny_terminal_does_not_panic() {
        let mut app = seeded_app();
        for (w, h) in [(1u16, 1u16), (2, 2), (10, 3), (24, 3), (80, 2)] {
            let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
            terminal.draw(|f| draw(f, &mut app)).unwrap();
        }
    }

    #[test]
    fn status_bar_shows_theme() {
        let mut app = seeded_app();
        app.config.theme = "light".into();
        let text = render(&mut app);
        assert!(text.contains("t tema [light]"), "{text}");
    }
}
