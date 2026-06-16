//! Renderização da TUI (widgets ratatui) — visual repaginado.
//!
//! Painéis com bordas arredondadas e respiro, paleta truecolor coesa
//! ([`crate::theme::Palette`]), caixa de input estilizada, rodapé de atalhos,
//! overlay de ajuda (`?`) e realce de busca colorido.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{
    Block, BorderType, Clear, List, ListItem, ListState, Padding, Paragraph, Wrap,
};
use ratatui::Frame;

use the_light_core::model::Lang;
use the_light_core::reference::{format_reference, BOOKS};
use the_light_core::search::{HL_END, HL_START};

use crate::app::{App, Focus, Input, InputKind};
use crate::theme::Palette;

/// Bloco padrão: borda arredondada, respiro interno e título estilizado.
fn block(title: &str, focused: bool, pal: &Palette) -> Block<'static> {
    Block::bordered()
        .border_type(BorderType::Rounded)
        .padding(Padding::horizontal(1))
        .border_style(pal.border_style(focused))
        .title(Line::from(Span::styled(
            format!(" {title} "),
            pal.title_style(focused),
        )))
}

/// Desenha a interface completa.
pub fn draw(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    // Degradação graciosa em janelas minúsculas (evita layout impossível).
    if area.height < 3 || area.width < 24 {
        frame.render_widget(Paragraph::new("janela pequena demais"), area);
        return;
    }
    let pal = Palette::resolve(app.theme());

    // A caixa de input ocupa 3 linhas quando há espaço; senão, uma linha.
    let input_boxed = app.input.is_some() && area.height >= 7;
    let footer_h = if input_boxed { 3 } else { 1 };
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(footer_h),
    ])
    .split(area);

    draw_header(frame, app, rows[0], &pal);

    // Layout responsivo: solta o painel de estudo e depois o de livros conforme
    // a largura diminui, garantindo que o leitor nunca seja esmagado.
    let bw = rows[1].width;
    if bw >= 74 {
        let body = Layout::horizontal([
            Constraint::Length(20),
            Constraint::Min(26),
            Constraint::Length(34),
        ])
        .split(rows[1]);
        draw_books(frame, app, body[0], &pal);
        draw_reader(frame, app, body[1], &pal);
        draw_panel(frame, app, body[2], &pal);
    } else if bw >= 46 {
        let body = Layout::horizontal([Constraint::Length(20), Constraint::Min(26)]).split(rows[1]);
        draw_books(frame, app, body[0], &pal);
        draw_reader(frame, app, body[1], &pal);
    } else {
        draw_reader(frame, app, rows[1], &pal);
    }

    match &app.input {
        Some(input) => draw_input(frame, rows[2], input, &pal, input_boxed),
        None => draw_footer(frame, app, rows[2], &pal),
    }

    if app.show_help {
        draw_help(frame, area, &pal);
    }
}

/// Cabeçalho: marca à esquerda + breadcrumb (Livro Cap · VERSÃO) à direita.
fn draw_header(frame: &mut Frame, app: &App, area: Rect, pal: &Palette) {
    let brand = "✦ The Light";
    let w = area.width as usize;
    let brand_w = brand.chars().count();
    // Encolhe o breadcrumb se não couber: completo → sem versão → vazio.
    let full = format!(
        "{} {} · {}",
        app.book_name(),
        app.chapter,
        app.version_label()
    );
    let short = format!("{} {}", app.book_name(), app.chapter);
    let crumb = if brand_w + full.chars().count() + 3 <= w {
        full
    } else if brand_w + short.chars().count() + 3 <= w {
        short
    } else {
        String::new()
    };
    let used = brand_w + crumb.chars().count() + 3;
    let gap = w.saturating_sub(used).max(1);
    let line = Line::from(vec![
        Span::styled(
            format!(" {brand}"),
            pal.fg(pal.accent).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" ".repeat(gap)),
        Span::styled(format!("{crumb} "), pal.fg(pal.dim)),
    ]);
    frame.render_widget(Paragraph::new(line), area);
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

/// Converte uma string com marcadores [`HL_START`]/[`HL_END`] em spans, com os
/// trechos casados realçados. Os marcadores nunca aparecem no resultado.
fn highlight_spans(s: &str, pal: &Palette) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let style = pal.match_style();
    let mut rest = s;
    while let Some(start) = rest.find(HL_START) {
        if start > 0 {
            spans.push(Span::raw(rest[..start].to_string()));
        }
        rest = &rest[start + HL_START.len()..];
        match rest.find(HL_END) {
            Some(end) => {
                spans.push(Span::styled(rest[..end].to_string(), style));
                rest = &rest[end + HL_END.len()..];
            }
            None => {
                // Marcador de abertura sem fechamento: estiliza o restante.
                spans.push(Span::styled(rest.to_string(), style));
                rest = "";
                break;
            }
        }
    }
    if !rest.is_empty() {
        spans.push(Span::raw(rest.to_string()));
    }
    spans
}

