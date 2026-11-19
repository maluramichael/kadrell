//! Rückfrage mit Titel, Text und einer Haupttaste (Ersatz für `sheets.confirm` der Mac-App): „Was ist neu“, Update-
//! Hinweis, Übernahme von Hintergrund-Sessions. Esc und Klick daneben brechen ab. Bei `destructive` (laufende Arbeit
//! geht verloren) gilt nicht ⏎ allein, sondern ⌘⏎ (Strg+⏎), wie überall sonst in der App.

use gpui::{
    App, Context, EventEmitter, FocusHandle, Focusable, InteractiveElement, IntoElement, ParentElement, Render, SharedString, StatefulInteractiveElement, Styled,
    Window, div, px,
};
use kadrell_core::settings::Ask;
use kadrell_i18n::trd;

use crate::lang::shortcut;
use crate::theme::Theme;
use crate::ui::groups::confirm::checkbox;

use crate::ui::chrome::h_seam;
use crate::ui::dialog_keys;
use crate::ui::instrument::KeyRole;
use crate::ui::overlay::{backdrop, dialog_foot_in, dialog_frame, dialog_title, overlay_width};
use crate::ui::theme;

const CONTEXT: &str = "KadrellConfirm";

/// Enter, Strg/⌘+Enter, Esc und Tab laufen über die gemeinsame Dialogtastatur (`dialog_keys`).
pub fn bind_keys(cx: &mut App) {
    dialog_keys::bind_keys(cx);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmEvent {
    Confirmed,
    Cancelled,
    /// Häkchen „Nicht mehr fragen“ gesetzt oder entfernt: die Einstellung gilt sofort (`ask.<name>`).
    DontAsk(Ask, bool),
}

impl EventEmitter<ConfirmEvent> for ConfirmDialog {}

/// Was der Dialog zeigt und tut.
pub struct Spec {
    pub title: String,
    pub text: String,
    pub button: String,
    pub destructive: bool,
    /// Ohne Abbrechen (reine Mitteilung wie „Was ist neu“): Esc, ⏎ und Klick daneben schließen.
    pub notice: bool,
    /// Häkchen „Nicht mehr fragen“ für diese Rückfrage (`ask.*`), `None` ohne.
    pub ask: Option<Ask>,
}

pub struct ConfirmDialog {
    focus: FocusHandle,
    title: SharedString,
    text: SharedString,
    button: SharedString,
    destructive: bool,
    notice: bool,
    ask: Option<Ask>,
    dont_ask: bool,
}

/// Darf ⏎ allein bestätigen? Nicht, wenn laufende Arbeit abbrechen würde.
pub fn enter_confirms(destructive: bool) -> bool {
    !destructive
}

impl ConfirmDialog {
    pub fn new(spec: Spec, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        Self { focus, title: spec.title.into(), text: spec.text.into(), button: spec.button.into(), destructive: spec.destructive, notice: spec.notice, ask: spec.ask, dont_ask: false }
    }

    fn confirm(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(ConfirmEvent::Confirmed);
    }

    fn cancel(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        // Abbrechen stellt zurück: wer abbricht, will weiter gefragt werden.
        if let (true, Some(ask)) = (self.dont_ask, self.ask) {
            cx.emit(ConfirmEvent::DontAsk(ask, false));
        }
        cx.emit(if self.notice { ConfirmEvent::Confirmed } else { ConfirmEvent::Cancelled });
    }

    fn toggle_dont_ask(&mut self, cx: &mut Context<Self>) {
        self.dont_ask = !self.dont_ask;
        if let Some(ask) = self.ask {
            cx.emit(ConfirmEvent::DontAsk(ask, self.dont_ask));
        }
        cx.notify();
    }

    /// Häkchen „Nicht mehr fragen“ unter dem Text, ein Tab-Stopp (Leertaste und Enter schalten um).
    fn ask_row(&self, t: &Theme, window: &mut Window, cx: &mut Context<Self>) -> gpui::Stateful<gpui::Div> {
        let color = if self.dont_ask { t.fg } else { t.muted };
        let row = div()
            .id("confirm-dont-ask")
            .relative()
            .flex()
            .items_center()
            .gap(px(6.))
            .cursor_pointer()
            .text_size(px(12.))
            .text_color(color)
            .tooltip(crate::ui::tooltip::text(shortcut(&trd!("Wieder einschalten: Einstellungen (⌘,) › Rückfragen"))))
            .child(checkbox(self.dont_ask, t))
            .child(trd!("Nicht mehr fragen"));
        let toggle = cx.listener(|this, _, _, cx| this.toggle_dont_ask(cx));
        let (row, ring) = dialog_keys::control(row, "confirm-dont-ask", window, cx, std::rc::Rc::new(toggle));
        if ring { row.child(crate::ui::instrument::focus_ring(t)) } else { row }
    }

    fn hint(&self) -> String {
        match (self.notice, self.destructive) {
            (true, _) => trd!("Esc schließt"),
            (false, true) => format!("{} · {}", trd!("Esc schließt"), super::os_text("⌘⏎")),
            (false, false) => trd!("Esc schließt"),
        }
    }
}

impl Focusable for ConfirmDialog {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for ConfirmDialog {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme(cx);
        // „<Taste>  ⌘⏎“ in der destruktiven Rolle, sonst „<Taste>  ⏎“ (die Fußzeile hängt es an).
        let label = if self.destructive { format!("{}  {}", self.button, super::os_text("⌘⏎")) } else { self.button.to_string() };
        let role = if self.destructive { KeyRole::Destructive } else { KeyRole::Primary };
        let accept = cx.listener(|this, _, w, cx| this.confirm(w, cx));
        let foot = dialog_foot_in(self.hint(), label, role, &t, window, cx, accept);
        let mut body = div().flex_none().flex().flex_col().gap(px(12.)).p(px(16.)).text_size(px(12.)).line_height(px(18.)).child(self.text.clone());
        if self.ask.is_some() {
            body = body.child(self.ask_row(&t, window, cx));
        }
        let dialog = overlay_width(dialog_frame(&t), 480.).key_context(dialog_keys::context(CONTEXT)).track_focus(&self.focus).text_color(t.fg);
        let dialog = dialog_keys::keys(dialog, enter_confirms(self.destructive), cx, Self::confirm, Self::cancel)
            .child(dialog_title(self.title.to_uppercase(), &t))
            .child(body)
            .child(h_seam(&t))
            .child(foot);
        dialog_keys::nav(backdrop(cx.listener(|this, _, w, cx| this.cancel(w, cx)))).child(dialog)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enter_alone_never_confirms_destructive_actions() {
        assert!(enter_confirms(false));
        assert!(!enter_confirms(true));
    }
}
