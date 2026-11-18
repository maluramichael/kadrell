//! Rückfrage im App-Design (`ConfirmView.swift`): Titel, Info, Abbrechen und Bestätigen, Häkchen „Nicht mehr fragen“.
//! Nicht-destruktive bestätigt blankes Enter, destruktive nur Strg/⌘+Enter. Esc bricht ab. Dazu die Texte der drei
//! Rückfragen (Session schließen, Gruppe schließen, Session stoppen) aus `AppDelegate.swift`.

use gpui::{
    App, Context, EventEmitter, FocusHandle, Focusable, InteractiveElement, IntoElement, KeyBinding, ParentElement, Render,
    StatefulInteractiveElement, Styled, Window, actions, div, px,
};

use kadrell_core::Session;
use kadrell_core::settings::Ask;
use kadrell_i18n::trd;

use crate::lang::shortcut;
use crate::theme::Theme;
use crate::ui::instrument::{KeyRole, key_button_with};
use crate::ui::overlay::{backdrop, dialog_frame, overlay_width};
use crate::ui::theme;

actions!(kadrell_confirm, [Accept, AcceptForce, Dismiss]);

const CONTEXT: &str = "KadrellConfirm";

pub fn bind_keys(cx: &mut App) {
    let c = Some(CONTEXT);
    let force = if cfg!(target_os = "macos") { "cmd-enter" } else { "ctrl-enter" };
    cx.bind_keys([KeyBinding::new("enter", Accept, c), KeyBinding::new(force, AcceptForce, c), KeyBinding::new("escape", Dismiss, c)]);
}

/// Was nach dem Bestätigen passiert.
#[derive(Clone, Debug, PartialEq)]
pub enum ConfirmAction {
    CloseSession(String),
    CloseGroup(String),
    StopSession(String),
}

/// Inhalt einer Rückfrage.
#[derive(Clone, Debug, PartialEq)]
pub struct ConfirmSpec {
    pub title: String,
    pub info: String,
    pub button: String,
    pub destructive: bool,
    /// Nur eine Mitteilung ohne Alternative: kein „Abbrechen“.
    pub info_only: bool,
    pub ask: Option<Ask>,
    pub action: ConfirmAction,
}

/// „Session beenden und entfernen“ (`closeSession`): Text je nach Art der Session.
pub fn close_session_spec(s: &Session) -> ConfirmSpec {
    let info = if s.is_remote() {
        trd!("Die Verbindung wird getrennt, die tmux-Session auf %@ läuft weiter.", s.host.clone().unwrap_or_default())
    } else if s.is_shell() {
        trd!("Die Shell und alles, was darin läuft, wird beendet.")
    } else {
        trd!("Claude wird beendet und die Kachel entfernt. Die Konversation bleibt erhalten: claude --resume %@", &s.session_id)
    };
    ConfirmSpec {
        title: trd!("„%@“ beenden und entfernen?", s.title()),
        info,
        button: trd!("Entfernen"),
        destructive: true,
        info_only: false,
        ask: Some(Ask::CloseSession),
        action: ConfirmAction::CloseSession(s.id.clone()),
    }
}

/// „Gruppe schließen“ (`closeGroup`): nur mit Sessions, eine leere Gruppe geht ohne Rückfrage.
pub fn close_group_spec(id: &str, name: &str, members: usize) -> ConfirmSpec {
    ConfirmSpec {
        title: trd!("Gruppe „%@“ mit %lld Session(s) schließen?", name, members),
        info: trd!("Claude wird in allen Sessions beendet und die Gruppe entfernt. Die Konversationen bleiben erhalten."),
        button: trd!("Schließen"),
        destructive: true,
        info_only: false,
        ask: Some(Ask::CloseGroup),
        action: ConfirmAction::CloseGroup(id.to_string()),
    }
}

