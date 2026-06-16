//! Paleta de cores da TUI (truecolor), resolvida a partir do nome do tema.
//!
//! Mantém uma identidade visual coesa (acento azul/ciano, neutros suaves) e um
//! modo sem cor (`none`/`plain`) que zera os tons mas preserva a estrutura
//! (bordas arredondadas, negrito). É independente do tema ANSI da CLI.

use ratatui::style::{Color, Modifier, Style};

/// Conjunto de cores usado para estilizar a interface.
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    /// `false` em `none`/`plain`: cai para realces por atributo (negrito/inverso/
    /// sublinhado), que funcionam em qualquer terminal sem depender de cor.
    pub colored: bool,
    /// Acento: foco, números de versículo, marca, títulos focados.
    pub accent: Color,
    /// Fundo suave da seleção.
    pub sel_bg: Color,
    /// Texto da seleção.
    pub sel_fg: Color,
    /// Borda de painel sem foco.
    pub border: Color,
    /// Texto secundário (rodapé, metadados, títulos sem foco).
    pub dim: Color,
    /// Realce de busca / avisos.
    pub warn: Color,
}

impl Palette {
    /// Resolve a paleta a partir do nome do tema (`dark`/`light`/`none`/outros).
    pub fn resolve(theme: &str) -> Palette {
        match theme {
            "none" | "plain" => Palette {
                colored: false,
                accent: Color::Reset,
                sel_bg: Color::Reset,
                sel_fg: Color::Reset,
                border: Color::Reset,
                dim: Color::Reset,
                warn: Color::Reset,
            },
            "light" => Palette {
                colored: true,
                accent: Color::Rgb(29, 108, 201),
                sel_bg: Color::Rgb(214, 230, 250),
                sel_fg: Color::Rgb(15, 23, 42),
                border: Color::Rgb(150, 158, 170),
                dim: Color::Rgb(110, 118, 130),
                warn: Color::Rgb(176, 124, 0),
            },
            // dark e padrão
            _ => Palette {
                colored: true,
                accent: Color::Rgb(56, 189, 248),
                sel_bg: Color::Rgb(30, 58, 78),
                sel_fg: Color::Rgb(226, 240, 252),
                border: Color::Rgb(80, 88, 102),
                dim: Color::Rgb(130, 140, 155),
                warn: Color::Rgb(245, 200, 90),
            },
        }
    }

    /// `Style` com a cor de frente `c` (no-op visual quando `!colored`).
    pub fn fg(&self, c: Color) -> Style {
        Style::new().fg(c)
    }

    /// Estilo de borda conforme o foco do painel. Sem cor, o foco vira negrito.
    pub fn border_style(&self, focused: bool) -> Style {
        if self.colored {
            Style::new().fg(if focused { self.accent } else { self.border })
        } else if focused {
            Style::new().add_modifier(Modifier::BOLD)
        } else {
            Style::new()
        }
    }

    /// Estilo do título de um painel conforme o foco (sempre negrito).
    pub fn title_style(&self, focused: bool) -> Style {
        let s = Style::new().add_modifier(Modifier::BOLD);
        if self.colored {
            s.fg(if focused { self.accent } else { self.dim })
        } else {
            s
        }
    }

    /// Estilo da linha selecionada. Sem cor, usa vídeo invertido (universal).
    pub fn selection_style(&self) -> Style {
        if self.colored {
            Style::new()
                .fg(self.sel_fg)
                .bg(self.sel_bg)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::new().add_modifier(Modifier::REVERSED | Modifier::BOLD)
        }
    }

    /// Estilo do trecho casado na busca. Sem cor, negrito + sublinhado.
    pub fn match_style(&self) -> Style {
        if self.colored {
            Style::new().fg(self.warn).add_modifier(Modifier::BOLD)
        } else {
            Style::new().add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn none_theme_is_uncolored() {
        let p = Palette::resolve("none");
        assert_eq!(p.accent, Color::Reset);
        assert_eq!(p.border, Color::Reset);
    }

    #[test]
    fn dark_and_light_are_distinct() {
        let d = Palette::resolve("dark");
        let l = Palette::resolve("light");
        assert_ne!(d.accent, l.accent);
        // Tema desconhecido cai no escuro (padrão).
        assert_eq!(Palette::resolve("auto").accent, d.accent);
    }
}
