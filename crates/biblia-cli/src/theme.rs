//! Estilo/cores ANSI da saída.
//!
//! Regra de ouro de alinhamento: só aplicamos cor a strings cuja **largura
//! visível já é final** (números de versículo já justificados, células já
//! preenchidas). Assim os códigos ANSI (largura zero) nunca quebram colunas.
//!
//! Cor é desativada quando: `--plain`, `NO_COLOR` setado, saída não é TTY, ou
//! `theme = none`. Convenção <https://no-color.org/> respeitada.

use std::io::IsTerminal;

use biblia_core::search::{HL_END, HL_START};

/// Estilo de renderização (cores on/off + paleta).
#[derive(Debug, Clone, Copy)]
pub struct Style {
    enabled: bool,
    light: bool,
}

impl Style {
    /// Estilo sem cor (usado em testes de renderização).
    #[cfg(test)]
    pub fn plain() -> Self {
        Style {
            enabled: false,
            light: false,
        }
    }

    /// Resolve o estilo a partir de `--plain`, `NO_COLOR`, do TTY e do tema.
    pub fn resolve(plain: bool, theme: &str) -> Self {
        let no_color = std::env::var_os("NO_COLOR").is_some();
        let theme_off = theme.eq_ignore_ascii_case("none") || theme.eq_ignore_ascii_case("plain");
        let enabled = !plain && !no_color && !theme_off && std::io::stdout().is_terminal();
        Style {
            enabled,
            light: theme.eq_ignore_ascii_case("light"),
        }
    }

    /// `true` se as cores estão ativas.
    #[cfg(test)]
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    fn paint(&self, code: &str, s: &str) -> String {
        if self.enabled {
            format!("\x1b[{code}m{s}\x1b[0m")
        } else {
            s.to_string()
        }
    }

    /// Número de versículo (ciano no escuro, azul no claro).
    pub fn verse_number(&self, s: &str) -> String {
        self.paint(if self.light { "34" } else { "36" }, s)
    }

    /// Cabeçalho de referência (negrito).
    pub fn reference(&self, s: &str) -> String {
        self.paint("1", s)
    }

    /// Rótulo de versão (verde no escuro, magenta no claro).
    pub fn label(&self, s: &str) -> String {
        self.paint(if self.light { "35" } else { "32" }, s)
    }

    /// Converte os marcadores de destaque da busca em cor (ou colchetes, sem cor).
    pub fn highlight(&self, s: &str) -> String {
        if self.enabled {
            s.replace(HL_START, "\x1b[1;33m").replace(HL_END, "\x1b[0m")
        } else {
            s.replace(HL_START, "[").replace(HL_END, "]")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_style_emits_no_ansi() {
        let s = Style::plain();
        assert!(!s.enabled());
        assert_eq!(s.verse_number("16"), "16");
        assert_eq!(s.reference("John 3:16"), "John 3:16");
        assert_eq!(s.highlight(&format!("{HL_START}graça{HL_END}")), "[graça]");
    }

    #[test]
    fn theme_none_disables_color() {
        // Mesmo forçando, theme=none desliga (independe de TTY).
        let s = Style::resolve(false, "none");
        assert!(!s.enabled());
    }

    #[test]
    fn enabled_style_wraps_with_ansi() {
        // Constrói um estilo habilitado diretamente (sem depender de TTY).
        let s = Style {
            enabled: true,
            light: false,
        };
        assert!(s.verse_number("16").starts_with("\x1b["));
        assert!(s.verse_number("16").ends_with("\x1b[0m"));
        assert!(s
            .highlight(&format!("{HL_START}x{HL_END}"))
            .contains("\x1b[1;33m"));
    }
}
