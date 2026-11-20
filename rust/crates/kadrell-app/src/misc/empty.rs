//! Leere Arbeitsfläche (Swift: `WorkspaceView.draw` mit `emptyReason`, `drawEmptyOnboarding`, `drawEmptyButtons`).
//! Fünf Fälle in dieser Rangfolge: Fehler (claude fehlt oder `claude agents` scheitert: Meldung und „Erneut prüfen“),
//! Laden (vor dem ersten Poll), Einstieg ohne jede Session (Titel, Haupttaste, Kürzel, auf dem ersten Start
//! Sprachtasten), Hinweis bei Sessions ohne Auswahl und im Auto-Modus der Hinweis „Gerade wartet keine Session“.

use gpui::{
    Context, FontWeight, Hsla, InteractiveElement, IntoElement, MouseButton, ParentElement, Styled, div, px, relative,
};
use kadrell_core::settings::Language;
use kadrell_i18n::trd;

use super::{adopt, os_text};
use crate::Shell;
use crate::theme::Theme;
use crate::ui::chrome::key_edge;

/// Warum die Fläche leer ist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reason {
    /// Binary fehlt oder `claude agents` schlägt fehl; `missing_binary` bietet zusätzlich den Installationsbefehl an.
    Error { message: String, missing_binary: bool },
    /// Vor dem ersten Poll.
    Loading,
    /// Es gibt keine Session: Einstieg.
    NoSessions,
    /// Es gibt Sessions, aber keine ist sichtbar; `auto_idle` im Auto-Modus mit nicht leerem Pool.
    Hint { auto_idle: bool },
}

/// Was die Shell über die Fläche weiß.
pub struct Facts {
    pub error: Option<String>,
    pub missing_binary: bool,
    pub polled: bool,
    pub session_count: usize,
    pub auto_idle: bool,
}

pub fn reason(f: &Facts) -> Reason {
    if let Some(message) = &f.error {
        return Reason::Error { message: message.clone(), missing_binary: f.missing_binary };
    }
    if !f.polled {
        return Reason::Loading;
    }
    if f.session_count == 0 { Reason::NoSessions } else { Reason::Hint { auto_idle: f.auto_idle } }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Card {
    pub title: String,
    pub button: String,
    /// Kürzel und was es tut, je eine Zeile.
    pub shortcuts: Vec<(String, String)>,
    /// Sessions in anderen Terminals.
    pub elsewhere: Option<String>,
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
    Card { title: trd!("Erste Session starten"), button: os_text(&trd!("Neue Session starten  ⌘N")), shortcuts, elsewhere }
}

/// Zweite Zeile, wenn Sessions da sind, aber keine gezeigt wird.
pub fn selection_hint(help_key: Option<&str>) -> String {
    match help_key {
        Some(k) => os_text(&trd!("⌘-Klick für mehrere · ⇧-Klick Bereich · Gruppe = alle · %@ Hilfe", k)),
        None => os_text(&trd!("⌘-Klick für mehrere · ⇧-Klick Bereich · Gruppe = alle")),
    }
}

/// Die zwei Zeilen des Hinweises: im Auto-Modus „Gerade wartet keine Session“, sonst die Auswahl.
pub fn hint_lines(auto_idle: bool, help_key: Option<&str>) -> (String, String) {
    if auto_idle {
        (trd!("Gerade wartet keine Session"), trd!("Auto-Modus: Kacheln erscheinen, sobald Claude etwas von dir will"))
    } else {
        (trd!("Session im Baum wählen"), selection_hint(help_key))
    }
}

/// Flagge und Name der Sprachtasten des ersten Starts („🇩🇪  Deutsch“).
pub fn language_choices() -> [(Language, String); 2] {
    [(Language::De, "🇩🇪  Deutsch".to_string()), (Language::En, "🇬🇧  English".to_string())]
}

/// Was eine Taste des Leerzustands tut.
#[derive(Clone)]
pub enum Press {
    NewSession,
    Recheck,
    CopyInstall,
    OpenDocs,
    Language(Language),
}

/// Die Tasten der Fehlerfläche: „Erneut prüfen“ (Haupttaste), bei fehlendem Binary dazu Installationsbefehl und Doku.
pub fn error_buttons(missing_binary: bool) -> Vec<(String, bool, Press)> {
    let mut buttons = vec![(trd!("Erneut prüfen"), true, Press::Recheck)];
    if missing_binary {
        buttons.push((trd!("Installationsbefehl kopieren"), false, Press::CopyInstall));
        buttons.push((trd!("Doku öffnen"), false, Press::OpenDocs));
    }
    buttons
}

// MARK: Zeichnen

const BUTTON_HEIGHT: f32 = 32.;
const BUTTON_GAP: f32 = 8.;

/// Taste des Leerzustands: 32 hoch, Breite Beschriftung plus 32, Beschriftung 11,5 fett; die Haupttaste in Akzent,
/// die anderen neutral (`KeyRole`). Die Aktion feuert beim Drücken wie in Swift.
fn button(i: usize, label: String, primary: bool, press: Press, t: &Theme, k: f32, cx: &mut Context<Shell>) -> impl IntoElement + use<> {
    let (face, ink): (Hsla, Hsla) = if primary { (t.accent, t.pill_text_on(t.accent)) } else { (t.surface, t.key_label()) };
    let cap = div()
        .flex()
        .items_center()
        .justify_center()
        .h_full()
        .px(px(16. * k))
        .rounded(px(4. * k))
        .bg(face)
        .text_color(ink)
        .text_size(px(11.5 * k))
        .font_weight(FontWeight::BOLD)
        .whitespace_nowrap()
        .child(label);
    div()
        .id(("empty-button", i))
        .flex_none()
        .h(px(BUTTON_HEIGHT * k))
        .pb(px(2. * k))
        .rounded(px(5. * k))
        .bg(key_edge(t))
        .cursor_pointer()
        .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, window, cx| {
            cx.stop_propagation();
            this.press_empty(&press, window, cx);
        }))
        .child(cap)
}

