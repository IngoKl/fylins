mod app;
mod highlight;
mod ui;

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::{
    env,
    io::{self, stdout},
    path::PathBuf,
};

use app::{App, Mode};
use ui::draw_ui;

// =============================================================================
// Terminal Setup/Cleanup
// =============================================================================

fn setup_terminal() -> io::Result<Terminal<CrosstermBackend<io::Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    Terminal::new(backend)
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> io::Result<()> {
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    Ok(())
}

// =============================================================================
// Event Handling
// =============================================================================

/// Result of handling common text input keys.
enum InputAction {
    /// Key was handled (cursor movement, character input, etc.)
    Handled,
    /// Escape was pressed - caller should cancel
    Cancel,
    /// Enter was pressed - caller should confirm
    Confirm,
    /// Key was not a text input key
    Unhandled,
}

/// Handles common text input keys (Ctrl+U, cursor movement, character input).
/// Returns an InputAction indicating what the caller should do.
fn handle_text_input(app: &mut App, key: &event::KeyEvent) -> InputAction {
    // Ctrl+U clears the input
    if key.modifiers.contains(event::KeyModifiers::CONTROL) {
        if let KeyCode::Char('u') = key.code {
            app.input_clear();
            return InputAction::Handled;
        }
    }

    match key.code {
        KeyCode::Esc => InputAction::Cancel,
        KeyCode::Enter => InputAction::Confirm,
        KeyCode::Backspace => {
            app.input_backspace();
            InputAction::Handled
        }
        KeyCode::Delete => {
            app.input_delete();
            InputAction::Handled
        }
        KeyCode::Left => {
            app.cursor_left();
            InputAction::Handled
        }
        KeyCode::Right => {
            app.cursor_right();
            InputAction::Handled
        }
        KeyCode::Home => {
            app.cursor_home();
            InputAction::Handled
        }
        KeyCode::End => {
            app.cursor_end();
            InputAction::Handled
        }
        KeyCode::Char(c) => {
            app.input_char(c);
            InputAction::Handled
        }
        _ => InputAction::Unhandled,
    }
}

fn handle_normal_mode(app: &mut App, key: event::KeyEvent) -> bool {
    // Clear transient messages on any keypress
    app.message = None;

    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => return false,
        KeyCode::Up | KeyCode::Char('k') => app.move_up(),
        KeyCode::Down | KeyCode::Char('j') => app.move_down(),
        KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => {
            if let Err(err) = app.enter_selected() {
                app.message = Some(format!("Cannot enter: {}", err));
            }
        }
        KeyCode::Backspace | KeyCode::Left | KeyCode::Char('h') => {
            app.go_to_parent();
        }
        KeyCode::PageUp => app.scroll_preview_up(),
        KeyCode::PageDown => app.scroll_preview_down(),
        KeyCode::Char('/') => app.start_search(),
        KeyCode::Char('H') => app.toggle_hidden(),
        KeyCode::Char('y') => app.yank_path(),
        KeyCode::Char('r') => app.start_rename(),
        KeyCode::Char('d') => app.start_delete(),
        KeyCode::Char('o') => app.open_with_default(),
        KeyCode::Char('p') => app.start_path(),
        KeyCode::Char('c') => app.copy_file(),
        KeyCode::Char('x') => app.cut_file(),
        KeyCode::Char('v') => app.paste_file(),
        KeyCode::Char('n') => app.start_new_file(),
        KeyCode::Char('N') => app.start_new_folder(),
        KeyCode::Char('`') => app.go_to_start(),
        KeyCode::Char('?') => app.toggle_help(),
        _ => {}
    }
    true
}

fn handle_search_mode(app: &mut App, key: event::KeyEvent) -> bool {
    match key.code {
        KeyCode::Esc => app.cancel_search(),
        KeyCode::Enter => app.confirm_search(),
        KeyCode::Backspace => app.backspace_search(),
        KeyCode::Up | KeyCode::Char('k')
            if key.modifiers.contains(event::KeyModifiers::CONTROL) =>
        {
            app.move_up()
        }
        KeyCode::Down | KeyCode::Char('j')
            if key.modifiers.contains(event::KeyModifiers::CONTROL) =>
        {
            app.move_down()
        }
        KeyCode::Char(c) => app.update_search(c),
        _ => {}
    }
    true
}

fn handle_rename_mode(app: &mut App, key: event::KeyEvent) -> bool {
    match handle_text_input(app, &key) {
        InputAction::Cancel => {
            app.mode = Mode::Normal;
            app.input.clear();
            app.cursor = 0;
            app.message = None;
        }
        InputAction::Confirm => app.confirm_rename(),
        InputAction::Handled | InputAction::Unhandled => {}
    }
    true
}

fn handle_confirm_delete_mode(app: &mut App, key: event::KeyEvent) -> bool {
    match key.code {
        KeyCode::Char('y') | KeyCode::Char('Y') => app.confirm_delete(),
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => app.cancel_delete(),
        _ => {}
    }
    true
}

fn handle_path_mode(app: &mut App, key: event::KeyEvent) -> bool {
    match handle_text_input(app, &key) {
        InputAction::Cancel => app.cancel_path(),
        InputAction::Confirm => app.confirm_path(),
        InputAction::Handled | InputAction::Unhandled => {}
    }
    true
}

fn handle_new_file_mode(app: &mut App, key: event::KeyEvent) -> bool {
    match handle_text_input(app, &key) {
        InputAction::Cancel => app.cancel_new(),
        InputAction::Confirm => app.confirm_new_file(),
        InputAction::Handled | InputAction::Unhandled => {}
    }
    true
}

fn handle_new_folder_mode(app: &mut App, key: event::KeyEvent) -> bool {
    match handle_text_input(app, &key) {
        InputAction::Cancel => app.cancel_new(),
        InputAction::Confirm => app.confirm_new_folder(),
        InputAction::Handled | InputAction::Unhandled => {}
    }
    true
}

fn handle_help_mode(app: &mut App, key: event::KeyEvent) -> bool {
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('?') => app.toggle_help(),
        _ => {}
    }
    true
}

fn handle_key_event(app: &mut App, key: event::KeyEvent) -> bool {
    if key.kind != KeyEventKind::Press {
        return true;
    }

    match &app.mode {
        Mode::Normal => handle_normal_mode(app, key),
        Mode::Search => handle_search_mode(app, key),
        Mode::Rename => handle_rename_mode(app, key),
        Mode::ConfirmDelete => handle_confirm_delete_mode(app, key),
        Mode::Path => handle_path_mode(app, key),
        Mode::NewFile => handle_new_file_mode(app, key),
        Mode::NewFolder => handle_new_folder_mode(app, key),
        Mode::Help => handle_help_mode(app, key),
    }
}

fn run_event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> io::Result<()> {
    loop {
        terminal.draw(|f| draw_ui(f, app))?;

        if let Event::Key(key) = event::read()? {
            if !handle_key_event(app, key) {
                break;
            }
        }
    }
    Ok(())
}

// =============================================================================
// Main Entry Point
// =============================================================================

fn main() -> io::Result<()> {
    let start_dir = env::args()
        .nth(1)
        .map(PathBuf::from)
        .or_else(|| env::current_dir().ok())
        .or_else(|| dirs_next::home_dir())
        .unwrap_or_else(|| PathBuf::from("."));

    let mut terminal = setup_terminal()?;
    let mut app = App::new(start_dir)?;

    let result = run_event_loop(&mut terminal, &mut app);

    restore_terminal(&mut terminal)?;
    result
}
