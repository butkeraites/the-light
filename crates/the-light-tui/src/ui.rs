//! Renderização da TUI (widgets ratatui) — visual repaginado.
//!
//! Painéis com bordas arredondadas e respiro, paleta truecolor coesa
//! ([`crate::theme::Palette`]), caixa de input estilizada, rodapé de atalhos,
//! overlay de ajuda (`?`) e realce de busca colorido.

use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{
    Block, BorderType, Clear, List, ListItem, ListState, Padding, Paragraph, Wrap,
};
use ratatui::Frame;
use unicode_width::UnicodeWidthStr;

use the_light_core::ai::{ChatRole, PROVIDERS};
use the_light_core::model::{Lang, Reference, VerseRange};
use the_light_core::reference::{format_reference, scan_references, BOOKS};
use the_light_core::search::{HL_END, HL_START};

use the_light_core::ai::StudyMode;

use crate::app::{
    AiPanel, AiStatus, App, ClickTarget, Focus, Input, InputKind, ModePicker, ScholarlyPanel,
    ScholarlyState, Selection, SessionBrowser, SettingsMode,
};
use crate::theme::Palette;

/// Lista de alvos clicáveis que uma função de desenho produz (ver
/// [`crate::app::ClickTarget`]). O chamador estende `app.click_targets`.
type Hits = Vec<(Rect, ClickTarget)>;

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
    // Sem leitor renderizado neste frame não há área de seleção; reposiciona-se
    // adiante quando o leitor é de fato desenhado.
    app.reader_inner = None;
    // Alvos clicáveis são remontados a cada frame pelas funções de desenho.
    app.click_targets.clear();
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
        let book_hits = draw_books(frame, app, body[0], &pal);
        app.click_targets.extend(book_hits);
        let (inner, verse_hits) = draw_reader(frame, app, body[1], &pal);
        app.reader_inner = Some(inner);
        app.click_targets.extend(verse_hits);
        draw_panel(frame, app, body[2], &pal);
    } else if bw >= 46 {
        let body = Layout::horizontal([Constraint::Length(20), Constraint::Min(26)]).split(rows[1]);
        let book_hits = draw_books(frame, app, body[0], &pal);
        app.click_targets.extend(book_hits);
        let (inner, verse_hits) = draw_reader(frame, app, body[1], &pal);
        app.reader_inner = Some(inner);
        app.click_targets.extend(verse_hits);
    } else {
        let (inner, verse_hits) = draw_reader(frame, app, rows[1], &pal);
        app.reader_inner = Some(inner);
        app.click_targets.extend(verse_hits);
    }

    // Pinta a seleção do mouse sobre o leitor e remonta o texto copiável.
    apply_selection(frame, app, &pal);

    match &app.input {
        Some(input) => draw_input(frame, rows[2], input, &pal, input_boxed),
        None => draw_footer(frame, app, rows[2], &pal),
    }

    if app.show_help {
        draw_help(frame, area, &pal);
    }
    if app.ai.is_some() {
        let hits = draw_ai_panel(frame, app, area, &pal);
        app.click_targets.extend(hits);
    }
    if app.settings.is_some() {
        let hits = draw_settings(frame, app, area, &pal);
        app.click_targets.extend(hits);
    }
    let hits = if let Some(browser) = &app.sessions {
        draw_sessions(frame, browser, area, &pal)
    } else {
        Hits::new()
    };
    app.click_targets.extend(hits);
    let hits = if let Some(picker) = &app.mode_picker {
        draw_mode_picker(frame, picker, area, &pal)
    } else {
        Hits::new()
    };
    app.click_targets.extend(hits);
    let hits = if let Some(panel) = &app.scholarly {
        draw_scholarly(frame, panel, app.spinner, area, &pal)
    } else {
        Hits::new()
    };
    app.click_targets.extend(hits);
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

fn draw_books(frame: &mut Frame, app: &App, area: Rect, pal: &Palette) -> Hits {
    let blk = block("Livros", app.focus == Focus::Books, pal);
    let inner = blk.inner(area);
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
        .block(blk)
        .highlight_style(pal.fg(pal.accent).add_modifier(Modifier::BOLD))
        .highlight_symbol("❯ ");

    let mut state = ListState::default();
    state.select(Some(app.book_idx));
    frame.render_stateful_widget(list, area, &mut state);

    // Cada linha visível (1 por livro) vira um alvo clicável a partir do offset
    // que o widget acabou de fixar.
    let offset = state.offset();
    let mut hits = Hits::new();
    for r in 0..inner.height {
        let idx = offset + r as usize;
        if idx >= BOOKS.len() {
            break;
        }
        let rect = Rect {
            x: inner.x,
            y: inner.y + r,
            width: inner.width,
            height: 1,
        };
        hits.push((rect, ClickTarget::Book(idx)));
    }
    hits
}

