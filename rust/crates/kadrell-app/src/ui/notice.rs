//! Meldung mit Titel, Text und „Nicht mehr anzeigen“ (Ersatz für den `NSAlert` der Mac-App): Account automatisch
//! gewechselt, 7-Tage-Limit läuft zu schnell voll, kein angemeldeter Account. ⏎ oder Esc schließen.

use gpui::{
    App, Context, EventEmitter, FocusHandle, Focusable, InteractiveElement, IntoElement, KeyBinding, ParentElement, Render, SharedString,
    Styled, Window, actions, div, prelude::*, px,
};
use kadrell_i18n::trd;

use super::chrome::h_seam;
use super::dialog_keys;
use super::instrument::{KeyRole, focus_ring};
use super::overlay::{backdrop, dialog_foot_in, dialog_frame, dialog_title, overlay_width};
use super::theme;

actions!(kadrell_notice, [Close]);

const CONTEXT: &str = "KadrellNotice";

pub fn bind_keys(cx: &mut App) {
    dialog_keys::bind_keys(cx);
    cx.bind_keys([KeyBinding::new("enter", Close, Some(CONTEXT)), KeyBinding::new("escape", Close, Some(CONTEXT))]);
}

pub enum NoticeEvent {
    /// `suppress`: der Haken „Nicht mehr anzeigen“ war gesetzt.
    Close { suppress: bool },
}

impl EventEmitter<NoticeEvent> for NoticeDialog {}

pub struct NoticeDialog {
    focus: FocusHandle,
    title: SharedString,
    text: SharedString,
    /// `None`: die Meldung lässt sich nicht abschalten, es gibt keinen Haken.
    suppress: Option<bool>,
}

impl NoticeDialog {
    pub fn new(title: String, text: String, can_suppress: bool, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        Self { focus, title: title.into(), text: text.into(), suppress: can_suppress.then_some(false) }
    }

    fn close(&mut self, cx: &mut Context<Self>) {
        cx.emit(NoticeEvent::Close { suppress: self.suppress == Some(true) });
    }
}

impl Focusable for NoticeDialog {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for NoticeDialog {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme(cx);
        let check = self.suppress.map(|on| {
            let boxed = div().size(px(14.)).flex_none().rounded(px(3.)).border_1().border_color(t.muted).when(on, |d| d.bg(t.accent).border_color(t.accent));
            let row = div()
                .id("suppress")
                .relative()
                .self_start()
                .flex()
                .items_center()
                .gap(px(8.))
                .cursor_pointer()
                .text_size(px(12.))
                .text_color(t.sub)
                .child(boxed)
                .child(trd!("Nicht mehr anzeigen"));
            let toggle = cx.listener(|this, _, _, cx| {
                this.suppress = this.suppress.map(|v| !v);
                cx.notify();
            });
            let (row, ring) = dialog_keys::control(row, "notice-suppress", window, cx, std::rc::Rc::new(toggle));
            if ring { row.child(focus_ring(&t)) } else { row }
        });
        let body = div().flex().flex_col().gap(px(12.)).p(px(16.)).child(div().text_size(px(12.)).child(self.text.clone())).children(check);
        let on_foot = cx.listener(|this, _, _, cx| this.close(cx));
        let foot = dialog_foot_in("", "OK ⏎", KeyRole::Primary, &t, window, cx, on_foot);
        let dialog = overlay_width(dialog_frame(&t), 440.)
            .key_context(CONTEXT)
            .track_focus(&self.focus)
            .text_color(t.fg)
            .on_action(cx.listener(|this, _: &Close, _, cx| this.close(cx)))
            .child(dialog_title(self.title.to_uppercase(), &t))
            .child(body)
            .child(h_seam(&t))
            .child(foot);
        dialog_keys::nav(backdrop(cx.listener(|this, _, _, cx| this.close(cx)))).child(dialog)
    }
}
