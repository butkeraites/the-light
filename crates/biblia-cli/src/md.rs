//! Render mínimo de Markdown para o terminal (via `pulldown-cmark`).
//!
//! Cobre o essencial de notas: títulos, ênfase, listas, citações, código e
//! regras. Em modo `--plain`/pipe, a estrutura (listas, parágrafos) é mantida e
//! a ênfase vira texto puro.

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

use crate::theme::Style;

/// Converte Markdown em texto para terminal, aplicando `style`.
pub fn render_markdown(src: &str, style: &Style) -> String {
    let parser = Parser::new_ext(src, Options::ENABLE_STRIKETHROUGH);
    let mut out = String::new();
    let mut bold = 0usize;
    let mut italic = 0usize;
    let mut list_depth = 0usize;
    let mut in_code_block = false;
    let mut in_quote = false;

    for ev in parser {
        match ev {
            Event::Start(Tag::Heading { .. }) => {
                ensure_newline(&mut out);
                bold += 1;
            }
            Event::End(TagEnd::Heading(_)) => {
                bold = bold.saturating_sub(1);
                out.push_str("\n\n");
            }
            Event::End(TagEnd::Paragraph) => out.push_str("\n\n"),
            Event::Start(Tag::Strong) => bold += 1,
            Event::End(TagEnd::Strong) => bold = bold.saturating_sub(1),
            Event::Start(Tag::Emphasis) => italic += 1,
            Event::End(TagEnd::Emphasis) => italic = italic.saturating_sub(1),
            Event::Start(Tag::List(_)) => list_depth += 1,
            Event::End(TagEnd::List(_)) => {
                list_depth = list_depth.saturating_sub(1);
                if list_depth == 0 {
                    ensure_newline(&mut out);
                }
            }
            Event::Start(Tag::Item) => {
                out.push_str(&"  ".repeat(list_depth.max(1)));
                out.push_str("• ");
            }
            Event::End(TagEnd::Item) => out.push('\n'),
            Event::Start(Tag::BlockQuote(_)) => {
                in_quote = true;
                ensure_newline(&mut out);
                out.push_str("│ ");
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                in_quote = false;
                out.push('\n');
            }
            Event::Start(Tag::CodeBlock(_)) => {
                in_code_block = true;
                ensure_newline(&mut out);
            }
            Event::End(TagEnd::CodeBlock) => {
                in_code_block = false;
                out.push('\n');
            }
            Event::Code(t) => out.push_str(&style.dim(&t)),
            Event::Text(t) => {
                if in_code_block {
                    let indented = t
                        .lines()
                        .map(|l| format!("    {l}"))
                        .collect::<Vec<_>>()
                        .join("\n");
                    out.push_str(&style.dim(&indented));
                } else {
                    let mut s = t.to_string();
                    if bold > 0 {
                        s = style.bold(&s);
                    }
                    if italic > 0 {
                        s = style.italic(&s);
                    }
                    out.push_str(&s);
                }
            }
            Event::SoftBreak | Event::HardBreak => {
                out.push('\n');
                if in_quote {
                    out.push_str("│ ");
                }
            }
            Event::Rule => {
                ensure_newline(&mut out);
                out.push_str("───\n");
            }
            _ => {}
        }
    }

    let mut result = out.trim_end().to_string();
    result.push('\n');
    result
}

fn ensure_newline(out: &mut String) {
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_headings_lists_and_emphasis_in_plain() {
        let md = "# Título\n\nUm **forte** e _ênfase_.\n\n- item um\n- item dois\n";
        let out = render_markdown(md, &Style::plain());
        assert!(out.contains("Título"));
        assert!(out.contains("forte"));
        assert!(out.contains("ênfase"));
        assert!(out.contains("• item um"));
        assert!(out.contains("• item dois"));
        // Sem ANSI em modo plain.
        assert!(!out.contains('\u{1b}'));
        // Termina com exatamente uma quebra.
        assert!(out.ends_with('\n') && !out.ends_with("\n\n"));
    }

    #[test]
    fn renders_code_and_quote() {
        let md = "> citação\n\n`codigo`\n";
        let out = render_markdown(md, &Style::plain());
        assert!(out.contains("citação"));
        assert!(out.contains("codigo"));
    }

    #[test]
    fn empty_input_yields_single_newline() {
        assert_eq!(render_markdown("", &Style::plain()), "\n");
    }
}
