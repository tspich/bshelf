mod app;
mod events;
mod keybindings;
mod ui;

use crossterm::{
    event::{self, Event},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use anyhow::Result;

use bshelf::load_config;
use app::App;

// NOTE:
//  - Get DOI from PDF can be very tedious, need to be careful
//
// TODO: 
//  - use title and authors for refetch_metadata if doi missing
//  - Using direct link to pdf, download the pdf and store it as {doi}.pdf
//  - import bib multiple and single need testing.
//

fn main() -> Result<()> {
    let config = load_config();

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(config);
    app.load_references();

    terminal.clear().ok();

    loop {
        // --- draw UI ---
        terminal.draw(|f| ui::draw(f, &mut app))?;

        if crossterm::event::poll(std::time::Duration::from_millis(200))? {
            if let Event::Key(key) = event::read()? {
                if events::handle_key(&mut app, key.code, &mut terminal) {
                    break;
                }
            }
        }

        app.clear_expired_alert();  

    } // end loop

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}