/// Desenha o leitor e devolve o retângulo **interno** (onde os versículos são
/// pintados) — usado para restringir a seleção de texto via mouse.
fn draw_reader(frame: &mut Frame, app: &App, area: Rect, pal: &Palette) -> (Rect, Hits) {
    let title = format!(
        "{} {}  ·  {}",
        app.book_name(),
        app.chapter,
        app.version_label()
    );
    let blk = block(&title, app.focus == Focus::Reader, pal);
    let inner = blk.inner(area);
    let inner_width = inner.width as usize;

    if app.verses.is_empty() {
        let p = Paragraph::new(Span::styled(
            "(sem texto neste capítulo)",
            pal.fg(pal.dim).add_modifier(Modifier::ITALIC),
        ))
        .block(blk);
        frame.render_widget(p, area);
        return (inner, Hits::new());
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

    // Altura (em linhas) de cada versículo após a quebra — usada tanto para o
    // item da lista quanto para mapear cliques de volta ao índice do versículo.
    let mut heights: Vec<u16> = Vec::with_capacity(app.verses.len());
    let items: Vec<ListItem> = app
        .verses
        .iter()
        .map(|(n, text)| {
            let segments = wrap(text, avail);
            heights.push(segments.len().max(1) as u16);
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

    // Mapeia cada versículo visível ao seu intervalo de linhas (top-down a partir
    // do offset que o widget fixou), respeitando a altura quebrada de cada um.
    let offset = state.offset();
    let mut hits = Hits::new();
    let mut y = inner.y;
    let bottom = inner.y.saturating_add(inner.height);
    for (idx, h) in heights.iter().enumerate().skip(offset) {
        if y >= bottom {
            break;
        }
        let visible = (*h).min(bottom - y);
        hits.push((
            Rect {
                x: inner.x,
                y,
                width: inner.width,
                height: visible,
            },
            ClickTarget::Verse(idx),
        ));
        y = y.saturating_add(*h);
    }
    (inner, hits)
}

/// Pinta a seleção do mouse sobre o buffer **já renderizado** do leitor e remonta
/// `app.selection_text` lendo célula a célula — ou seja, exatamente o que está
/// visível (robusto a quebra de linha e rolagem). A seleção é "em fluxo" (como um
/// parágrafo) e sempre confinada ao retângulo dos versículos.
fn apply_selection(frame: &mut Frame, app: &mut App, pal: &Palette) {
    let (Some(inner), Some(sel)) = (app.reader_inner, app.selection) else {
        return;
    };
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let left = inner.x;
    let right = inner.x + inner.width - 1;
    let top = inner.y;
    let bottom = inner.y + inner.height - 1;

    // Ordena âncora/cursor em ordem de leitura (linha, depois coluna).
    let (start, end) = order_points(sel);
    let y0 = start.1.clamp(top, bottom);
    let y1 = end.1.clamp(top, bottom);

    let style = pal.selection_style();
    let buf = frame.buffer_mut();
    let mut lines: Vec<String> = Vec::new();
    for y in y0..=y1 {
        // Chaves de coluna baseadas nas linhas JÁ grampeadas (y0/y1): a 1ª/última
        // linha visível recebem o recorte parcial mesmo após um resize. Linha
        // única: do menor ao maior x.
        let (cx0, cx1) = if y0 == y1 {
            (start.0.min(end.0), start.0.max(end.0))
        } else if y == y0 {
            (start.0, right)
        } else if y == y1 {
            (left, end.0)
        } else {
            (left, right)
        };
        let cx0 = cx0.clamp(left, right);
        let cx1 = cx1.clamp(left, right);
        let mut line = String::new();
        // Glifos largos ocupam 2 células (a 2ª é um espaço de preenchimento):
        // estiliza todas para um realce contíguo, mas só copia o glifo líder.
        let mut lead = cx0;
        for x in cx0..=cx1 {
            let Some(cell) = buf.cell_mut((x, y)) else {
                continue;
            };
            cell.set_style(style);
            if x == lead {
                let sym = cell.symbol();
                let w = UnicodeWidthStr::width(sym).max(1) as u16;
                line.push_str(sym);
                lead = x.saturating_add(w);
            }
        }
        // Apara espaços ao fim para que a cópia saia limpa.
        while line.ends_with(' ') {
            line.pop();
        }
        lines.push(line);
    }
    // Remove linhas vazias ao fim (arrasto que entrou na região em branco abaixo
    // do texto) para não anexar quebras de linha soltas à cópia.
    while lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }
    let body = lines.join("\n");

    // Seleção só de espaços/vazia: nada a copiar (e nada de citação solta).
    if body.trim().is_empty() {
        app.selection_text = body;
        return;
    }

    // Preâmbulo de citação (Livro Cap:Vers (VERSÃO)) deduzido do número de
    // versículo impresso na "calha" das linhas selecionadas — robusto à rolagem.
    let numw = gutter_width(&app.verses) as u16;
    let start_v = verse_at_or_above(buf, inner, numw, y0);
    let end_v = verse_at_or_above(buf, inner, numw, y1);
    let verses = match (start_v, end_v) {
        (Some(s), Some(e)) if s == e => VerseRange::Single(s),
        (Some(s), Some(e)) => VerseRange::Range {
            start: s.min(e),
            end: s.max(e),
        },
        // Número fora da viewport (versículo muito longo): cita o capítulo.
        _ => VerseRange::WholeChapter,
    };
    let reference = Reference {
        book: app.book_number(),
        chapter: app.chapter,
        verses,
    };
    let citation = format!(
        "{} ({})",
        format_reference(&reference, app.lang()),
        app.version_label()
    );
    app.selection_text = format!("{citation}\n{body}");
}

/// Largura da calha de números de versículo — espelha o cálculo do leitor.
fn gutter_width(verses: &[(u16, String)]) -> usize {
    verses
        .iter()
        .map(|(n, _)| n.to_string().len())
        .max()
        .unwrap_or(2)
        .max(2)
}

/// Lê o número de versículo impresso na calha da linha `y` (as `numw` primeiras
/// colunas internas), se houver. Linhas de continuação têm a calha em branco.
fn gutter_verse(buf: &Buffer, inner: Rect, numw: u16, y: u16) -> Option<u16> {
    let x_end = (inner.x + numw).min(inner.x + inner.width);
    let mut s = String::new();
    for x in inner.x..x_end {
        if let Some(c) = buf.cell((x, y)) {
            s.push_str(c.symbol());
        }
    }
    s.trim().parse::<u16>().ok()
}

/// Número de versículo "dono" da linha `y`: o último impresso na calha em `y` ou
/// acima (cobre linhas de continuação de versículos que quebraram em várias).
fn verse_at_or_above(buf: &Buffer, inner: Rect, numw: u16, y: u16) -> Option<u16> {
    let mut yy = y;
    loop {
        if let Some(v) = gutter_verse(buf, inner, numw, yy) {
            return Some(v);
        }
        if yy == inner.y {
            return None;
        }
        yy -= 1;
    }
}

/// Ordena os extremos da seleção em ordem de leitura `(linha, coluna)`.
fn order_points(sel: Selection) -> ((u16, u16), (u16, u16)) {
    if (sel.anchor.1, sel.anchor.0) <= (sel.cursor.1, sel.cursor.0) {
        (sel.anchor, sel.cursor)
    } else {
        (sel.cursor, sel.anchor)
    }
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
        InputKind::Ask => "perguntar",
        InputKind::StudyBrief => "estudar",
        InputKind::StudyCustom => "sua resposta",
        InputKind::StudyDeepen => "aprofundar (opcional)",
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

/// Rodapé de atalhos (quando não há input ativo). Mostra o tema atual — ou, por
/// alguns instantes, uma mensagem efêmera (ex.: confirmação de cópia).
fn draw_footer(frame: &mut Frame, app: &App, area: Rect, pal: &Palette) {
    if let Some(msg) = &app.toast {
        frame.render_widget(
            Paragraph::new(format!(" {msg}"))
                .style(pal.fg(pal.accent).add_modifier(Modifier::BOLD)),
            area,
        );
        return;
    }
    let hints = format!(
        " ↑↓ versículo · n/p cap · v versão · / buscar · g ir · x refs · t tema [{}] · a IA · s conversas · m estudar · d dados · c chaves · ? ajuda",
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
        Line::from("  a           perguntar/continuar a conversa com a IA"),
        Line::from("  s           conversas e estudos salvos (retomar)"),
        Line::from("  m           estudar (modo + lente → assunto → refinar → estudo)"),
        Line::from("  d           dados acadêmicos (instalar línguas originais + léxico)"),
        Line::from("  c           configurar provedor/chaves de IA"),
        Line::from("  /           buscar (full-text)"),
        Line::from("  g           ir para referência (ex.: Jo 3.16)"),
        Line::from("  x           referências cruzadas"),
        Line::from("  t           trocar tema (dark/light/none)"),
        Line::from("  estudo      a continua · + aprofunda · e exporta · L lente"),
        Line::from("  mouse       clique p/ navegar · arraste copia (roda: use ↑↓)"),
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

/// Overlay do seletor de modo de estudo padrão (tecla `m`).
fn draw_mode_picker(frame: &mut Frame, picker: &ModePicker, area: Rect, pal: &Palette) -> Hits {
    let mut lines: Vec<Line> = vec![
        Line::from(Span::styled(
            "Estudar — escolha o modo e a lente",
            pal.fg(pal.accent).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
    ];
    // Campo da lente (clicável/ciclável) no topo.
    let lens_row = lines.len() as u16;
    lines.push(Line::from(vec![
        Span::styled(
            "  Lente:  ",
            pal.fg(pal.accent).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("◀ {} ▶", picker.lens.name_pt()),
            pal.selection_style(),
        ),
        Span::styled("   (←/→ ou clique)", pal.fg(pal.dim)),
    ]));
    lines.push(Line::from(""));
    // Linha (relativa ao topo interno) onde cada modo começa.
    let mut mode_rows: Vec<u16> = Vec::new();
    for (i, m) in StudyMode::all().iter().enumerate() {
        mode_rows.push(lines.len() as u16);
        let cursor = if i == picker.selected { "❯ " } else { "  " };
        let style = if i == picker.selected {
            pal.selection_style()
        } else {
            pal.fg(pal.accent)
        };
        lines.push(Line::from(Span::styled(
            format!("{cursor}{}", m.name_pt()),
            style,
        )));
        lines.push(Line::from(Span::styled(
            format!("    {}", m.description_pt()),
            pal.fg(pal.dim),
        )));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "  ↑↓ modo · ←→ lente · Enter estuda · s padrão · Esc fecha",
        pal.fg(pal.dim),
    )));

    let popup = centered_rect(74, lines.len() as u16 + 2, area);
    let blk = Block::bordered()
        .border_type(BorderType::Rounded)
        .padding(Padding::horizontal(1))
        .border_style(pal.fg(pal.accent))
        .title(Line::from(Span::styled(
            " Estudar ",
            pal.fg(pal.accent).add_modifier(Modifier::BOLD),
        )));
    let inner = blk.inner(popup);
    frame.render_widget(Clear, popup);
    frame.render_widget(Paragraph::new(lines).block(blk), popup);

    let mut hits = Hits::new();
    let limit = inner.y.saturating_add(inner.height);
    // A linha da lente é clicável (cicla para a próxima).
    let ly = inner.y.saturating_add(lens_row);
    if ly < limit {
        hits.push((
            Rect {
                x: inner.x,
                y: ly,
                width: inner.width,
                height: 1,
            },
            ClickTarget::LensCycle,
        ));
    }
    // Cada modo é clicável (cobre as 2 linhas: nome + descrição).
    for (i, &rel) in mode_rows.iter().enumerate() {
        let y = inner.y.saturating_add(rel);
        if y >= limit {
            break;
        }
        hits.push((
            Rect {
                x: inner.x,
                y,
                width: inner.width,
                height: 2.min(limit - y),
            },
            ClickTarget::ModeRow(i),
        ));
    }
    hits
}

/// Overlay dos **dados acadêmicos** (línguas originais + léxico): status +
/// instalação em segundo plano (tecla `d`).
fn draw_scholarly(
    frame: &mut Frame,
    panel: &ScholarlyPanel,
    spinner: u8,
    area: Rect,
    pal: &Palette,
) -> Hits {
    let mut lines: Vec<Line> = vec![
        Line::from(Span::styled(
            "Dados acadêmicos — STEPBible (CC BY 4.0)",
            pal.fg(pal.accent).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
    ];
    // Linha (relativa ao topo interno) do botão "instalar", quando aplicável.
    let mut install_row: Option<u16> = None;
    match &panel.state {
        ScholarlyState::Absent => {
            lines.push(Line::from(
                "  Línguas originais (hebraico/grego) + Strong + léxico.",
            ));
            lines.push(Line::from(
                "  Necessário para o modo Acadêmico/Pregação fundamentar termos no original.",
            ));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "  Não instalado · ~78 MB de download · ~447 mil tokens.",
                pal.fg(pal.warn),
            )));
            lines.push(Line::from(""));
            install_row = Some(lines.len() as u16);
            lines.push(Line::from(Span::styled(
                "  Enter/i/clique instala (baixa da internet) · Esc fecha",
                pal.fg(pal.dim),
            )));
        }
        ScholarlyState::Installed { tokens, lexicon } => {
            lines.push(Line::from(Span::styled(
                format!("  ✓ Instalado: {tokens} tokens · {lexicon} entradas de léxico"),
                pal.fg(pal.accent),
            )));
            lines.push(Line::from(""));
            lines.push(Line::from(
                "  O modo Acadêmico já fundamenta os termos a partir destes dados.",
            ));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled("  Esc fecha", pal.fg(pal.dim))));
        }
        ScholarlyState::Installing(msg) => {
            const FRAMES: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
            let sp = FRAMES[(spinner as usize) % FRAMES.len()];
            lines.push(Line::from(Span::styled(
                format!("  {sp} {msg}"),
                pal.fg(pal.accent),
            )));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "  Instalando — pode levar um minuto; não feche.",
                pal.fg(pal.dim),
            )));
        }
        ScholarlyState::Error(e) => {
            lines.push(Line::from(Span::styled(
                format!("  ⚠ {e}"),
                pal.fg(pal.warn),
            )));
            lines.push(Line::from(""));
            install_row = Some(lines.len() as u16);
            lines.push(Line::from(Span::styled(
                "  Enter/i/clique tenta de novo · Esc fecha",
                pal.fg(pal.dim),
            )));
        }
    }

    let popup = centered_rect(76, lines.len() as u16 + 2, area);
    let blk = Block::bordered()
        .border_type(BorderType::Rounded)
        .padding(Padding::horizontal(1))
        .border_style(pal.fg(pal.accent))
        .title(Line::from(Span::styled(
            " Dados acadêmicos ",
            pal.fg(pal.accent).add_modifier(Modifier::BOLD),
        )));
    let inner = blk.inner(popup);
    frame.render_widget(Clear, popup);
    frame.render_widget(Paragraph::new(lines).block(blk), popup);

    let mut hits = Hits::new();
    if let Some(rel) = install_row {
        let y = inner.y.saturating_add(rel);
        if y < inner.y.saturating_add(inner.height) {
            hits.push((
                Rect {
                    x: inner.x,
                    y,
                    width: inner.width,
                    height: 1,
                },
                ClickTarget::ScholarlyInstall,
            ));
        }
    }
    hits
}

