mod agents;
mod app;
mod events;
mod resume;
mod sessions;
mod ui;

use app::App;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;
use std::io;
use std::time::Duration;

fn main() -> io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();
    let tick_rate = Duration::from_secs(3);

    loop {
        terminal.draw(|f| ui::draw(f, &app))?;

        if event::poll(tick_rate)? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                // Clear status message on any key press
                app.clear_status_message();
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => {
                        app.should_quit = true;
                    }
                    KeyCode::Down | KeyCode::Char('j') => app.select_next(),
                    KeyCode::Up | KeyCode::Char('k') => app.select_prev(),
                    KeyCode::Char('r') => app.refresh(),
                    KeyCode::Char('d') => app.toggle_details(),
                    KeyCode::Enter => {
                        if let Some(session) = app.selected_session().cloned() {
                            let info = agents::lookup(&session.agent_type);
                            let can_resume = info
                                .map(|i| matches!(i.resume, agents::ResumePattern::CliFlag { .. }))
                                .unwrap_or(false);

                            if can_resume {
                                // Cleanup terminal before exec
                                disable_raw_mode()?;
                                execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
                                terminal.show_cursor()?;

                                match resume::exec_resume(&session) {
                                    Err(e) => {
                                        eprintln!("{}", e);
                                        enable_raw_mode()?;
                                        execute!(io::stdout(), EnterAlternateScreen)?;
                                    }
                                    Ok(_) => unreachable!(),
                                }
                            } else {
                                app.set_status_message(
                                    format!("{} does not support resume yet", session.agent_type),
                                );
                            }
                        }
                    }
                    _ => {}
                }
            }
        } else {
            // Tick: auto-refresh sessions
            app.refresh();
        }

        if app.should_quit {
            break;
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}
