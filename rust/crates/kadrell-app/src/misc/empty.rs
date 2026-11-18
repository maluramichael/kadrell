//! Leere Arbeitsfläche (Swift: `drawEmptyOnboarding` und der `.hint`-Zweig in `WorkspaceView`). Ohne jede Session führt
//! eine Karte zur ersten: Titel, Haupttaste, die Kernkürzel in der Schreibweise des Systems, ein Hinweis auf Claude-
//! Sessions in anderen Terminals und wie man sie übernimmt. Gibt es Sessions, aber keine ist sichtbar, steht nur ein
//! kurzer Hinweis da.

use gpui::{Context, FontWeight, InteractiveElement, IntoElement, ParentElement, StatefulInteractiveElement, Styled, div, px};
use kadrell_i18n::trd;

use super::{adopt, os_text};
use crate::Shell;
use crate::theme::Theme;
use crate::ui::overlay::key_button;

/// Warum die Fläche leer ist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    /// Es gibt keine Session: Einstieg.
    NoSessions,
    /// Es gibt Sessions, aber keine ist ausgewählt.
    Hint,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Card {
    pub title: String,
    pub button: String,
    /// Kürzel und was es tut, je eine Zeile.
    pub shortcuts: Vec<(String, String)>,
    /// Sessions in anderen Terminals.
    pub elsewhere: Option<String>,
    /// Wie man sie übernimmt (tmux-Import), nur wenn es welche gibt.
    pub import: Option<String>,
}

pub fn reason(session_count: usize) -> Reason {
    if session_count == 0 { Reason::NoSessions } else { Reason::Hint }
}

/// Die Karte für den Einstieg. `help_key` ist die beschriftete Belegung von „Hilfe“ (`None`: keine, dann fehlt die Zeile).
pub fn card(help_key: Option<String>, other_interactive: usize) -> Card {
    let mut shortcuts = vec![
        ("⌘N".to_string(), trd!("sucht Projekt oder Ordner")),
        ("⌘⏎".to_string(), trd!("zweite Session im selben Ordner")),
    ];
    shortcuts = shortcuts.into_iter().map(|(k, t)| (os_text(&k), t)).collect();
    if let Some(key) = help_key {
        shortcuts.push((key, trd!("alle Kürzel")));
    }
    let elsewhere = adopt::other_terminals_hint(other_interactive);
    let import = elsewhere.as_ref().map(|_| trd!("Import: tools/tmux-dump.py dump (siehe README)"));
    Card { title: trd!("Erste Session starten"), button: os_text(&trd!("Neue Session starten  ⌘N")), shortcuts, elsewhere, import }
}

/// Zweite Zeile, wenn Sessions da sind, aber keine gezeigt wird.
pub fn selection_hint(help_key: Option<&str>) -> String {
    match help_key {
        Some(k) => os_text(&trd!("⌘-Klick für mehrere · ⇧-Klick Bereich · Gruppe = alle · %@ Hilfe", k)),
        None => os_text(&trd!("⌘-Klick für mehrere · ⇧-Klick Bereich · Gruppe = alle")),
    }
}

fn shortcut_line(key: String, text: String, t: &Theme) -> impl IntoElement + use<> {
    div()
        .flex()
        .gap(px(8.))
        .h(px(20.))
        .items_center()
        .text_size(px(11.))
        .child(div().font_weight(FontWeight::BOLD).text_color(t.fg).child(key))
        .child(div().text_color(t.muted).child(text))
}

fn onboarding(c: Card, t: &Theme, cx: &mut Context<Shell>) -> impl IntoElement + use<> {
    let button = key_button(c.button, t).id("empty-new").on_click(cx.listener(|this, _, window, cx| this.new_session_action(window, cx)));
    let lines = c.shortcuts.into_iter().map(|(k, text)| shortcut_line(k, text, t));
    let hint = |text: String, id: &'static str| div().id(id).text_size(px(11.)).text_color(t.muted.opacity(0.7)).child(text);
    div()
        .flex()
        .flex_col()
        .items_center()
        .child(div().text_size(px(13.)).font_weight(FontWeight::BOLD).text_color(t.fg).child(c.title))
        .child(div().mt(px(16.)).child(button))
        .child(div().mt(px(28.)).flex().flex_col().items_center().children(lines))
        .children(c.elsewhere.map(|text| div().mt(px(32.)).child(hint(text, "empty-elsewhere"))))
        .children(c.import.map(|text| div().mt(px(4.)).child(hint(text, "empty-import"))))
}

fn selection(help_key: Option<&str>, t: &Theme) -> impl IntoElement + use<> {
    div()
        .flex()
        .flex_col()
        .items_center()
        .gap(px(8.))
        .child(div().text_size(px(12.)).text_color(t.muted).child(trd!("Session im Baum wählen")))
        .child(div().text_size(px(11.)).text_color(t.muted.opacity(0.7)).child(selection_hint(help_key)))
}

/// Die leere Arbeitsfläche, mittig. `shell` liefert Sessions, Belegung der Hilfe und Sessions anderswo.
pub fn view(shell: &Shell, t: &Theme, cx: &mut Context<Shell>) -> impl IntoElement + use<> {
    let help_key = shell.help_shortcut();
    let body = match reason(shell.state.store.sessions.sessions.len()) {
        Reason::NoSessions => onboarding(card(help_key, shell.misc.other_interactive), t, cx).into_any_element(),
        Reason::Hint => selection(help_key.as_deref(), t).into_any_element(),
    };
    div().absolute().size_full().flex().items_center().justify_center().text_color(t.muted).child(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_sessions_means_onboarding() {
        assert_eq!(reason(0), Reason::NoSessions);
        assert_eq!(reason(3), Reason::Hint);
    }

    #[test]
    fn the_card_lists_the_core_shortcuts_and_the_help_key() {
        let c = card(Some("Alt+F1".into()), 0);
        let keys: Vec<&str> = c.shortcuts.iter().map(|(k, _)| k.as_str()).collect();
        if cfg!(target_os = "macos") {
            assert_eq!(keys, ["⌘N", "⌘⏎", "Alt+F1"]);
        } else {
            assert_eq!(keys, ["Strg+N", "Strg+Enter", "Alt+F1"]);
        }
        assert_eq!(c.title, "Erste Session starten");
        assert!(c.button.starts_with("Neue Session starten"));
        assert!(!c.button.contains('⌘') || cfg!(target_os = "macos"));
    }

    #[test]
    fn without_a_help_binding_there_is_no_help_line() {
        assert_eq!(card(None, 0).shortcuts.len(), 2);
    }

    #[test]
    fn sessions_elsewhere_add_the_hint_and_the_import_tip() {
        assert_eq!(card(None, 0).elsewhere, None);
        assert_eq!(card(None, 0).import, None);
        let c = card(None, 2);
        assert_eq!(c.elsewhere.as_deref(), Some("2 Claude-Sessions laufen interaktiv in anderen Terminals"));
        assert!(c.import.unwrap().contains("tmux-dump.py"));
    }

    #[test]
    fn the_selection_hint_names_the_help_key_only_when_there_is_one() {
        assert!(selection_hint(Some("Alt+F1")).ends_with("Alt+F1 Hilfe"));
        assert!(!selection_hint(None).contains("Hilfe"));
    }
}
