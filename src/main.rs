mod acpx_control;
mod agents;
mod app;
mod events;
mod launcher;
mod prompt_editor;
mod prompt_history;
mod resume;
mod sessions;
mod ui;

use app::{App, Panel};
use crossterm::{
    event::{
        self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
        Event, KeyCode, KeyEventKind, KeyModifiers, KeyboardEnhancementFlags, MouseEventKind,
        PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
    },
    execute,
    terminal::{
        disable_raw_mode, enable_raw_mode, supports_keyboard_enhancement, EnterAlternateScreen,
        LeaveAlternateScreen,
    },
};
use ratatui::prelude::*;
use std::io;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PromptKeyAction {
    Cancel,
    Newline,
    Submit,
    MoveUpOrHistory,
    MoveDownOrHistory,
    MoveLeft,
    MoveRight,
    MoveLineStart,
    MoveLineEnd,
    Backspace,
    Delete,
    DeleteLineStart,
    DeleteBufferStart,
    DeleteLineEnd,
    DeleteWordBefore,
    Insert(char),
    Ignore,
}

fn prompt_key_action(key: crossterm::event::KeyEvent) -> PromptKeyAction {
    let has_command = key
        .modifiers
        .intersects(KeyModifiers::SUPER | KeyModifiers::META);
    match key.code {
        KeyCode::Esc => PromptKeyAction::Cancel,
        KeyCode::Enter if key.modifiers.contains(KeyModifiers::SHIFT) => PromptKeyAction::Newline,
        // Crossterm maps LF (0x0a) to Ctrl+J on Unix. Some terminals emit
        // Shift+Enter as LF, so handle it as the composer newline shortcut.
        KeyCode::Char('j') | KeyCode::Char('J')
            if key.modifiers.contains(KeyModifiers::CONTROL) =>
        {
            PromptKeyAction::Newline
        }
        KeyCode::Enter => PromptKeyAction::Submit,
        KeyCode::Char('a') | KeyCode::Char('A')
            if key.modifiers.contains(KeyModifiers::CONTROL) =>
        {
            PromptKeyAction::MoveLineStart
        }
        KeyCode::Char('e') | KeyCode::Char('E')
            if key.modifiers.contains(KeyModifiers::CONTROL) =>
        {
            PromptKeyAction::MoveLineEnd
        }
        KeyCode::Char('u') | KeyCode::Char('U')
            if key.modifiers.contains(KeyModifiers::CONTROL) =>
        {
            PromptKeyAction::DeleteBufferStart
        }
        KeyCode::Char('k') | KeyCode::Char('K')
            if key.modifiers.contains(KeyModifiers::CONTROL) =>
        {
            PromptKeyAction::DeleteLineEnd
        }
        KeyCode::Char('w') | KeyCode::Char('W')
            if key.modifiers.contains(KeyModifiers::CONTROL) =>
        {
            PromptKeyAction::DeleteWordBefore
        }
        KeyCode::Up => PromptKeyAction::MoveUpOrHistory,
        KeyCode::Down => PromptKeyAction::MoveDownOrHistory,
        KeyCode::Left => PromptKeyAction::MoveLeft,
        KeyCode::Right => PromptKeyAction::MoveRight,
        KeyCode::Backspace if has_command => PromptKeyAction::DeleteLineStart,
        KeyCode::Backspace
            if key
                .modifiers
                .intersects(KeyModifiers::ALT | KeyModifiers::CONTROL) =>
        {
            PromptKeyAction::DeleteWordBefore
        }
        KeyCode::Backspace => PromptKeyAction::Backspace,
        KeyCode::Delete => PromptKeyAction::Delete,
        KeyCode::Char(c)
            if !key.modifiers.intersects(
                KeyModifiers::CONTROL
                    | KeyModifiers::ALT
                    | KeyModifiers::SUPER
                    | KeyModifiers::META,
            ) =>
        {
            PromptKeyAction::Insert(c)
        }
        _ => PromptKeyAction::Ignore,
    }
}