fn button_row(buttons: Vec<(String, bool, Press)>, t: &Theme, k: f32, cx: &mut Context<Shell>) -> impl IntoElement + use<> {
    let items = buttons.into_iter().enumerate().map(|(i, (label, primary, press))| button(i, label, primary, press, t, k, cx));
    div().flex().items_center().gap(px(BUTTON_GAP * k)).children(items)
}

fn shortcut_line(key: String, text: String, t: &Theme, k: f32) -> impl IntoElement + use<> {
    div()
        .flex()
        .gap(px(6. * k))
        .h(px(20. * k))
        .items_center()
        .text_size(px(11. * k))
        .child(div().font_weight(FontWeight::BOLD).text_color(t.fg).child(key))
        .child(div().text_color(t.muted.opacity(0.75)).child(text))
}

fn onboarding(c: Card, chosen: bool, t: &Theme, k: f32, cx: &mut Context<Shell>) -> impl IntoElement + use<> {
    let main = vec![(c.button, true, Press::NewSession)];
    let lines = c.shortcuts.into_iter().map(|(key, text)| shortcut_line(key, text, t, k));
    let hint = |text: String| div().text_size(px(11. * k)).text_color(t.muted.opacity(0.55)).child(text);
    let languages = (!chosen).then(|| language_choices().map(|(l, title)| (title, false, Press::Language(l))).to_vec());
    div()
        .flex()
        .flex_col()
        .items_center()
        .children(languages.map(|buttons| div().mb(px(24. * k)).child(button_row(buttons, t, k, cx))))
        .child(div().text_size(px(13. * k)).font_weight(FontWeight::BOLD).text_color(t.fg).child(c.title))
        .child(div().mt(px(16. * k)).child(button_row(main, t, k, cx)))
        .child(div().mt(px(28. * k)).flex().flex_col().items_center().children(lines))
        .children(c.elsewhere.map(|text| div().mt(px(12. * k)).child(hint(text))))
}

/// Zwei Zeilen unter dem Mittelpunkt wie `drawCentered`: die erste 16 über der Mitte, die zweite 4 darunter.
fn two_lines(first: impl IntoElement, second: Option<impl IntoElement>, k: f32) -> gpui::Div {
    div()
        .absolute()
        .w_full()
        .top(relative(0.5))
        .mt(px(-16. * k))
        .flex()
        .flex_col()
        .items_center()
        .gap(px(4. * k))
        .child(first)
        .children(second)
}

fn error_view(message: String, missing_binary: bool, t: &Theme, k: f32, cx: &mut Context<Shell>) -> impl IntoElement + use<> {
    let title = div().text_size(px(12. * k)).text_color(t.error).child(trd!("Sessions können nicht geladen werden"));
    let detail = div().text_size(px(11. * k)).text_color(t.error.opacity(0.7)).child(message);
    two_lines(title, Some(detail), k).child(div().mt(px(8. * k)).child(button_row(error_buttons(missing_binary), t, k, cx)))
}

fn loading_view(t: &Theme, k: f32) -> impl IntoElement + use<> {
    two_lines(div().text_size(px(12. * k)).text_color(t.muted).child(trd!("Lade Sessions …")), None::<gpui::Div>, k)
}

