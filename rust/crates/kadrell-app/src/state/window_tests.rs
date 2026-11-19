//! Befehle wirken im aktuellen Fenster (`AppDelegate.current`): Steuerkanal, neue Sessions mit `--show`, Auswahl und
//! Fokus landen in der Ansicht des weiteren Fensters, das Hauptfenster bleibt unberührt.

use kadrell_control::{ControlHandler, LayoutMode};

use super::AppState;
use super::tests::{Dir, cwd, state_in};

fn two_windows(dir: &Dir) -> (AppState, String, String, String) {
    let mut state = state_in(dir);
    let a = state.new_claude_session(&cwd(dir, "alpha"), None, true);
    let b = state.new_claude_session(&cwd(dir, "beta"), None, false);
    let c = state.new_claude_session(&cwd(dir, "gamma"), None, false);
    state.open_view(1);
    (state, a, b, c)
}

#[test]
fn commands_in_the_main_window_leave_other_windows_alone() {
    let dir = Dir::new();
    let (mut state, a, b, _) = two_windows(&dir);
    let before = state.wins.get(1).cloned();
    state.set_current_window(0);
    ControlHandler::select(&mut state, std::slice::from_ref(&b), false);
    assert_eq!(state.selected, std::slice::from_ref(&b));
    assert_eq!(state.wins.get(1).cloned(), before);
    assert_ne!(state.selected, [a]);
}

#[test]
fn new_show_select_focus_and_layout_land_in_the_current_extra_window() {
    let dir = Dir::new();
    let (mut state, a, b, c) = two_windows(&dir);
    state.set_current_window(1);

    let d = state.new_claude_session(&cwd(&dir, "delta"), None, true);
    assert_eq!(state.selected, std::slice::from_ref(&a), "das Hauptfenster zeigt weiter seine Session");
    assert_eq!(state.cur_selected(), std::slice::from_ref(&d));
    assert_eq!(state.cur_focus().as_deref(), Some(d.as_str()));

    ControlHandler::select(&mut state, &[b.clone(), c.clone()], false);
    assert_eq!(ControlHandler::selected(&state), [b.clone(), c.clone()]);
    ControlHandler::focus(&mut state, &c);
    assert_eq!(ControlHandler::focused(&state).as_deref(), Some(c.as_str()));
    ControlHandler::set_layout(&mut state, LayoutMode::Stack);
    assert_eq!(ControlHandler::layout(&state), LayoutMode::Stack);

    assert_eq!(state.selected, std::slice::from_ref(&a));
    assert_eq!(state.layout, LayoutMode::Grid, "das Hauptfenster behält sein Layout");
    state.set_current_window(0);
    assert_eq!(ControlHandler::layout(&state), LayoutMode::Grid);
    assert_eq!(ControlHandler::selected(&state), std::slice::from_ref(&a));
}

#[test]
fn the_view_survives_a_restart_with_its_suffix_keys() {
    let dir = Dir::new();
    let (b, c) = {
        let (mut state, _, b, c) = two_windows(&dir);
        state.set_current_window(1);
        ControlHandler::select(&mut state, &[b.clone(), c.clone()], false);
        ControlHandler::set_layout(&mut state, LayoutMode::Row);
        (b, c)
    };
    let mut state = state_in(&dir);
    state.open_view(1);
    let view = state.wins.get(1).unwrap();
    assert_eq!(view.selected, [b.clone(), c]);
    assert_eq!(view.mode, LayoutMode::Row);
    assert_eq!(view.focus.as_deref(), Some(b.as_str()));
}

#[test]
fn closing_the_current_window_hands_commands_to_the_fallback() {
    let dir = Dir::new();
    let (mut state, a, b, _) = two_windows(&dir);
    state.set_current_window(1);
    state.close_view(1, 0);
    assert_eq!(state.wins.current, 0);
    ControlHandler::select(&mut state, std::slice::from_ref(&b), false);
    assert_eq!(state.selected, std::slice::from_ref(&b));
    assert_ne!(state.selected, [a]);
}

#[test]
fn a_selection_in_an_extra_window_starts_its_sessions() {
    let dir = Dir::new();
    let (mut state, _, b, _) = two_windows(&dir);
    state.set_current_window(1);
    ControlHandler::select(&mut state, std::slice::from_ref(&b), false);
    let started: Vec<String> = state.take_starts().into_iter().map(|(k, _)| k).collect();
    assert!(started.contains(&b));
}