fn main() -> io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(
        stdout,
        EnterAlternateScreen,
        EnableMouseCapture,
        EnableBracketedPaste
    )?;
    let keyboard_enhancement_enabled = matches!(supports_keyboard_enhancement(), Ok(true));
    if keyboard_enhancement_enabled {
        execute!(
            stdout,
            PushKeyboardEnhancementFlags(keyboard_enhancement_flags())
        )?;
    }
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();
    let (prompt_result_tx, prompt_result_rx) = mpsc::channel::<Result<String, String>>();
    let (create_result_tx, create_result_rx) = mpsc::channel::<Result<String, String>>();
    let tick_rate = Duration::from_millis(250);
    let scroll_throttle = Duration::from_millis(50);
    let mut last_scroll = Instant::now() - scroll_throttle;

    loop {
        while let Ok(result) = prompt_result_rx.try_recv() {
            app.complete_prompt_send(result);
        }
        while let Ok(result) = create_result_rx.try_recv() {
            app.complete_session_create(result);
        }

        terminal.draw(|f| ui::draw(f, &mut app))?;

        if event::poll(tick_rate)? {
            match event::read()? {
                Event::Key(key) => {
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }

                    if key.code == KeyCode::Char('c')
                        && key.modifiers.contains(KeyModifiers::CONTROL)
                    {
                        if app.launcher_is_active() {
                            app.cancel_launcher();
                        } else if app.focused_panel == Panel::Prompt {
                            app.handle_prompt_interrupt();
                        } else {
                            app.should_quit = true;
                        }
                        continue;
                    }

                    if app.launcher_is_active() {
                        match key.code {
                            KeyCode::Esc => app.cancel_launcher(),
                            KeyCode::Enter => {
                                if let Some(request) = app.confirm_launcher() {
                                    let tx = create_result_tx.clone();
                                    thread::spawn(move || {
                                        let agent = request.agent;
                                        let cwd = request.cwd;
                                        let result = acpx_control::create_session(&agent, &cwd)
                                            .map(|r| r.summary(&agent))
                                            .map_err(|e| e.to_string());
                                        let _ = tx.send(result);
                                    });
                                }
                            }
                            KeyCode::Down | KeyCode::Char('j') => app.select_next_launcher_item(),
                            KeyCode::Up | KeyCode::Char('k') => app.select_prev_launcher_item(),
                            KeyCode::Backspace => app.backspace_launcher(),
                            KeyCode::Char(c)
                                if !key.modifiers.contains(KeyModifiers::CONTROL)
                                    && !key.modifiers.contains(KeyModifiers::ALT) =>
                            {
                                app.push_launcher_char(c);
                            }
                            _ => {}
                        }
                        continue;
                    }

                    if app.confirm_delete {
                        match key.code {
                            KeyCode::Char('y') => app.confirm_delete_yes(),
                            _ => app.cancel_delete(),
                        }
                        continue;
                    }

                    if matches!(key.code, KeyCode::Tab | KeyCode::BackTab) {
                        app.clear_status_message();
                        app.toggle_panel();
                        continue;
                    }

                    if app.focused_panel == Panel::Prompt {
                        match prompt_key_action(key) {
                            PromptKeyAction::Cancel => app.cancel_prompt_input(),
                            PromptKeyAction::Newline => app.insert_prompt_newline(),
                            PromptKeyAction::Submit => {
                                if let Some((session, prompt)) = app.submit_prompt_input() {
                                    let tx = prompt_result_tx.clone();
                                    thread::spawn(move || {
                                        let result = acpx_control::send_prompt(&session, &prompt)
                                            .map(|r| r.summary())
                                            .map_err(|e| e.to_string());
                                        let _ = tx.send(result);
                                    });
                                }
                            }
                            PromptKeyAction::MoveUpOrHistory => app.prompt_up(),
                            PromptKeyAction::MoveDownOrHistory => app.prompt_down(),
                            PromptKeyAction::MoveLeft => app.move_prompt_left(),
                            PromptKeyAction::MoveRight => app.move_prompt_right(),
                            PromptKeyAction::MoveLineStart => app.move_prompt_to_line_start(),
                            PromptKeyAction::MoveLineEnd => app.move_prompt_to_line_end(),
                            PromptKeyAction::Backspace => app.backspace_prompt(),
                            PromptKeyAction::Delete => app.delete_prompt_after_cursor(),
                            PromptKeyAction::DeleteLineStart => app.delete_prompt_to_line_start(),
                            PromptKeyAction::DeleteBufferStart => {
                                app.delete_prompt_to_buffer_start()
                            }
                            PromptKeyAction::DeleteLineEnd => app.delete_prompt_to_line_end(),
                            PromptKeyAction::DeleteWordBefore => app.delete_prompt_word_before(),
                            PromptKeyAction::Insert(c) => app.push_prompt_char(c),
                            PromptKeyAction::Ignore => {}
                        }
                        continue;
                    }

                    // Clear status message on normal-mode key press
                    app.clear_status_message();

                    match key.code {
                        KeyCode::Esc => {
                            app.should_quit = true;
                        }
                        KeyCode::Down | KeyCode::Char('j') => match app.focused_panel {
                            Panel::Sessions => app.select_next(),
                            Panel::Events => app.scroll_events_down(),
                            Panel::Prompt => {}
                        },
                        KeyCode::Up | KeyCode::Char('k') => match app.focused_panel {
                            Panel::Sessions => app.select_prev(),
                            Panel::Events => app.scroll_events_up(),
                            Panel::Prompt => {}
                        },
                        KeyCode::Char('r') => app.refresh(),
                        KeyCode::Char('n') => match acpx_control::discover_registered_agents() {
                            Ok(agents) => {
                                let current_dir = std::env::current_dir()
                                    .unwrap_or_else(|_| std::path::PathBuf::from("."));
                                let roots = launcher::default_directory_roots(&current_dir);
                                app.open_launcher(agents, roots);
                            }
                            Err(error) => {
                                app.set_status_message(format!(
                                    "Agent discovery failed: {}",
                                    error
                                ));
                            }
                        },
                        KeyCode::Char('d') => app.toggle_details(),
                        KeyCode::Char('D') => app.request_delete(),
                        KeyCode::Char('i') | KeyCode::Char('s') => app.start_prompt_input(),
                        KeyCode::Enter => {
                            if let Some(session) = app.selected_session().cloned() {
                                let info = agents::lookup(&session.agent_type);
                                let can_resume = info
                                    .map(|i| {
                                        !matches!(i.resume, agents::ResumePattern::Unsupported)
                                    })
                                    .unwrap_or(false);

                                if can_resume {
                                    // Cleanup terminal before exec
                                    disable_raw_mode()?;
                                    if keyboard_enhancement_enabled {
                                        execute!(
                                            terminal.backend_mut(),
                                            PopKeyboardEnhancementFlags
                                        )?;
                                    }
                                    execute!(
                                        terminal.backend_mut(),
                                        LeaveAlternateScreen,
                                        DisableMouseCapture,
                                        DisableBracketedPaste
                                    )?;
                                    terminal.show_cursor()?;

                                    match resume::exec_resume(&session) {
                                        Err(e) => {
                                            eprintln!("{}", e);
                                            enable_raw_mode()?;
                                            execute!(
                                                io::stdout(),
                                                EnterAlternateScreen,
                                                EnableMouseCapture,
                                                EnableBracketedPaste
                                            )?;
                                            if keyboard_enhancement_enabled {
                                                execute!(
                                                    terminal.backend_mut(),
                                                    PushKeyboardEnhancementFlags(
                                                        keyboard_enhancement_flags()
                                                    )
                                                )?;
                                            }
                                        }
                                        Ok(_) => unreachable!(),
                                    }
                                } else {
                                    app.set_status_message(format!(
                                        "{} does not support resume yet",
                                        session.agent_type
                                    ));
                                }
                            }
                        }
                        _ => {}
                    }
                }
                Event::Paste(text) => {
                    if app.launcher_is_active() {
                        app.paste_launcher(&text);
                    } else if app.focused_panel == Panel::Prompt {
                        app.paste_prompt(&text);
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
    if keyboard_enhancement_enabled {
        execute!(terminal.backend_mut(), PopKeyboardEnhancementFlags)?;
    }
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture,
        DisableBracketedPaste
    )?;
    terminal.show_cursor()?;

    Ok(())
}

fn keyboard_enhancement_flags() -> KeyboardEnhancementFlags {
    // Keep this intentionally conservative. `REPORT_ALTERNATE_KEYS` can cause
    // Shift+Enter to surface as an alternate printable key (observed as "j")
    // in some terminals. `DISAMBIGUATE_ESCAPE_CODES` is enough for terminals
    // that correctly support modified Enter via the kitty keyboard protocol.
    KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyEvent;

    #[test]
    fn prompt_key_action_treats_ctrl_j_as_newline() {
        assert_eq!(
            prompt_key_action(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::CONTROL)),
            PromptKeyAction::Newline
        );
    }

    #[test]
    fn prompt_key_action_keeps_plain_j_as_text() {
        assert_eq!(
            prompt_key_action(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE)),
            PromptKeyAction::Insert('j')
        );
        assert_eq!(
            prompt_key_action(KeyEvent::new(KeyCode::Char('J'), KeyModifiers::SHIFT)),
            PromptKeyAction::Insert('J')
        );
    }

    #[test]
    fn prompt_key_action_maps_line_editing_controls() {
        assert_eq!(
            prompt_key_action(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL)),
            PromptKeyAction::MoveLineStart
        );
        assert_eq!(
            prompt_key_action(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::CONTROL)),
            PromptKeyAction::MoveLineEnd
        );
        assert_eq!(
            prompt_key_action(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL)),
            PromptKeyAction::DeleteBufferStart
        );
        assert_eq!(
            prompt_key_action(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL)),
            PromptKeyAction::DeleteLineEnd
        );
        assert_eq!(
            prompt_key_action(KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL)),
            PromptKeyAction::DeleteWordBefore
        );
    }

    #[test]
    fn prompt_key_action_maps_mac_delete_fallbacks() {
        assert_eq!(
            prompt_key_action(KeyEvent::new(KeyCode::Backspace, KeyModifiers::SUPER)),
            PromptKeyAction::DeleteLineStart
        );
        assert_eq!(
            prompt_key_action(KeyEvent::new(KeyCode::Backspace, KeyModifiers::ALT)),
            PromptKeyAction::DeleteWordBefore
        );
        assert_eq!(
            prompt_key_action(KeyEvent::new(KeyCode::Backspace, KeyModifiers::CONTROL)),
            PromptKeyAction::DeleteWordBefore
        );
    }

    #[test]
    fn prompt_key_action_maps_enter_and_shift_enter() {
        assert_eq!(
            prompt_key_action(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            PromptKeyAction::Submit
        );
        assert_eq!(
            prompt_key_action(KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT)),
            PromptKeyAction::Newline
        );
    }
}
