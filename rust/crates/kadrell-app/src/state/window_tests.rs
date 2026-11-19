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

#[test]
fn zoom_belongs_to_the_current_window() {
    let dir = Dir::new();
    let (mut state, _, _, _) = two_windows(&dir);
    state.set_current_window(1);
    ControlHandler::toggle_zoom(&mut state);
    assert!(state.wins.get(1).unwrap().zoom);
    assert!(!state.zoom, "das Hauptfenster zoomt nicht mit");
    assert!(state.cur_zoom());
    state.set_current_window(0);
    assert!(!state.cur_zoom());
    ControlHandler::toggle_zoom(&mut state);
    assert!(state.zoom && state.wins.get(1).unwrap().zoom, "beide Fenster zoomen unabhängig");
    state.set_current_window(1);
    ControlHandler::toggle_zoom(&mut state);
    assert!(!state.wins.get(1).unwrap().zoom && state.zoom);
}

#[test]
fn zoom_ends_with_another_selection_and_is_not_saved() {
    let dir = Dir::new();
    let (mut state, _, b, c) = two_windows(&dir);
    state.view_select(1, &[b.clone(), c.clone()], false);
    state.view_toggle_zoom(1);
    state.view_focus(1, &c);
    assert!(state.wins.get(1).unwrap().zoom, "ein anderer Fokus lässt den Zoom stehen");
    state.view_select(1, std::slice::from_ref(&b), false);
    assert!(!state.wins.get(1).unwrap().zoom, "eine andere Auswahl beendet ihn");
    state.view_toggle_zoom(1);
    let mut again = state_in(&dir);
    again.open_view(1);
    assert!(!again.wins.get(1).unwrap().zoom, "der Zoom wird nicht gespeichert");
}

#[test]
fn taking_the_focus_tile_out_changes_only_that_window() {
    let dir = Dir::new();
    let (mut state, a, b, c) = two_windows(&dir);
    state.view_select(1, &[b.clone(), c.clone()], false);
    state.view_focus(1, &c);
    state.view_select(1, std::slice::from_ref(&c), true);
    assert_eq!(state.wins.get(1).unwrap().selected, std::slice::from_ref(&b));
    assert_eq!(state.selected, [a]);
}

/// Audit A27: „Kachel schließen, wenn Claude endet“ nimmt die Kachel aus der Auswahl jedes Fensters, das sie zeigt.
#[test]
fn closing_a_tile_when_claude_ends_acts_on_every_window() {
    let dir = Dir::new();
    let (mut state, a, b, c) = two_windows(&dir);
    state.select_keys(std::slice::from_ref(&b), true);
    state.view_select(1, &[a.clone(), b.clone()], false);
    state.settings.set("closeTileOnExit", serde_json::Value::Bool(true));
    let terms = super::tests::start_all(&mut state);
    terms.iter().find(|(k, _)| *k == b).unwrap().1.exited.set(true);
    assert!(state.on_exit(&b));
    assert_eq!(state.selected, std::slice::from_ref(&a), "Hauptfenster");
    assert_eq!(state.wins.get(1).unwrap().selected, std::slice::from_ref(&a), "weiteres Fenster");
    assert_eq!(state.wins.get(1).unwrap().focus.as_deref(), Some(a.as_str()));
    assert!(state.session(&c).is_some());
}

#[test]
fn a_window_with_only_the_ended_tile_keeps_it() {
    let dir = Dir::new();
    let (mut state, a, b, _) = two_windows(&dir);
    state.select_keys(std::slice::from_ref(&b), true);
    state.view_select(1, std::slice::from_ref(&b), false);
    state.settings.set("closeTileOnExit", serde_json::Value::Bool(true));
    let terms = super::tests::start_all(&mut state);
    terms.iter().find(|(k, _)| *k == b).unwrap().1.exited.set(true);
    state.on_exit(&b);
    assert_eq!(state.selected, std::slice::from_ref(&a));
    assert_eq!(state.wins.get(1).unwrap().selected, std::slice::from_ref(&b), "die letzte Kachel eines Fensters bleibt");
}

#[test]
fn a_column_step_in_any_window_reaches_the_others() {
    use crate::ui::layout::{GRID_COLUMNS_KEY, LayoutMode, LayoutState};
    let dir = Dir::new();
    let (mut state, _, _, _) = two_windows(&dir);
    state.save_workspace_setting(GRID_COLUMNS_KEY, Some(serde_json::json!(3)));
    let mut layout = LayoutState::new(LayoutMode::Grid);
    layout.sync_columns(&state.settings);
    assert_eq!(layout.cols, 3);
    state.save_workspace_setting(GRID_COLUMNS_KEY, None);
    layout.sync_columns(&state.settings);
    assert_eq!(layout.cols, 0, "ohne Eintrag wieder automatisch");
}