fn draw_books(frame: &mut Frame, app: &App, area: Rect, pal: &Palette) {
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
        .block(block("Livros", app.focus == Focus::Books, pal))
        .highlight_style(pal.fg(pal.accent).add_modifier(Modifier::BOLD))
        .highlight_symbol("❯ ");

    let mut state = ListState::default();
    state.select(Some(app.book_idx));
    frame.render_stateful_widget(list, area, &mut state);
}

fn draw_reader(frame: &mut Frame, app: &App, area: Rect, pal: &Palette) {
    let title = format!(
        "{} {}  ·  {}",
        app.book_name(),
        app.chapter,
        app.version_label()
    );
    let blk = block(&title, app.focus == Focus::Reader, pal);
    let inner_width = blk.inner(area).width as usize;

    if app.verses.is_empty() {
        let p = Paragraph::new(Span::styled(
            "(sem texto neste capítulo)",
            pal.fg(pal.dim).add_modifier(Modifier::ITALIC),
        ))
        .block(blk);
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
                        Span::styled(
                            format!("{n:>numw$}  "),
                            pal.fg(pal.accent).add_modifier(Modifier::BOLD),
                        ),
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
        .block(blk)
        .highlight_style(pal.selection_style());

    let mut state = ListState::default();
    state.select(Some(app.selected));
    frame.render_stateful_widget(list, area, &mut state);
}

/// Painel lateral: busca (se ativa), navegação de xref ou estudo do versículo.
fn draw_panel(frame: &mut Frame, app: &App, area: Rect, pal: &Palette) {
    // Resultados de busca incremental (com realce dos termos casados).
    if let Some(input) = &app.input {
        if input.kind == InputKind::Search {
            let items: Vec<ListItem> = app
                .search_results
                .iter()
                .map(|h| {
                    let mut spans = vec![Span::styled(
                        format!("{}  ", format_reference(&h.reference, app.lang())),
                        pal.fg(pal.dim),
                    )];
                    spans.extend(highlight_spans(&h.highlighted, pal));
                    ListItem::new(Line::from(spans))
                })
                .collect();
            let title = format!("Busca ({})", app.search_results.len());
            let list = List::new(items)
                .block(
                    Block::bordered()
                        .border_type(BorderType::Rounded)
                        .padding(Padding::horizontal(1))
                        .border_style(pal.fg(pal.warn))
                        .title(Line::from(Span::styled(
                            format!(" {title} "),
                            pal.fg(pal.warn).add_modifier(Modifier::BOLD),
                        ))),
                )
                .highlight_style(pal.selection_style());
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
                    .border_type(BorderType::Rounded)
                    .padding(Padding::horizontal(1))
                    .border_style(pal.fg(pal.warn))
                    .title(Line::from(Span::styled(
                        " Refs cruzadas — Enter salta, Esc fecha ",
                        pal.fg(pal.warn).add_modifier(Modifier::BOLD),
                    ))),
            )
            .highlight_style(pal.selection_style());
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
        pal.fg(pal.accent).add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(""));

    lines.push(section_title("Marcações", pal));
    let highlights = app.current_highlights();
    if highlights.is_empty() {
        lines.push(dim_item(pal));
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

    lines.push(section_title("Nota", pal));
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
        None => lines.push(dim_item(pal)),
    }
    lines.push(Line::from(""));

    let xcount = app.current_xrefs().len();
    lines.push(section_title("Refs cruzadas", pal));
    if xcount == 0 {
        lines.push(dim_item(pal));
    } else {
        lines.push(Line::from(Span::styled(
            format!("  {xcount} · x para abrir"),
            pal.fg(pal.dim),
        )));
    }

    let panel = Paragraph::new(Text::from(lines))
        .wrap(Wrap { trim: false })
        .block(block("Estudo", false, pal));
    frame.render_widget(panel, area);
}

/// Título de seção do painel de estudo.
fn section_title(label: &str, pal: &Palette) -> Line<'static> {
    Line::from(Span::styled(
        label.to_string(),
        pal.fg(pal.accent).add_modifier(Modifier::BOLD),
    ))
}

