mod agents;
mod app;
mod events;
mod resume;
mod sessions;
mod ui;

use app::{App, Panel};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, MouseEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;
use std::io;
use std::time::{Duration, Instant};

fn main() -> io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();
    let tick_rate = Duration::from_secs(3);
    let scroll_throttle = Duration::from_millis(50);
    let mut last_scroll = Instant::now() - scroll_throttle;

    loop {
        terminal.draw(|f| ui::draw(f, &mut app))?;

        if event::poll(tick_rate)? {
            match event::read()? {
                Event::Key(key) => {
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }
                    // Clear status message on any key press
                    app.clear_status_message();

                    // Handle delete confirmation mode
                    if app.confirm_delete {
                        match key.code {
                            KeyCode::Char('y') => app.confirm_delete_yes(),
                            _ => app.cancel_delete(),
                        }
                        continue;
                    }

                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => {
                            app.should_quit = true;
                        }
                        KeyCode::Tab | KeyCode::BackTab => app.toggle_panel(),
                        KeyCode::Down | KeyCode::Char('j') => match app.focused_panel {
                            Panel::Sessions => app.select_next(),
                            Panel::Events => app.scroll_events_down(),
                        },
                        KeyCode::Up | KeyCode::Char('k') => match app.focused_panel {
                            Panel::Sessions => app.select_prev(),
                            Panel::Events => app.scroll_events_up(),
                        },
                        KeyCode::Char('r') => app.refresh(),
                        KeyCode::Char('d') => app.toggle_details(),
                        KeyCode::Char('D') => app.request_delete(),
                        KeyCode::Enter => {
                            if let Some(session) = app.selected_session().cloned() {
                                let info = agents::lookup(&session.agent_type);
                                let can_resume = info
                                    .map(|i| matches!(i.resume, agents::ResumePattern::CliFlag { .. }))
                                    .unwrap_or(false);

                                if can_resume {
                                    // Cleanup terminal before exec
                                    disable_raw_mode()?;
                                    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
                                    terminal.show_cursor()?;

                                    match resume::exec_resume(&session) {
                                        Err(e) => {
                                            eprintln!("{}", e);
                                            enable_raw_mode()?;
                                            execute!(io::stdout(), EnterAlternateScreen, EnableMouseCapture)?;
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
                Event::Mouse(mouse) => {
                    if last_scroll.elapsed() < scroll_throttle {
                        continue;
                    }
                    let term_width = terminal.size()?.width;
                    let left_boundary = (term_width as f64 * 0.4) as u16;
                    let is_left_panel = mouse.column < left_boundary;

                    match mouse.kind {
                        MouseEventKind::ScrollDown => {
                            last_scroll = Instant::now();
                            if is_left_panel {
                                app.select_next();
                            } else {
                                app.scroll_events_down();
                            }
                        }
                        MouseEventKind::ScrollUp => {
                            last_scroll = Instant::now();
                            if is_left_panel {
                                app.select_prev();
                            } else {
                                app.scroll_events_up();
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
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
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;

    Ok(())
}
