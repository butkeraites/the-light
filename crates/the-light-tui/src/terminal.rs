//! Guarda RAII do terminal: entra em raw mode + tela alternativa na construção
//! e **sempre** restaura no drop (inclusive durante panic).

use std::io::{stdout, Stdout};

use anyhow::Result;
use ratatui::crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
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
    /// Entra em raw mode + tela alternativa (com captura de mouse) e cria o
    /// terminal. A captura permite a seleção de texto restrita à área de leitura;
    /// para a seleção nativa do terminal, segure ⌥/Option (iTerm2) ou Shift.
    pub fn new() -> Result<Self> {
        enable_raw_mode()?;
        // Se algo falhar após entrar em raw mode (mas antes do guard existir para
        // restaurar no Drop), desfaz tudo para não deixar o terminal corrompido.
        let build = || -> Result<Self> {
            let mut out = stdout();
            execute!(out, EnterAlternateScreen, EnableMouseCapture)?;
            let terminal = Terminal::new(CrosstermBackend::new(out))?;
            Ok(TerminalGuard { terminal })
        };
        build().inspect_err(|_| {
            let _ = disable_raw_mode();
            let _ = execute!(stdout(), DisableMouseCapture, LeaveAlternateScreen);
        })
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(
            self.terminal.backend_mut(),
            DisableMouseCapture,
            LeaveAlternateScreen
        );
        let _ = self.terminal.show_cursor();
    }
}

/// Instala um hook de panic que restaura o terminal antes de imprimir o erro,
/// para que a mensagem fique legível e o terminal não fique corrompido.
pub fn install_panic_hook() {
    let original = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(stdout(), DisableMouseCapture, LeaveAlternateScreen);
        original(info);
    }));
}
