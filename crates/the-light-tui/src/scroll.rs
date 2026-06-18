//! Parágrafo rolável: um módulo **dono do offset de rolagem E do clamp contra o
//! conteúdo já quebrado**. Mantê-los juntos elimina o bug clássico de TUI —
//! calcular o máximo de rolagem por linhas *lógicas* enquanto o widget quebra o
//! texto em mais linhas *visuais*, deixando o fim inalcançável.

use ratatui::layout::Rect;
use ratatui::text::Text;
use ratatui::widgets::{Block, Paragraph, Wrap};
use ratatui::Frame;

/// Estado de rolagem de um parágrafo (offset em linhas visuais a partir do topo).
///
/// O offset é **intenção**: as setas/roda apenas o empurram, e `jump_to_end()`
/// marca `u16::MAX`. É [`render`](Self::render) que conhece a altura já quebrada e
/// normaliza o offset para o máximo real — então "ir ao fim" seguido de uma seta
/// pra cima responde de imediato (em vez de ficar preso no fundo).
#[derive(Debug, Default, Clone, Copy)]
pub struct ScrollState {
    offset: u16,
}

impl ScrollState {
    /// Offset efetivo do último render (0 = topo).
    pub fn offset(&self) -> u16 {
        self.offset
    }

    /// Desce `n` linhas (o clamp acontece no próximo render).
    pub fn down(&mut self, n: u16) {
        self.offset = self.offset.saturating_add(n);
    }

    /// Sobe `n` linhas.
    pub fn up(&mut self, n: u16) {
        self.offset = self.offset.saturating_sub(n);
    }

    /// Salta para o fim (clampado de fato no próximo render).
    pub fn jump_to_end(&mut self) {
        self.offset = u16::MAX;
    }

    /// Renderiza `text` dentro de `block`/`area`, **rolado e grampeado** ao
    /// conteúdo já quebrado. `reserve_bottom` são linhas do fundo do bloco que o
    /// chamador vai sobrescrever (ex.: um rodapé) e que não contam como corpo
    /// visível. Normaliza o offset para o máximo real (efeito colateral
    /// proposital: deixa "fim" + setas responsivos).
    pub fn render<'a>(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        block: Block<'a>,
        text: Text<'a>,
        reserve_bottom: u16,
    ) {
        let inner = block.inner(area);
        let para = Paragraph::new(text).wrap(Wrap { trim: false });
        let body_h = inner.height.saturating_sub(reserve_bottom).max(1);
        let max = (para.line_count(inner.width) as u16).saturating_sub(body_h);
        self.offset = self.offset.min(max);
        frame.render_widget(para.scroll((self.offset, 0)).block(block), area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    /// Renderiza `lines` linhas curtas num backend `20 × h` (bloco com rodapé de
    /// 1 linha reservada) para exercitar o clamp real.
    fn render(state: &mut ScrollState, lines: usize, h: u16) {
        let body = (0..lines)
            .map(|i| format!("linha {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let mut term = Terminal::new(TestBackend::new(20, h)).unwrap();
        term.draw(|f| {
            let area = f.area();
            state.render(f, area, Block::bordered(), Text::from(body.clone()), 1);
        })
        .unwrap();
    }

    #[test]
    fn jump_to_end_normalizes_to_real_max_and_up_responds() {
        let mut s = ScrollState::default();
        s.jump_to_end();
        render(&mut s, 40, 10); // 40 linhas, corpo visível ~7
        let at_end = s.offset();
        assert!(
            at_end > 0 && at_end < 40,
            "fim virou um máximo real: {at_end}"
        );
        // Antes da normalização, ↑ ficava preso no fundo; agora responde.
        s.up(1);
        assert_eq!(s.offset(), at_end - 1);
    }

    #[test]
    fn short_content_clamps_to_top() {
        let mut s = ScrollState::default();
        s.down(50);
        render(&mut s, 2, 20); // cabe folgado
        assert_eq!(s.offset(), 0, "conteúdo curto não rola");
    }

    #[test]
    fn cannot_scroll_past_the_end() {
        let mut s = ScrollState::default();
        s.down(10_000);
        render(&mut s, 30, 8);
        let max = s.offset();
        // Mais um empurrão e novo render não passa do mesmo máximo.
        s.down(10_000);
        render(&mut s, 30, 8);
        assert_eq!(s.offset(), max);
    }
}