/// Overlay rolável com a **conversa** com a IA (tecla `a`; navegada por `s`).
fn draw_ai_panel(frame: &mut Frame, app: &mut App, area: Rect, pal: &Palette) -> Hits {
    // Captura o que vem de `&App` antes de emprestar `app.ai` mutavelmente: o
    // render normaliza o offset de rolagem guardado em `panel.scroll`.
    let lang = app.lang();
    let spinner = app.spinner;
    let Some(panel) = app.ai.as_mut() else {
        return Hits::new();
    };
    let session = &panel.session;
    let w = (area.width * 4 / 5).clamp(20, 92);
    let h = (area.height * 4 / 5).max(6);
    let popup = centered_rect(w, h, area);

    let title = match (session.title.is_empty(), session.anchor_label.is_empty()) {
        (true, _) => format!(" Conversa — {} ", session.anchor_label),
        (false, true) => format!(" {} ", session.title),
        (false, false) => format!(" {} — {} ", session.title, session.anchor_label),
    };
    let blk = Block::bordered()
        .border_type(BorderType::Rounded)
        .padding(Padding::horizontal(1))
        .border_style(pal.fg(pal.accent))
        .title(Line::from(Span::styled(
            title,
            pal.fg(pal.accent).add_modifier(Modifier::BOLD),
        )));
    let inner = blk.inner(popup);

    // Conversa: alterna turnos de usuário (❯, em acento) e respostas (realçadas).
    let mut lines: Vec<Line> = Vec::new();
    for m in &session.messages {
        if m.role == ChatRole::User {
            lines.push(Line::from(vec![
                Span::styled("❯ ", pal.fg(pal.accent).add_modifier(Modifier::BOLD)),
                Span::styled(
                    m.content.clone(),
                    pal.fg(pal.accent).add_modifier(Modifier::BOLD),
                ),
            ]));
        } else {
            for l in m.content.lines() {
                lines.push(ref_highlighted_line(l, pal));
            }
        }
        lines.push(Line::from(""));
    }
    match &panel.status {
        AiStatus::Pending => {
            const FRAMES: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
            let sp = FRAMES[(spinner as usize) % FRAMES.len()];
            lines.push(Line::from(Span::styled(
                format!("{sp} consultando… (Esc cancela)"),
                pal.fg(pal.dim),
            )));
        }
        AiStatus::Error(msg) => lines.push(Line::from(Span::styled(
            format!("⚠ {msg}"),
            pal.fg(pal.warn),
        ))),
        AiStatus::Idle => {}
    }
    // Opções de refinamento de escopo: renderizadas num bloco FIXO no rodapé
    // (sempre visível e clicável), não no fluxo rolável. Capturadas aqui.
    let study_opts: Option<(u8, usize, Vec<String>)> = panel
        .study
        .as_ref()
        .filter(|f| !f.options.is_empty())
        .map(|f| (f.round, f.selected, f.options.clone()));
    // Lista numerada de saltos rápidos (a "viagem rápida").
    if !panel.refs.is_empty() {
        lines.push(Line::from(Span::styled(
            "Saltar para  (Tab seleciona · Enter/1-9 salta):",
            pal.fg(pal.accent).add_modifier(Modifier::BOLD),
        )));
        for (i, r) in panel.refs.iter().enumerate() {
            let marker = if i < 9 {
                format!("[{}] ", i + 1)
            } else {
                "    ".to_string()
            };
            let label = format_reference(r, lang);
            let style = if i == panel.ref_selected {
                pal.selection_style()
            } else {
                pal.fg(pal.accent)
            };
            lines.push(Line::from(Span::styled(
                format!("  {marker}{label}"),
                style,
            )));
        }
    }

    // Altura do bloco fixo de opções (cabeçalho + 1 por opção), limitada ao
    // espaço acima do rodapé.
    let mut hits = Hits::new();
    let study_h = match &study_opts {
        Some((_, _, opts)) => {
            let want = 1 + opts.len() as u16;
            want.min(inner.height.saturating_sub(2)) // deixa ≥1 linha de conversa + rodapé
        }
        None => 0,
    };
    // Limpa o fundo e delega rolagem + clamp ao parágrafo rolável (reserva o
    // rodapé + o bloco de opções). Mantém quebra e clamp juntos.
    frame.render_widget(Clear, popup);
    panel
        .scroll
        .render(frame, popup, blk, Text::from(lines), 1 + study_h);

    // Bloco fixo de opções logo acima do rodapé (com alvos clicáveis).
    if let Some((round, selected, opts)) = study_opts {
        if study_h > 0 {
            // Aritmética saturante (defensiva): nunca produz um Rect degenerado.
            let footer_y = inner.y.saturating_add(inner.height).saturating_sub(1);
            let top = footer_y.saturating_sub(study_h);
            let header = Rect {
                x: inner.x,
                y: top,
                width: inner.width,
                height: 1,
            };
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    format!("Rodada {round}/3  (clique/Enter/1-9 · c digita a sua · Esc cancela):"),
                    pal.fg(pal.accent).add_modifier(Modifier::BOLD),
                ))),
                header,
            );
            for (i, opt) in opts.iter().enumerate() {
                let y = top.saturating_add(1).saturating_add(i as u16);
                if y >= footer_y {
                    break;
                }
                let marker = if i < 9 {
                    format!("[{}] ", i + 1)
                } else {
                    "    ".to_string()
                };
                let style = if i == selected {
                    pal.selection_style()
                } else {
                    pal.fg(pal.accent)
                };
                let rect = Rect {
                    x: inner.x,
                    y,
                    width: inner.width,
                    height: 1,
                };
                frame.render_widget(
                    Paragraph::new(Line::from(Span::styled(format!("  {marker}{opt}"), style))),
                    rect,
                );
                hits.push((rect, ClickTarget::StudyOption(i)));
            }
        }
    }

    // Rodapé (sobrescreve a última linha interna) com os atalhos da conversa.
    if inner.height >= 1 {
        let frect = Rect {
            x: inner.x,
            y: inner.y.saturating_add(inner.height).saturating_sub(1),
            width: inner.width,
            height: 1,
        };
        frame.render_widget(
            Paragraph::new(ai_footer(panel)).style(pal.fg(pal.dim)),
            frect,
        );
    }
    hits
}

