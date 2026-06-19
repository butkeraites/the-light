//! `the-light-tui` — interface de terminal baseada em `ratatui`.
//!
//! Ponto de entrada: [`run`]. O terminal é restaurado automaticamente ao sair
//! (inclusive em panic), via [`terminal::TerminalGuard`] + hook de panic.

mod app;
mod clipboard;
mod scroll;
mod terminal;
mod theme;
mod ui;

pub use app::{App, Focus};

use std::path::PathBuf;
use std::time::Duration;

use anyhow::Result;
use ratatui::crossterm::event::{self, Event, KeyEventKind};

use the_light_core::model::TranslationId;
use the_light_core::store::Store;

/// Cadência do loop: pausa por evento antes de redesenhar. Mantém o spinner da
/// IA animado e drena o canal da consulta sem bloquear o teclado.
const TICK: Duration = Duration::from_millis(80);

/// Abre a TUI para leitura, consumindo o `Store` aberto. As demais versões
/// disponíveis são descobertas a partir do banco. `db_path` (`None` = padrão) é
/// usado para reabrir o banco numa thread ao instalar os dados acadêmicos.
pub fn run(store: Store, version: TranslationId, db_path: Option<PathBuf>) -> Result<()> {
    terminal::install_panic_hook();
    let mut guard = terminal::TerminalGuard::new()?;
    let mut app = App::new(store, version, db_path)?;
    app.load_userdata();

    while !app.should_quit {
        guard.terminal.draw(|frame| ui::draw(frame, &mut app))?;
        // Cópia diferida: o `draw` acima remonta `selection_text` (com a citação)
        // a partir do buffer fresco; só então copiamos. O IO da área de
        // transferência fica aqui para manter `app` como lógica pura/testável.
        if app.take_copy_request() {
            let text = app.selection_text.clone();
            if !text.trim().is_empty() {
                let chars = text.chars().count();
                let ok = clipboard::copy(&text);
                app.notify_copied(ok, chars);
            }
        }
        // Recebe o resultado de uma consulta de IA em andamento, se houver.
        app.poll_ai();
        // Recebe uma rodada de refinamento ou o estudo final, se houver.
        app.poll_study();
        // Recebe o progresso da instalação dos dados acadêmicos, se houver.
        app.poll_scholarly();
        // `poll` em vez de `read` para não congelar enquanto a IA responde:
        // sem evento, avança o spinner e volta a desenhar. Quando há eventos,
        // **drena todos os pendentes** antes de redesenhar (um `draw` por lote):
        // a captura de mouse inunda o loop com eventos de roda/movimento e
        // redesenhar a cada um faz a fila crescer sem fim (a app "trava").
        if event::poll(TICK)? {
            loop {
                match event::read()? {
                    Event::Key(key) if key.kind == KeyEventKind::Press => app.handle_key(key),
                    // Seleção de texto via mouse, restrita à área de leitura.
                    Event::Mouse(m) => app.handle_mouse(m),
                    _ => {}
                }
                // Sai assim que a fila esvazia (sem bloquear) — então o topo do
                // loop redesenha uma única vez para o lote inteiro.
                if !event::poll(Duration::ZERO)? {
                    break;
                }
            }
        } else {
            app.tick();
        }
    }
    app.save_config();
    Ok(())
}