/// Item vazio (`—`) em tom suave.
fn dim_item(pal: &Palette) -> Line<'static> {
    Line::from(Span::styled("  —", pal.fg(pal.dim)))
}

/// Caixa/linha de input estilizada (busca `/` ou ir-para `g`).
fn draw_input(frame: &mut Frame, area: Rect, input: &Input, pal: &Palette, boxed: bool) {
    let label = match input.kind {
        InputKind::GoTo => "ir para",
        InputKind::Search => "buscar",
    };
    let mut spans = vec![
        Span::styled("❯ ", pal.fg(pal.accent).add_modifier(Modifier::BOLD)),
        Span::raw(input.buffer.clone()),
    ];
    if let Some(e) = &input.error {
        spans.push(Span::styled(format!("   ⚠ {e}"), pal.fg(pal.warn)));
    }
    // Colunas até o fim do texto digitado: "❯ " (2) + caracteres do buffer.
    // (`chars().count()` casa com a largura em texto latino/acentuado da busca.)
    let typed_off = 2 + input.buffer.chars().count() as u16;

    if boxed {
        let blk = Block::bordered()
            .border_type(BorderType::Rounded)
            .padding(Padding::horizontal(1))
            .border_style(pal.fg(pal.accent))
            .title(Line::from(Span::styled(
                format!(" {label} "),
                pal.fg(pal.accent).add_modifier(Modifier::BOLD),
            )));
        let inner = blk.inner(area);
        frame.render_widget(Paragraph::new(Line::from(spans)).block(blk), area);
        frame.set_cursor_position((inner.x + typed_off, inner.y));
    } else {
        // Modo compacto (1 linha): inclui o rótulo em texto antes do prompt.
        let prefix = format!(" {label} ");
        let prefix_w = prefix.chars().count() as u16;
        let mut line = vec![Span::styled(
            prefix,
            pal.fg(pal.dim).add_modifier(Modifier::BOLD),
        )];
        line.extend(spans);
        frame.render_widget(Paragraph::new(Line::from(line)), area);
        // Cursor após o rótulo + "❯ " + texto, dentro da área.
        let x = (area.x + prefix_w + typed_off).min(area.x + area.width.saturating_sub(1));
        frame.set_cursor_position((x, area.y));
    }
}

/// Rodapé de atalhos (quando não há input ativo). Mostra o tema atual.
fn draw_footer(frame: &mut Frame, app: &App, area: Rect, pal: &Palette) {
    let hints = format!(
        " ↑↓ versículo · n/p cap · v versão · / buscar · g ir · x refs · t tema [{}] · ? ajuda",
        app.theme()
    );
    frame.render_widget(Paragraph::new(hints).style(pal.fg(pal.dim)), area);
}