/// Quebra uma linha de resposta em spans, realçando as referências citadas.
fn ref_highlighted_line(src: &str, pal: &Palette) -> Line<'static> {
    let scanned = scan_references(src);
    if scanned.is_empty() {
        return Line::from(src.to_string());
    }
    let style = pal
        .fg(pal.accent)
        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED);
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut last = 0usize;
    for sr in scanned {
        if sr.range.start > last {
            spans.push(Span::raw(src[last..sr.range.start].to_string()));
        }
        spans.push(Span::styled(src[sr.range.clone()].to_string(), style));
        last = sr.range.end;
    }
    if last < src.len() {
        spans.push(Span::raw(src[last..].to_string()));
    }
    Line::from(spans)
}

/// Rodapé do overlay de conversa: provedor/modelo + atalhos.
fn ai_footer(panel: &AiPanel) -> String {
    // Em refinamento de escopo os atalhos são os da escolha de opções.
    let refining = panel.study.as_ref().is_some_and(|f| !f.options.is_empty());
    if refining {
        return "↑↓ move · Enter/1-9 escolhe · c digita a sua · Esc cancela".to_string();
    }
    let is_study = panel.session.study_mode.is_some() && panel.study.is_none();
    let extra = if is_study {
        " · + aprofunda · e exporta · L lente"
    } else {
        ""
    };
    let nav = if panel.refs.is_empty() {
        format!("a continua · ↑↓ rola{extra} · Esc fecha")
    } else {
        format!("a continua · Tab/1-9 salta · ↑↓ rola{extra} · Esc fecha")
    };
    let model = &panel.session.model;
    if model.is_empty() {
        nav
    } else {
        format!("{model} · {nav}")
    }
}

