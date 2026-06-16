//! Guarda RAII do terminal: entra em raw mode + tela alternativa na construção
//! e **sempre** restaura no drop (inclusive durante panic).

use std::io::{stdout, Stdout};

use anyhow::Result;
use ratatui::crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::{CrosstermBackend, Terminal};

/// Terminal em modo TUI, restaurado automaticamente ao sair de escopo.
pub struct TerminalGuard {
    /// Terminal ratatui sobre o backend crossterm.
    pub terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl TerminalGuard {
    /// Entra em raw mode + tela alternativa e cria o terminal.
    pub fn new() -> Result<Self> {
        enable_raw_mode()?;
        let mut out = stdout();
        execute!(out, EnterAlternateScreen)?;
        let terminal = Terminal::new(CrosstermBackend::new(out))?;
        Ok(TerminalGuard { terminal })
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(self.terminal.backend_mut(), LeaveAlternateScreen);
        let _ = self.terminal.show_cursor();
    }
}

/// Instala um hook de panic que restaura o terminal antes de imprimir o erro,
/// para que a mensagem fique legível e o terminal não fique corrompido.
pub fn install_panic_hook() {
    let original = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(stdout(), LeaveAlternateScreen);
        original(info);
    }));
}