/// Overlay central de ajuda (atalhos), aberto/fechado por `?`.
fn draw_help(frame: &mut Frame, area: Rect, pal: &Palette) {
    let lines = vec![
        Line::from(Span::styled(
            "Atalhos",
            pal.fg(pal.accent).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from("  ↑↓ / j k    mover (versículo ou livro)"),
        Line::from("  n / p       próximo / capítulo anterior"),
        Line::from("  ← →         trocar de painel · Tab alterna foco"),
        Line::from("  Home/End    início / fim · PgUp/PgDn ±10"),
        Line::from("  v           trocar versão"),
        Line::from("  /           buscar (full-text)"),
        Line::from("  g           ir para referência (ex.: Jo 3.16)"),
        Line::from("  x           referências cruzadas"),
        Line::from("  t           trocar tema (dark/light/none)"),
        Line::from("  q / Esc     sair"),
        Line::from(""),
        Line::from(Span::styled("  ? ou Esc fecha esta ajuda", pal.fg(pal.dim))),
    ];
    let popup = centered_rect(58, lines.len() as u16 + 2, area);
    let blk = Block::bordered()
        .border_type(BorderType::Rounded)
        .padding(Padding::horizontal(1))
        .border_style(pal.fg(pal.accent))
        .title(Line::from(Span::styled(
            " Ajuda ",
            pal.fg(pal.accent).add_modifier(Modifier::BOLD),
        )));
    frame.render_widget(Clear, popup);
    frame.render_widget(Paragraph::new(lines).block(blk), popup);
}

/// Retângulo centralizado em `area`, limitado ao tamanho disponível.
fn centered_rect(w: u16, h: u16, area: Rect) -> Rect {
    let w = w.min(area.width);
    let h = h.min(area.height);
    Rect {
        x: area.x + area.width.saturating_sub(w) / 2,
        y: area.y + area.height.saturating_sub(h) / 2,
        width: w,
        height: h,
    }
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
        // Caixa de input estilizada com rótulo + texto digitado.
        assert!(text.contains("buscar"), "{text}");
        assert!(text.contains("sinned"), "{text}");
        assert!(text.contains("Busca ("), "{text}");
        // O realce não deixa vazar os marcadores de controle no resultado.
        assert!(!text.contains(HL_START), "{text}");
        assert!(!text.contains(HL_END), "{text}");
    }

    #[test]
    fn panels_use_rounded_borders() {
        let mut app = seeded_app();
        let text = render(&mut app);
        assert!(text.contains('╭'), "{text}");
    }

    #[test]
    fn help_overlay_toggles() {
        let mut app = seeded_app();
        app.handle_key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::empty()));
        assert!(app.show_help);
        let text = render(&mut app);
        assert!(text.contains("Atalhos"), "{text}");
        assert!(text.contains("Ajuda"), "{text}");
        app.handle_key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::empty()));
        assert!(!app.show_help);
    }

    #[test]
    fn highlight_spans_strips_markers_and_styles_match() {
        let pal = Palette::resolve("dark");
        let s = format!("a {HL_START}b{HL_END} c");
        let spans = highlight_spans(&s, &pal);
        let joined: String = spans.iter().map(|sp| sp.content.as_ref()).collect();
        assert_eq!(joined, "a b c");
        assert!(spans
            .iter()
            .any(|sp| sp.content.as_ref() == "b" && sp.style.fg == Some(pal.warn)));
    }

    #[test]
    fn none_theme_renders_without_color_and_keeps_content() {
        let mut app = seeded_app();
        app.config.theme = "none".into();
        let text = render(&mut app);
        // Sem cor, mas a estrutura e o conteúdo permanecem.
        assert!(text.contains('╭'), "{text}");
        assert!(text.contains("Estudo"), "{text}");
        assert!(text.contains("For all have sinned"), "{text}");
    }

    #[test]
    fn rounded_borders_all_corners() {
        let mut app = seeded_app();
        let text = render(&mut app);
        for corner in ['╭', '╮', '╰', '╯'] {
            assert!(text.contains(corner), "faltou o canto {corner}:\n{text}");
        }
    }

    #[test]
    fn search_match_cells_styled_with_warn_bold() {
        let mut app = seeded_app();
        app.config.theme = "dark".into();
        app.handle_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::empty()));
        // "for" casa "For ..." perto do início do resultado (visível no painel).
        for c in "for".chars() {
            app.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::empty()));
        }
        let pal = Palette::resolve("dark");
        let mut terminal = Terminal::new(TestBackend::new(100, 24)).unwrap();
        terminal.draw(|f| draw(f, &mut app)).unwrap();
        let buf = terminal.backend().buffer();
        // O 'F' maiúsculo da correspondência "For" (ausente no título/ref) deve
        // sair em warn + negrito — confirma que o realce chega ao buffer.
        let styled = (0..buf.area.height).any(|y| {
            (0..buf.area.width).any(|x| {
                buf.cell((x, y)).is_some_and(|c| {
                    c.symbol() == "F" && c.fg == pal.warn && c.modifier.contains(Modifier::BOLD)
                })
            })
        });
        assert!(
            styled,
            "o termo casado deveria sair realçado (warn + negrito)"
        );
    }

    #[test]
    fn input_cursor_follows_typed_text() {
        let mut app = seeded_app();
        app.handle_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::empty()));
        for c in "ab".chars() {
            app.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::empty()));
        }
        let mut terminal = Terminal::new(TestBackend::new(100, 24)).unwrap();
        terminal.draw(|f| draw(f, &mut app)).unwrap();
        let x1 = terminal.get_cursor_position().unwrap().x;
        // Mais um caractere → cursor avança exatamente uma coluna.
        app.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::empty()));
        terminal.draw(|f| draw(f, &mut app)).unwrap();
        let x2 = terminal.get_cursor_position().unwrap().x;
        assert_eq!(x2, x1 + 1, "cursor deveria avançar 1 coluna por caractere");
    }

    #[test]
    fn responsive_layout_is_readable_when_narrow() {
        let mut app = seeded_app();
        for w in [30u16, 50, 100] {
            let mut terminal = Terminal::new(TestBackend::new(w, 24)).unwrap();
            terminal.draw(|f| draw(f, &mut app)).unwrap();
            let text = to_text(terminal.backend().buffer());
            assert!(
                text.contains("For all have sinned"),
                "largura {w}: leitor deveria mostrar o texto:\n{text}"
            );
        }
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
