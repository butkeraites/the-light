//! `biblia-tui` — interface de terminal baseada em `ratatui`.
//!
//! Ponto de entrada: [`run`]. O terminal é restaurado automaticamente ao sair
//! (inclusive em panic), via [`terminal::TerminalGuard`] + hook de panic.

mod app;
mod terminal;
mod ui;

pub use app::{App, Focus};

use anyhow::Result;
use ratatui::crossterm::event::{self, Event, KeyEventKind};

use biblia_core::model::TranslationId;
use biblia_core::store::Store;

/// Abre a TUI para leitura, consumindo o `Store` aberto. As demais versões
/// disponíveis são descobertas a partir do banco.
pub fn run(store: Store, version: TranslationId) -> Result<()> {
    terminal::install_panic_hook();
    let mut guard = terminal::TerminalGuard::new()?;
    let mut app = App::new(store, version)?;

    while !app.should_quit {
        guard.terminal.draw(|frame| ui::draw(frame, &mut app))?;
        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => app.handle_key(key),
            _ => {}
        }
    }
    Ok(())
}
