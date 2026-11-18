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
    /// Titel von Claude Code (nur Sessions): Platzhalter des Feldes und Text der Hilfezeile.
    auto_title: Option<String>,
}

/// Hilfezeile unter dem Feld: leer lassen heißt wieder der Titel von Claude Code (`RenameSessionView`).
pub fn help_line(auto_title: &str) -> String {
    trd!("Leer lassen: wieder der Titel von Claude Code (%@)", auto_title)
}

impl Rename {
    pub fn new(target: Target, heading: &str, current: &str, auto_title: Option<String>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        // Schrift 15 wie in der Mac-App; der Platzhalter ist der automatische Titel, bei Gruppen „Name“.
        let placeholder = auto_title.clone().unwrap_or_else(|| trd!("Name"));
        let input = cx.new(|cx| TextInput::new(&placeholder, cx).size(15.));
        input.update(cx, |i, cx| i.set_text(current, window, cx));
        let focus = input.read(cx).focus.clone();
        focus.focus(window, cx);
        Self { input, target, heading: heading.to_string(), auto_title }
    }

    fn confirm(&mut self, _: &Confirm, _: &mut Window, cx: &mut Context<Self>) {
        let name = self.input.read(cx).text().trim().to_string();
        cx.emit(RenameEvent::Apply(self.target.clone(), name));
    }
}

impl Render for Rename {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme(cx);
        let s = super::overlay::scale();
        let foot = dialog_foot(trd!("Esc abbrechen"), trd!("Speichern"), &t, cx.listener(|this, _, w, cx| this.confirm(&Confirm, w, cx)));
        let help = self.auto_title.as_deref().map(|auto| {
            div().flex_none().w_full().px(px(16. * s)).pb(px(12. * s)).text_size(px(11. * s)).text_color(t.muted).child(help_line(auto))
        });
        let dialog = overlay_width(dialog_frame(&t), 640.)
            .key_context(CONTEXT)
            .text_color(t.fg)
            .on_action(cx.listener(Self::confirm))
            .on_action(cx.listener(|_, _: &Cancel, _, cx| cx.emit(RenameEvent::Close)))
            .child(dialog_title(self.heading.to_uppercase(), &t))
            .child(div().flex_none().p(px(14. * s)).child(self.input.clone()))
            .children(help)
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
        if self.rename.is_some() || self.overlay.is_some() {
            return;
        }
        self.present_overlay(Box::new(move |shell, window, cx| shell.show_rename(target, window, cx)), window, cx);
    }

    fn show_rename(&mut self, target: Target, window: &mut Window, cx: &mut Context<Self>) {
        let (heading, current, auto) = match &target {
            Target::Session(key) => {
                let session = self.state.session(key);
                (trd!("Session umbenennen"), session.map(|s| s.title()).unwrap_or_default(), Some(session.map(|s| s.auto_title()).unwrap_or_default()))
            }
            Target::Group(id) => (trd!("Gruppe umbenennen"), self.state.store.groups.group(id).map(|g| g.name.clone()).unwrap_or_default(), None),
        };
        let dialog = cx.new(|cx| Rename::new(target, &heading, &current, auto, window, cx));
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

#[cfg(test)]
mod tests {
    use super::*;

    /// `RenameSessionView`: Hilfezeile nennt den Titel von Claude Code.
    #[test]
    fn help_line_names_the_claude_title() {
        assert_eq!(help_line("Titel"), "Leer lassen: wieder der Titel von Claude Code (Titel)");
    }
}