/// „Session stoppen“ (`stopSession`): nicht destruktiv, die Kachel bleibt.
pub fn stop_session_spec(s: &Session) -> ConfirmSpec {
    ConfirmSpec {
        title: trd!("Session „%@“ stoppen?", s.title()),
        info: trd!("Claude wird beendet, die Kachel bleibt. Ein Klick setzt die Konversation fort."),
        button: trd!("Stoppen"),
        destructive: false,
        info_only: false,
        ask: Some(Ask::StopSession),
        action: ConfirmAction::StopSession(s.id.clone()),
    }
}

pub enum ConfirmEvent {
    Accept(ConfirmAction),
    Cancel(ConfirmAction),
    /// Häkchen „Nicht mehr fragen“ gesetzt oder entfernt: die Einstellung gilt sofort.
    DontAsk(Ask, bool),
}

impl EventEmitter<ConfirmEvent> for Confirm {}

pub struct Confirm {
    spec: ConfirmSpec,
    dont_ask: bool,
    focus: FocusHandle,
}

impl Confirm {
    pub fn new(spec: ConfirmSpec, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        Self { spec, dont_ask: false, focus }
    }

    fn accept(&mut self, cx: &mut Context<Self>) {
        cx.emit(ConfirmEvent::Accept(self.spec.action.clone()));
    }

    fn cancel(&mut self, cx: &mut Context<Self>) {
        if self.dont_ask {
            // Abbrechen stellt zurück: wer abbricht, will weiter gefragt werden.
            if let Some(ask) = self.spec.ask {
                cx.emit(ConfirmEvent::DontAsk(ask, false));
            }
        }
        cx.emit(ConfirmEvent::Cancel(self.spec.action.clone()));
    }

    fn toggle_dont_ask(&mut self, cx: &mut Context<Self>) {
        self.dont_ask = !self.dont_ask;
        if let Some(ask) = self.spec.ask {
            cx.emit(ConfirmEvent::DontAsk(ask, self.dont_ask));
        }
        cx.notify();
    }
}

/// Beschriftung des destruktiven Knopfs: `Entfernen  ⌘⏎` auf dem Mac, `Entfernen  Strg+Enter` unter Windows.
pub fn destructive_label(button: &str) -> String {
    let key = shortcut("⌘↩");
    let key = if cfg!(target_os = "macos") { key.replace('↩', "⏎") } else { key };
    format!("{button}  {key}")
}

/// Rolle der Bestätigen-Taste: destruktiv in `error`, sonst primär in Akzent (`InstrumentKeyStyle`).
pub fn confirm_role(destructive: bool) -> KeyRole {
    if destructive { KeyRole::Destructive } else { KeyRole::Primary }
}

fn checkbox(on: bool, t: &Theme) -> gpui::Div {
    let s = crate::ui::overlay::scale();
    let mark = div().size(px(8. * s)).rounded(px(1.5 * s)).bg(if on { t.accent } else { gpui::transparent_black() });
    div().size(px(14. * s)).flex_none().flex().items_center().justify_center().rounded(px(3. * s)).border_1().border_color(t.line).bg(t.bg).child(mark)
}

impl Confirm {
    fn ask_row(&self, t: &Theme, cx: &mut Context<Self>) -> gpui::Stateful<gpui::Div> {
        let color = if self.dont_ask { t.fg } else { t.muted };
        div()
            .id("confirm-dont-ask")
            .flex()
            .items_center()
            .gap(px(6.))
            .cursor_pointer()
            .text_size(px(12.))
            .text_color(color)
            .on_click(cx.listener(|this, _, _, cx| this.toggle_dont_ask(cx)))
            .tooltip(crate::ui::tooltip::text(shortcut(&trd!("Wieder einschalten: Einstellungen (⌘,) › Rückfragen"))))
            .child(checkbox(self.dont_ask, t))
            .child(trd!("Nicht mehr fragen"))
    }

