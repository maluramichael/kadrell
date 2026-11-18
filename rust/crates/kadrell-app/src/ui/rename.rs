//! Dialog „Umbenennen“ für Sessions (F2) und Gruppen: ein Feld, Enter übernimmt, Esc schließt. Leerer Name setzt
//! bei einer Session wieder den Titel von Claude Code (wie `rename` im Steuerkanal).

use gpui::{
    App, Context, Entity, EventEmitter, FocusHandle, Focusable, InteractiveElement, IntoElement, KeyBinding, ParentElement, Render,
    Styled, Window, actions, div, prelude::*, px,
};

use kadrell_i18n::trd;
use super::chrome::h_seam;
use super::overlay::{backdrop, dialog_foot, dialog_frame, dialog_title, overlay_width};
use super::text_input::TextInput;
use super::theme;
use crate::Shell;

actions!(kadrell_rename, [Confirm, Cancel]);

const CONTEXT: &str = "KadrellRename";

pub fn bind_keys(cx: &mut App) {
    let c = Some(CONTEXT);
    cx.bind_keys([KeyBinding::new("enter", Confirm, c), KeyBinding::new("escape", Cancel, c)]);
}

/// Was umbenannt wird.
#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    Session(String),
    Group(String),
}

pub enum RenameEvent {
    Close,
    Apply(Target, String),
}

impl EventEmitter<RenameEvent> for Rename {}

pub struct Rename {
    input: Entity<TextInput>,
    target: Target,
    heading: String,
}

impl Rename {
    pub fn new(target: Target, heading: &str, current: &str, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| TextInput::new(&trd!("Name"), cx));
        input.update(cx, |i, cx| i.set_text(current, window, cx));
        let focus = input.read(cx).focus.clone();
        focus.focus(window, cx);
        Self { input, target, heading: heading.to_string() }
    }

    fn confirm(&mut self, _: &Confirm, _: &mut Window, cx: &mut Context<Self>) {
        let name = self.input.read(cx).text().trim().to_string();
        cx.emit(RenameEvent::Apply(self.target.clone(), name));
    }
}

impl Render for Rename {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme(cx);
        let foot = dialog_foot(trd!("⏎ übernehmen · Esc schließen"), format!("{} ⏎", trd!("Übernehmen")), &t, cx.listener(|this, _, w, cx| this.confirm(&Confirm, w, cx)));
        let dialog = overlay_width(dialog_frame(&t), 420.)
            .key_context(CONTEXT)
            .text_color(t.fg)
            .on_action(cx.listener(Self::confirm))
            .on_action(cx.listener(|_, _: &Cancel, _, cx| cx.emit(RenameEvent::Close)))
            .child(dialog_title(self.heading.to_uppercase(), &t))
            .child(div().flex_none().p(px(14.)).child(self.input.clone()))
            .child(h_seam(&t))
            .child(foot);
        backdrop(cx.listener(|_, _, _, cx| cx.emit(RenameEvent::Close))).child(dialog)
    }
}

impl Focusable for Rename {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.input.read(cx).focus.clone()
    }
}

// Anbindung an die Shell.

impl Shell {
    /// Öffnet den Dialog für Session oder Gruppe; ein offener Dialog bleibt, wie er ist.
    pub fn open_rename(&mut self, target: Target, window: &mut Window, cx: &mut Context<Self>) {
        if self.rename.is_some() || self.overlay.is_some() || self.palette.is_some() {
            return;
        }
        let (heading, current) = match &target {
            Target::Session(key) => (trd!("Session umbenennen"), self.state.session(key).map(|s| s.title()).unwrap_or_default()),
            Target::Group(id) => (trd!("Gruppe umbenennen"), self.state.store.groups.group(id).map(|g| g.name.clone()).unwrap_or_default()),
        };
        let dialog = cx.new(|cx| Rename::new(target, &heading, &current, window, cx));
        cx.subscribe_in(&dialog, window, |this, _, ev: &RenameEvent, window, cx| {
            this.rename = None;
            if let RenameEvent::Apply(target, name) = ev {
                this.apply_rename(target, name);
            }
            this.sync(window, cx);
        })
        .detach();
        self.rename = Some(dialog);
        cx.notify();
    }

    fn apply_rename(&mut self, target: &Target, name: &str) {
        match target {
            Target::Session(key) => self.state.rename_session(key, name),
            Target::Group(id) if !name.is_empty() => self.state.store.groups.rename(id, name),
            Target::Group(_) => {}
        }
    }
}