fn hint_view(auto_idle: bool, help_key: Option<&str>, t: &Theme, k: f32) -> impl IntoElement + use<> {
    let (a, b) = hint_lines(auto_idle, help_key);
    let first = div().text_size(px(12. * k)).text_color(t.muted).child(a);
    two_lines(first, Some(div().text_size(px(11. * k)).text_color(t.muted.opacity(0.7)).child(b)), k)
}

/// Die leere Arbeitsfläche. Ohne jede Session startet ein Klick auf die freie Fläche eine neue Session (`onEmptyClick`).
pub fn view(shell: &Shell, t: &Theme, cx: &mut Context<Shell>) -> impl IntoElement + use<> {
    let help_key = shell.help_shortcut();
    let k = crate::appearance::current(cx).scale;
    let reason = reason(&shell.empty_facts());
    let area = div().id("empty-area").absolute().size_full().flex().items_center().justify_center().text_color(t.muted);
    match reason {
        Reason::Error { message, missing_binary } => area.child(error_view(message, missing_binary, t, k, cx)).into_any_element(),
        Reason::Loading => area.child(loading_view(t, k)).into_any_element(),
        Reason::NoSessions => {
            let body = onboarding(card(help_key, shell.misc.other_interactive), shell.language_chosen(), t, k, cx);
            area.on_mouse_down(MouseButton::Left, cx.listener(|this, _, window, cx| this.new_session_action(window, cx))).child(body).into_any_element()
        }
        Reason::Hint { auto_idle } => area.child(hint_view(auto_idle, help_key.as_deref(), t, k)).into_any_element(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts() -> Facts {
        Facts { error: None, missing_binary: false, polled: true, session_count: 0, auto_idle: false }
    }

    #[test]
    fn the_empty_reason_ranks_error_then_loading_then_sessions() {
        assert_eq!(reason(&facts()), Reason::NoSessions);
        assert_eq!(reason(&Facts { session_count: 3, ..facts() }), Reason::Hint { auto_idle: false });
        assert_eq!(reason(&Facts { session_count: 3, auto_idle: true, ..facts() }), Reason::Hint { auto_idle: true });
        assert_eq!(reason(&Facts { polled: false, ..facts() }), Reason::Loading);
        let failing = Facts { error: Some("x".into()), polled: false, missing_binary: true, ..facts() };
        assert_eq!(reason(&failing), Reason::Error { message: "x".into(), missing_binary: true });
    }

    #[test]
    fn the_error_offers_recheck_and_for_a_missing_binary_install_and_docs() {
        let plain: Vec<String> = error_buttons(false).into_iter().map(|b| b.0).collect();
        assert_eq!(plain, ["Erneut prüfen"]);
        let missing = error_buttons(true);
        let labels: Vec<&str> = missing.iter().map(|b| b.0.as_str()).collect();
        assert_eq!(labels, ["Erneut prüfen", "Installationsbefehl kopieren", "Doku öffnen"]);
        assert_eq!(missing.iter().map(|b| b.1).collect::<Vec<_>>(), [true, false, false], "nur die erste ist die Haupttaste");
    }

    #[test]
    fn the_card_lists_the_core_shortcuts_and_the_help_key() {
        let c = card(Some("Alt+F1".into()), 0);
        let keys: Vec<&str> = c.shortcuts.iter().map(|(k, _)| k.as_str()).collect();
        if cfg!(target_os = "macos") {
            assert_eq!(keys, ["⌘N", "⌘⏎", "Alt+F1"]);
        } else {
            assert_eq!(keys, ["Strg+N", "Alt+Enter", "Alt+F1"]);
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
    fn sessions_elsewhere_add_the_hint() {
        assert_eq!(card(None, 0).elsewhere, None);
        let c = card(None, 2);
        assert_eq!(c.elsewhere.as_deref(), Some("2 Claude-Sessions laufen interaktiv in anderen Terminals"));
    }

    #[test]
    fn the_selection_hint_names_the_help_key_only_when_there_is_one() {
        assert!(selection_hint(Some("Alt+F1")).ends_with("Alt+F1 Hilfe"));
        assert!(!selection_hint(None).contains("Hilfe"));
    }

    #[test]
    fn auto_mode_with_a_pool_says_nothing_is_waiting() {
        let (a, b) = hint_lines(true, None);
        assert_eq!(a, "Gerade wartet keine Session");
        assert!(b.starts_with("Auto-Modus: Kacheln erscheinen"));
        assert_eq!(hint_lines(false, None).0, "Session im Baum wählen");
    }

    #[test]
    fn the_language_buttons_show_flag_and_native_name() {
        let [(de, de_title), (en, en_title)] = language_choices();
        assert_eq!((de, en), (Language::De, Language::En));
        assert_eq!((de_title.as_str(), en_title.as_str()), ("🇩🇪  Deutsch", "🇬🇧  English"));
    }
}