/// Modal de configuração de provedor/chaves de IA (tecla `c`).
fn draw_settings(frame: &mut Frame, app: &App, area: Rect, pal: &Palette) -> Hits {
    let Some(settings) = &app.settings else {
        return Hits::new();
    };
    let active = app.config.provider.trim().to_ascii_lowercase();

    let mut lines: Vec<Line> = vec![
        Line::from(Span::styled(
            "Provedor de IA",
            pal.fg(pal.accent).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
    ];
    // Linha (relativa ao topo interno) onde os provedores começam.
    let provider_top = lines.len() as u16;
    for (i, name) in PROVIDERS.iter().enumerate() {
        let is_sel = i == settings.selected;
        let star = if active.as_str() == *name { "★" } else { " " };
        let has_key = settings.has_key.get(i).copied().unwrap_or(false);
        let keymark = if has_key {
            "✓ chave salva"
        } else {
            "— sem chave"
        };
        let note = if *name == "ollama" { " (local)" } else { "" };
        let cursor = if is_sel { "❯ " } else { "  " };
        let style = if is_sel {
            pal.selection_style()
        } else {
            pal.fg(pal.dim)
        };
        lines.push(Line::from(Span::styled(
            format!("{cursor}{star} {name}{note}   {keymark}"),
            style,
        )));
    }
    lines.push(Line::from(""));
    match &settings.mode {
        SettingsMode::EditKey(buf) => {
            let masked: String = "•".repeat(buf.chars().count());
            lines.push(Line::from(vec![
                Span::styled("chave: ", pal.fg(pal.accent).add_modifier(Modifier::BOLD)),
                Span::raw(masked),
            ]));
            lines.push(Line::from(Span::styled(
                "  Enter grava · Esc cancela (a chave fica oculta)",
                pal.fg(pal.dim),
            )));
        }
        SettingsMode::List => {
            lines.push(Line::from(Span::styled(
                "  ↑↓ move · Enter ativa · e edita chave · d remove · Esc fecha",
                pal.fg(pal.dim),
            )));
            lines.push(Line::from(Span::styled(
                "  ★ ativo · ✓ chave salva no cofre (0600, fora do git)",
                pal.fg(pal.dim),
            )));
        }
    }

    let popup = centered_rect(64, lines.len() as u16 + 2, area);
    let blk = Block::bordered()
        .border_type(BorderType::Rounded)
        .padding(Padding::horizontal(1))
        .border_style(pal.fg(pal.accent))
        .title(Line::from(Span::styled(
            " Configurar IA ",
            pal.fg(pal.accent).add_modifier(Modifier::BOLD),
        )));
    let inner = blk.inner(popup);
    frame.render_widget(Clear, popup);
    frame.render_widget(Paragraph::new(lines).block(blk), popup);

    // Linhas de provedor clicáveis (uma por provedor) — só fora da edição de chave.
    let mut hits = Hits::new();
    if matches!(settings.mode, SettingsMode::List) {
        for i in 0..PROVIDERS.len() {
            let y = inner.y + provider_top + i as u16;
            if y >= inner.y + inner.height {
                break;
            }
            hits.push((
                Rect {
                    x: inner.x,
                    y,
                    width: inner.width,
                    height: 1,
                },
                ClickTarget::SettingsRow(i),
            ));
        }
    }
    hits
}

/// Navegador de conversas salvas (tecla `s`).
fn draw_sessions(frame: &mut Frame, browser: &SessionBrowser, area: Rect, pal: &Palette) -> Hits {
    let mut lines: Vec<Line> = vec![
        Line::from(Span::styled(
            "Conversas e estudos salvos",
            pal.fg(pal.accent).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
    ];
    // Linha (relativa ao topo interno) onde a 1ª conversa aparece.
    let rows_top = lines.len() as u16;
    if browser.items.is_empty() {
        lines.push(Line::from(Span::styled(
            "  (nenhuma ainda — tecle a para começar uma conversa)",
            pal.fg(pal.dim),
        )));
    } else {
        for (i, s) in browser.items.iter().enumerate() {
            let when = s.updated_at.format("%Y-%m-%d %H:%M");
            let cursor = if i == browser.selected { "❯ " } else { "  " };
            let style = if i == browser.selected {
                pal.selection_style()
            } else {
                pal.fg(pal.dim)
            };
            // Âncora = capítulo (conversa) ou tag de estudo `[estudo {modo}]`.
            let tag = match s.study_mode {
                Some(mode) => format!("[estudo {}]", mode.name_pt()),
                None if s.anchor_label.is_empty() => "[conversa]".to_string(),
                None => s.anchor_label.clone(),
            };
            lines.push(Line::from(Span::styled(
                format!("{cursor}{}  ·  {tag}  ·  {when}", s.title),
                style,
            )));
        }
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "  ↑↓ move · Enter abre · d apaga · Esc fecha",
        pal.fg(pal.dim),
    )));

    let popup = centered_rect(72, lines.len() as u16 + 2, area);
    let blk = Block::bordered()
        .border_type(BorderType::Rounded)
        .padding(Padding::horizontal(1))
        .border_style(pal.fg(pal.accent))
        .title(Line::from(Span::styled(
            " Conversas ",
            pal.fg(pal.accent).add_modifier(Modifier::BOLD),
        )));
    let inner = blk.inner(popup);
    frame.render_widget(Clear, popup);
    frame.render_widget(Paragraph::new(lines).block(blk), popup);

    // Cada conversa/estudo é uma linha clicável.
    let mut hits = Hits::new();
    for i in 0..browser.items.len() {
        let y = inner.y + rows_top + i as u16;
        if y >= inner.y + inner.height {
            break;
        }
        hits.push((
            Rect {
                x: inner.x,
                y,
                width: inner.width,
                height: 1,
            },
            ClickTarget::SessionRow(i),
        ));
    }
    hits
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
    use ratatui::crossterm::event::{
        KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
    };
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
        let mut app = App::new(store, "kjv".into(), None).unwrap();
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

    fn mouse(kind: MouseEventKind, col: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind,
            column: col,
            row,
            modifiers: KeyModifiers::empty(),
        }
    }

    #[test]
    fn mouse_drag_selects_and_copies_only_reader_text() {
        let mut app = seeded_app();
        app.config.theme = "dark".into();
        let mut terminal = Terminal::new(TestBackend::new(100, 24)).unwrap();
        // 1º frame popula a área de leitura.
        terminal.draw(|f| draw(f, &mut app)).unwrap();
        let inner = app.reader_inner.expect("área de leitura conhecida");
        // Seleciona a 2ª linha de versículo (Rm 3:24) — que NÃO é a linha
        // realçada pela lista (cursor no 23), provando o realce da seleção.
        let row = inner.y + 1;
        app.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), inner.x, row));
        app.handle_mouse(mouse(
            MouseEventKind::Drag(MouseButton::Left),
            inner.x + inner.width - 1,
            row,
        ));
        // 2º frame: pinta a seleção e remonta o texto copiável.
        terminal.draw(|f| draw(f, &mut app)).unwrap();

        // Preâmbulo de citação (best-practice) na 1ª linha + versículo no corpo.
        let lines: Vec<&str> = app.selection_text.lines().collect();
        assert_eq!(
            lines.first().copied(),
            Some("Romans 3:24 (KJV)"),
            "1ª linha deveria ser a citação:\n{:?}",
            app.selection_text
        );
        assert!(
            app.selection_text
                .contains("Being justified freely by his grace"),
            "deveria copiar o versículo:\n{:?}",
            app.selection_text
        );
        // Nada de cromo: bordas/painéis vizinhos ficam de fora da seleção.
        for junk in ['│', '╭', '╮', '╰', '╯', '─'] {
            assert!(
                !app.selection_text.contains(junk),
                "a seleção não deve conter o caractere de borda {junk:?}"
            );
        }
        // A célula selecionada recebe o fundo da seleção no buffer renderizado.
        let pal = Palette::resolve("dark");
        let buf = terminal.backend().buffer();
        assert_eq!(
            buf.cell((inner.x, row)).unwrap().bg,
            pal.sel_bg,
            "a célula selecionada deveria ganhar o fundo de seleção"
        );
    }

    #[test]
    fn multi_verse_selection_cites_a_range() {
        let mut app = seeded_app();
        let mut terminal = Terminal::new(TestBackend::new(100, 24)).unwrap();
        terminal.draw(|f| draw(f, &mut app)).unwrap();
        let inner = app.reader_inner.expect("área de leitura conhecida");
        // Arrasta da 1ª linha (Rm 3:23) até a 2ª (Rm 3:24).
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            inner.x,
            inner.y,
        ));
        app.handle_mouse(mouse(
            MouseEventKind::Drag(MouseButton::Left),
            inner.x + inner.width - 1,
            inner.y + 1,
        ));
        terminal.draw(|f| draw(f, &mut app)).unwrap();
        assert!(
            app.selection_text.starts_with("Romans 3:23-24 (KJV)\n"),
            "deveria citar o intervalo:\n{:?}",
            app.selection_text
        );
    }

    #[test]
    fn dragging_over_blank_area_yields_no_citation() {
        let mut app = seeded_app();
        let mut terminal = Terminal::new(TestBackend::new(100, 24)).unwrap();
        terminal.draw(|f| draw(f, &mut app)).unwrap();
        let inner = app.reader_inner.expect("área de leitura conhecida");
        // Linhas em branco abaixo dos 2 versículos do capítulo.
        let blank = inner.y + inner.height - 1;
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            inner.x,
            blank,
        ));
        app.handle_mouse(mouse(
            MouseEventKind::Drag(MouseButton::Left),
            inner.x + inner.width - 1,
            blank,
        ));
        terminal.draw(|f| draw(f, &mut app)).unwrap();
        assert!(
            app.selection_text.trim().is_empty(),
            "área vazia não gera citação solta:\n{:?}",
            app.selection_text
        );
    }

    #[test]
    fn mouse_drag_starting_outside_reader_selects_nothing() {
        let mut app = seeded_app();
        let mut terminal = Terminal::new(TestBackend::new(100, 24)).unwrap();
        terminal.draw(|f| draw(f, &mut app)).unwrap();
        let inner = app.reader_inner.expect("área de leitura conhecida");
        // Começa na lista de livros (coluna 2), bem à esquerda do leitor.
        app.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 2, inner.y));
        app.handle_mouse(mouse(
            MouseEventKind::Drag(MouseButton::Left),
            inner.x + 1,
            inner.y,
        ));
        terminal.draw(|f| draw(f, &mut app)).unwrap();
        assert!(app.selection.is_none(), "começar fora não seleciona");
        assert!(app.selection_text.is_empty());
    }

    #[test]
    fn copy_confirmation_toast_shows_in_footer() {
        let mut app = seeded_app();
        app.notify_copied(true, 42);
        let text = render(&mut app);
        assert!(text.contains("42 caracteres copiados"), "{text}");
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

    // --- Overlays de IA ----------------------------------------------------

    use the_light_core::ai::ChatRole;
    use the_light_core::userdata::Session;

    /// Conversa de teste: 1 pergunta + 1 resposta, refs computadas da resposta.
    fn convo(answer: &str) -> AiPanel {
        let mut s = Session::start(
            "id".into(),
            "T".into(),
            "Romans 3".into(),
            "ctx".into(),
            Lang::En,
            "mock".into(),
            "mock-1".into(),
        );
        s.push(ChatRole::User, "pergunta".into());
        s.push(ChatRole::Assistant, answer.into());
        AiPanel {
            refs: crate::app::cited_refs(answer),
            session: s,
            status: AiStatus::Idle,
            ref_selected: 0,
            scroll: crate::scroll::ScrollState::default(),
            study: None,
            study_done: None,
        }
    }

    #[test]
    fn ai_panel_scrolls_to_end_of_long_answer() {
        let mut app = seeded_app();
        // Resposta longa de parágrafo único: poucas linhas LÓGICAS, muitas linhas
        // VISUAIS após a quebra. O clamp da rolagem precisa usar a altura quebrada
        // (não `lines.len()`), senão o fim fica inalcançável.
        let long = format!("{}ZZSENTINELAZZ", "palavra ".repeat(400));
        let mut panel = convo(&long);
        panel.scroll.jump_to_end(); // pede o fim
        app.ai = Some(panel);
        let text = render(&mut app);
        assert!(
            text.contains("ZZSENTINELAZZ"),
            "rolar até o fim de uma resposta longa deveria revelar o final"
        );
    }

    #[test]
    fn up_arrow_responds_right_after_scrolling_to_end() {
        // Regressão: `submit_ask` rola ao fim (`to_end`); antes, `u16::MAX` grudava
        // e ↑ não respondia. Agora o render normaliza o offset e ↑ sobe na hora.
        let mut app = seeded_app();
        let long = format!("{}ZZFIMZZ", "palavra ".repeat(400));
        let mut panel = convo(&long);
        panel.scroll.jump_to_end();
        app.ai = Some(panel);
        let _ = render(&mut app); // normaliza "fim" → máximo real
        let at_end = app.ai.as_ref().unwrap().scroll.offset();
        assert!(at_end > 0, "o fim virou um offset real, não u16::MAX");
        app.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::empty()));
        let _ = render(&mut app);
        assert_eq!(
            app.ai.as_ref().unwrap().scroll.offset(),
            at_end - 1,
            "↑ deveria subir uma linha imediatamente após ir ao fim"
        );
    }

    #[test]
    fn ai_panel_renders_conversation() {
        let mut app = seeded_app();
        app.ai = Some(convo("Resposta de teste do modelo."));
        let text = render(&mut app);
        assert!(text.contains("Romans 3"), "{text}");
        assert!(text.contains("pergunta"), "{text}");
        assert!(text.contains("Resposta de teste"), "{text}");
        assert!(text.contains("mock-1"), "rodapé com o modelo:\n{text}");
    }

    #[test]
    fn ai_panel_lists_cited_references_for_fast_travel() {
        let mut app = seeded_app();
        app.ai = Some(convo("Veja Romans 6:23 e Romans 3:24."));
        let text = render(&mut app);
        assert!(text.contains("Saltar para"), "{text}");
        assert!(text.contains("[1] Romans 6:23"), "{text}");
        assert!(text.contains("[2] Romans 3:24"), "{text}");
    }

    #[test]
    fn ai_panel_renders_pending_and_error() {
        let mut app = seeded_app();
        let mut panel = convo("ignorada");
        panel.status = AiStatus::Pending;
        app.ai = Some(panel);
        assert!(render(&mut app).contains("consultando"));

        let mut panel = convo("ignorada");
        panel.status = AiStatus::Error("sem chave para `anthropic`".into());
        app.ai = Some(panel);
        assert!(render(&mut app).contains("sem chave"));
    }

    #[test]
    fn sessions_browser_lists_saved_conversations() {
        let mut app = seeded_app();
        let mut s = Session::start(
            "id".into(),
            "sobre a graça".into(),
            "Romans 3".into(),
            "ctx".into(),
            Lang::En,
            "mock".into(),
            "mock-1".into(),
        );
        s.push(ChatRole::User, "q".into());
        app.sessions = Some(crate::app::SessionBrowser {
            items: vec![s],
            selected: 0,
        });
        let text = render(&mut app);
        assert!(text.contains("Conversas"), "{text}");
        assert!(text.contains("sobre a graça"), "{text}");
    }

    #[test]
    fn settings_modal_lists_providers_with_markers() {
        let mut app = seeded_app();
        app.config.provider = "openai".into();
        app.settings = Some(crate::app::Settings {
            selected: 1,
            has_key: vec![true, false, false],
            mode: SettingsMode::List,
        });
        let text = render(&mut app);
        assert!(text.contains("Configurar IA"), "{text}");
        for p in ["anthropic", "openai", "ollama"] {
            assert!(text.contains(p), "faltou {p}:\n{text}");
        }
        assert!(text.contains('★'), "marca de provedor ativo:\n{text}");
        assert!(text.contains('✓'), "marca de chave salva:\n{text}");
    }

    #[test]
    fn settings_modal_masks_key_input() {
        let mut app = seeded_app();
        app.settings = Some(crate::app::Settings {
            selected: 0,
            has_key: vec![false, false, false],
            mode: SettingsMode::EditKey("sk-secret-123".into()),
        });
        let text = render(&mut app);
        assert!(text.contains('•'), "a chave deve ser mascarada:\n{text}");
        assert!(
            !text.contains("sk-secret-123"),
            "a chave crua não pode aparecer:\n{text}"
        );
    }
}
