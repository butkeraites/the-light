//! `the-light-tui` — interface de terminal baseada em `ratatui`.
//!
//! Ponto de entrada: [`run`]. O terminal é restaurado automaticamente ao sair
//! (inclusive em panic), via [`terminal::TerminalGuard`] + hook de panic.

mod app;
mod terminal;
mod theme;
mod ui;

pub use app::{App, Focus};

use std::time::Duration;

use anyhow::Result;
use ratatui::crossterm::event::{self, Event, KeyEventKind};

use the_light_core::model::TranslationId;
use the_light_core::store::Store;

/// Cadência do loop: pausa por evento antes de redesenhar. Mantém o spinner da
/// IA animado e drena o canal da consulta sem bloquear o teclado.
const TICK: Duration = Duration::from_millis(80);

/// Abre a TUI para leitura, consumindo o `Store` aberto. As demais versões
/// disponíveis são descobertas a partir do banco.
pub fn run(store: Store, version: TranslationId) -> Result<()> {
    terminal::install_panic_hook();
    let mut guard = terminal::TerminalGuard::new()?;
    let mut app = App::new(store, version)?;
    app.load_userdata();

    while !app.should_quit {
        guard.terminal.draw(|frame| ui::draw(frame, &mut app))?;
        // Recebe o resultado de uma consulta de IA em andamento, se houver.
        app.poll_ai();
        // `poll` em vez de `read` para não congelar enquanto a IA responde:
        // sem evento, avança o spinner e volta a desenhar.
        if event::poll(TICK)? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    app.handle_key(key);
                }
            }
        } else {
            app.tick();
        }
    }
    app.save_config();
    Ok(())
}
