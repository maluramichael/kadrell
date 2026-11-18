//! Rückfrage mit Titel, Text und einer Haupttaste (Ersatz für `sheets.confirm` der Mac-App): „Was ist neu“, Update-
//! Hinweis, Übernahme von Hintergrund-Sessions. Esc und Klick daneben brechen ab. Bei `destructive` (laufende Arbeit
//! geht verloren) gilt nicht ⏎ allein, sondern ⌘⏎ (Strg+⏎), wie überall sonst in der App.

use gpui::{
    App, Context, EventEmitter, FocusHandle, Focusable, InteractiveElement, IntoElement, ParentElement, Render, SharedString, Styled,
    Window, div, px,
};
use kadrell_i18n::trd;

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
}

pub struct ConfirmDialog {
    focus: FocusHandle,
    title: SharedString,
    text: SharedString,
    button: SharedString,
    destructive: bool,
    notice: bool,
}

/// Darf ⏎ allein bestätigen? Nicht, wenn laufende Arbeit abbrechen würde.
pub fn enter_confirms(destructive: bool) -> bool {
    !destructive
}

impl ConfirmDialog {
    pub fn new(spec: Spec, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        Self { focus, title: spec.title.into(), text: spec.text.into(), button: spec.button.into(), destructive: spec.destructive, notice: spec.notice }
    }

    fn confirm(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(ConfirmEvent::Confirmed);
    }

    fn cancel(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(if self.notice { ConfirmEvent::Confirmed } else { ConfirmEvent::Cancelled });
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
        let body = div().flex_none().p(px(16.)).text_size(px(12.)).line_height(px(18.)).child(self.text.clone());
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
