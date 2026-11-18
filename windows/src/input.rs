//! Tasten der Oberfläche: Präfix Strg+B wie tmux, Alt+N/P und Alt+1…9 direkt, Baum-Modus, Eingabe beim
//! Umbenennen und Rückfragen. Alles, was hier nicht gebraucht wird, geht an das Terminal der Fokus-Session.

use crate::app::App;
use crate::ui::{self, Confirm, Mode, Ui};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

pub fn handle_key(app: &mut App, ui: &mut Ui, key: KeyEvent) {
    ui.notice = None;
    let mode = std::mem::replace(&mut ui.mode, Mode::Terminal);
    ui.mode = match mode {
        Mode::Terminal => terminal_key(app, key),
        Mode::Prefix => prefix_key(app, ui, key),
        Mode::Tree => tree_key(app, key),
        Mode::Rename(text) => rename_key(app, key, text),
        Mode::Confirm(c) => confirm_key(app, ui, key, c),
        Mode::Help => Mode::Terminal,
    };
    app.mark_dirty();
}

fn is_prefix(key: &KeyEvent) -> bool {
    key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('b')
}

fn terminal_key(app: &mut App, key: KeyEvent) -> Mode {
    if is_prefix(&key) {
        return Mode::Prefix;
    }
    if key.modifiers == KeyModifiers::ALT && navigate(app, key.code) {
        return Mode::Terminal;
    }
    send_key(app, &key);
    Mode::Terminal
}

fn send_key(app: &App, key: &KeyEvent) {
    let Some(tile) = app.focused.as_ref().and_then(|k| app.tiles.get(k)).filter(|t| t.alive) else { return };
    if let Some(bytes) = ui::key_bytes(key, tile.pane.application_cursor()) {
        tile.pane.write(&bytes);
    }
}

/// n/p und Ziffern, gemeinsam für Alt-Kürzel und Präfix. `true`, wenn die Taste dafür war.
fn navigate(app: &mut App, code: KeyCode) -> bool {
    let keys = app.store.ordered_keys();
    let current = app.focused.as_ref().and_then(|f| keys.iter().position(|k| k == f));
    let target = match code {
        KeyCode::Char('n') | KeyCode::Down => current.map(|i| (i + 1) % keys.len()).or(Some(0)),
        KeyCode::Char('p') | KeyCode::Up => current.map(|i| (i + keys.len() - 1) % keys.len()).or(Some(0)),
        KeyCode::Char(c @ '1'..='9') => Some(c as usize - '1' as usize),
        _ => return false,
    };
    if let Some(key) = target.and_then(|i| keys.get(i)).cloned() {
        app.focus(&key);
    }
    true
}

fn prefix_key(app: &mut App, ui: &mut Ui, key: KeyEvent) -> Mode {
    if is_prefix(&key) {
        send_key(app, &key);
        return Mode::Terminal;
    }
    if navigate(app, key.code) {
        return Mode::Terminal;
    }
    let focused = app.focused.clone();
    match (key.code, focused) {
        (KeyCode::Char('c'), f) => new_session(app, ui, f),
        (KeyCode::Char('x'), Some(k)) => return Mode::Confirm(Confirm::Kill(k)),
        (KeyCode::Char('s'), Some(k)) => app.stop(&k),
        (KeyCode::Char('r'), Some(k)) => restart(app, ui, &k),
        (KeyCode::Char(','), Some(k)) => return Mode::Rename(app.store.session(&k).and_then(|s| s.custom_name.clone()).unwrap_or_default()),
        (KeyCode::Char('w'), _) => return Mode::Tree,
        (KeyCode::Char('b'), _) => ui.sidebar = !ui.sidebar,
        (KeyCode::Char('q'), _) => return Mode::Confirm(Confirm::Quit),
        (KeyCode::Char('?'), _) => return Mode::Help,
        _ => {}
    }
    Mode::Terminal
}

fn new_session(app: &mut App, ui: &mut Ui, focused: Option<String>) {
    let cwd = focused
        .and_then(|k| app.store.session(&k).map(|s| s.cwd.clone()))
        .unwrap_or_else(|| std::env::current_dir().map(|p| p.to_string_lossy().replace('\\', "/")).unwrap_or_default());
    let group = app.focused.as_deref().and_then(|k| app.store.group_of(k));
    if let Err(e) = app.start_session(group, &cwd, true, None, None, None) {
        ui.notice = Some(e.0);
    }
}

fn restart(app: &mut App, ui: &mut Ui, key: &str) {
    app.stop(key);
    if let Err(e) = app.launch(key) {
        ui.notice = Some(e.0);
    }
}

fn tree_key(app: &mut App, key: KeyEvent) -> Mode {
    match key.code {
        KeyCode::Enter | KeyCode::Esc => Mode::Terminal,
        KeyCode::Char('j') => {
            navigate(app, KeyCode::Down);
            Mode::Tree
        }
        KeyCode::Char('k') => {
            navigate(app, KeyCode::Up);
            Mode::Tree
        }
        KeyCode::Char('x') => app.focused.clone().map(|k| Mode::Confirm(Confirm::Kill(k))).unwrap_or(Mode::Tree),
        KeyCode::Char(',') => app.focused.clone().map(|k| Mode::Rename(app.store.session(&k).and_then(|s| s.custom_name.clone()).unwrap_or_default())).unwrap_or(Mode::Tree),
        code => {
            navigate(app, code);
            Mode::Tree
        }
    }
}

fn rename_key(app: &mut App, key: KeyEvent, mut text: String) -> Mode {
    match key.code {
        KeyCode::Enter => {
            if let Some(k) = app.focused.clone() {
                app.rename(&k, &text);
            }
            return Mode::Terminal;
        }
        KeyCode::Esc => return Mode::Terminal,
        KeyCode::Backspace => {
            text.pop();
        }
        KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => text.push(c),
        _ => {}
    }
    Mode::Rename(text)
}

fn confirm_key(app: &mut App, ui: &mut Ui, key: KeyEvent, confirm: Confirm) -> Mode {
    if !matches!(key.code, KeyCode::Char('j' | 'J' | 'y' | 'Y') | KeyCode::Enter) {
        return Mode::Terminal;
    }
    match confirm {
        Confirm::Kill(k) => app.kill_session(&k),
        Confirm::Quit => ui.quit = true,
    }
    Mode::Terminal
}