    fn buttons(&self, t: &Theme, cx: &mut Context<Self>) -> gpui::Div {
        let s = crate::ui::overlay::scale();
        let mut row = div().flex().items_center().gap(px(8. * s)).p(px(16. * s));
        if self.spec.ask.is_some() {
            row = row.child(self.ask_row(t, cx));
        }
        row = row.child(div().flex_1());
        if !self.spec.info_only {
            row = row.child(key_button_with("confirm-cancel", trd!("Abbrechen"), KeyRole::Neutral, false, t, cx.listener(|this, _, _, cx| this.cancel(cx))));
        }
        let label = if self.spec.destructive { destructive_label(&self.spec.button) } else { self.spec.button.clone() };
        row.child(key_button_with("confirm-ok", label, confirm_role(self.spec.destructive), false, t, cx.listener(|this, _, _, cx| this.accept(cx))))
    }
}

impl Render for Confirm {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme(cx);
        let mut body = div().flex().flex_col().px(px(16.)).pt(px(16.)).child(
            div().text_size(px(14.)).font_weight(gpui::FontWeight::BOLD).text_color(t.fg).child(self.spec.title.clone()),
        );
        if !self.spec.info.is_empty() {
            body = body.child(div().pt(px(8.)).text_size(px(12.)).text_color(t.muted).child(self.spec.info.clone()));
        }
        let destructive = self.spec.destructive;
        let dialog = overlay_width(dialog_frame(&t), 520.)
            .key_context(CONTEXT)
            .track_focus(&self.focus)
            .text_color(t.fg)
            .on_action(cx.listener(move |this, _: &Accept, _, cx| {
                if !destructive {
                    this.accept(cx);
                }
            }))
            .on_action(cx.listener(|this, _: &AcceptForce, _, cx| this.accept(cx)))
            .on_action(cx.listener(|this, _: &Dismiss, _, cx| this.cancel(cx)))
            .child(body)
            .child(self.buttons(&t, cx));
        backdrop(cx.listener(|this, _, _, cx| this.cancel(cx))).child(dialog)
    }
}

impl Focusable for Confirm {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn destructive_confirmations_use_the_error_key() {
        assert_eq!(confirm_role(true), KeyRole::Destructive);
        assert_eq!(confirm_role(false), KeyRole::Primary);
    }

    fn session(id: &str, host: Option<&str>) -> Session {
        let mut s = Session::new(id, "C:/p", 1.0, "claude-uuid", "Titel");
        s.host = host.map(str::to_string);
        s
    }

    #[test]
    fn close_session_texts_follow_the_kind() {
        let claude = close_session_spec(&session("a", None));
        assert_eq!(claude.title, "„Titel“ beenden und entfernen?");
        assert!(claude.info.ends_with("claude --resume claude-uuid"));
        assert!(claude.destructive && claude.ask == Some(Ask::CloseSession));
        let shell = close_session_spec(&Session::new("shell-1", "C:/p", 1.0, "shell-1", ""));
        assert_eq!(shell.info, "Die Shell und alles, was darin läuft, wird beendet.");
        let remote = close_session_spec(&session("r", Some("box")));
        assert_eq!(remote.info, "Die Verbindung wird getrennt, die tmux-Session auf box läuft weiter.");
    }

    #[test]
    fn group_and_stop_specs() {
        let g = close_group_spec("g", "kadrell", 3);
        assert_eq!(g.title, "Gruppe „kadrell“ mit 3 Session(s) schließen?");
        assert_eq!((g.destructive, g.ask), (true, Some(Ask::CloseGroup)));
        let s = stop_session_spec(&session("a", None));
        assert_eq!((s.destructive, s.ask, s.button.as_str()), (false, Some(Ask::StopSession), "Stoppen"));
    }

    #[test]
    fn destructive_label_shows_the_platform_key() {
        let label = destructive_label("Entfernen");
        assert!(label.starts_with("Entfernen  "));
        assert!(if cfg!(target_os = "macos") { label.ends_with("⌘⏎") } else { label.ends_with("Strg+Enter") }, "{label}");
    }
}
