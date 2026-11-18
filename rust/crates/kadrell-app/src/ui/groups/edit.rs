//! Dialog „NEUE GRUPPE“ und „GRUPPE BEARBEITEN“ (`EditGroupView` in `EditGroupSheet.swift`): Name, Farbe, Ordner mit
//! Pfadkürzel (Tab schreibt aus, Strg/⌘+O wählt im Explorer/Finder, Ordner hineinziehen), die Farbpaletten stehen
//! immer offen. Die Logik steht in `model`.

use gpui::{
    App, Context, Entity, EventEmitter, ExternalPaths, FocusHandle, Focusable, InteractiveElement, IntoElement, KeyBinding,
    ParentElement, Render, StatefulInteractiveElement, Styled, Subscription, Window, actions, div, prelude::*, px,
    rgb,
};

use kadrell_core::group::{Group, PALETTES};
use kadrell_i18n::trd;

use super::files;
use super::model::{EditGroupModel, Hint, HintLine, RealFs};
use super::tool_icons;
use crate::lang::{file_manager, tx};
use crate::theme::Theme;
use crate::ui::chrome::h_seam;
use crate::ui::model::parse_color;
use crate::ui::dialog_keys;
use crate::ui::instrument::{KeyRole, focus_ring, key_focusable};
use crate::ui::overlay::{backdrop, border_px, dialog_foot_in, dialog_frame, dialog_title, overlay_width};
use crate::ui::text_input::{TextInput, TextInputEvent};
use crate::ui::theme;

actions!(kadrell_edit_group, [Complete, Browse]);

const CONTEXT: &str = "KadrellEditGroup";
const SWATCH: f32 = 18.;

pub fn bind_keys(cx: &mut App) {
    let c = Some(CONTEXT);
    let browse = if cfg!(target_os = "macos") { "cmd-o" } else { "ctrl-o" };
    dialog_keys::bind_keys(cx);
    cx.bind_keys([KeyBinding::new("tab", Complete, c), KeyBinding::new(browse, Browse, c)]);
}

pub enum EditEvent {
    Close,
    /// Gespeichert: die fertige Gruppe, `true` bei einer neuen.
    Save(Group, bool),
}

impl EventEmitter<EditEvent> for EditGroup {}

pub struct EditGroup {
    model: EditGroupModel,
    name: Entity<TextInput>,
    folder: Entity<TextInput>,
    _typing: Vec<Subscription>,
}

/// Text mit „Finder“ im Quelltext in der Sprache der App, unter Windows mit „Explorer“.
fn fm(source: &str) -> String {
    let text = tx(source);
    if cfg!(target_os = "macos") { text } else { text.replace("Finder", file_manager()) }
}

impl EditGroup {
    pub fn new(model: EditGroupModel, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let name = cx.new(|cx| TextInput::new(&trd!("Name"), cx).size(15.));
        let folder = cx.new(|cx| TextInput::new(&trd!("Ordner (~/d/p/kad kürzt ab)"), cx));
        name.update(cx, |i, cx| i.set_text(&model.name, window, cx));
        folder.update(cx, |i, cx| i.set_text(model.folder(), window, cx));
        let typing = vec![
            cx.subscribe(&name, |this, _, _: &TextInputEvent, cx| this.name_typed(cx)),
            cx.subscribe_in(&folder, window, |this, _, _: &TextInputEvent, window, cx| this.folder_typed(window, cx)),
        ];
        name.read(cx).focus.clone().focus(window, cx);
        Self { model, name, folder, _typing: typing }
    }

    fn name_typed(&mut self, cx: &mut Context<Self>) {
        self.model.name = self.name.read(cx).text().to_string();
        cx.notify();
    }

    /// Ordnertext geändert: auflösen; zieht der Name mit, steht er gleich im Namensfeld.
    fn folder_typed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.model.name = self.name.read(cx).text().to_string();
        let typed = self.folder.read(cx).text().to_string();
        self.model.set_folder(&typed, &RealFs);
        if self.name.read(cx).text() != self.model.name {
            let name = self.model.name.clone();
            self.name.update(cx, |i, cx| i.set_text(&name, window, cx));
        }
        cx.notify();
    }

    fn set_folder_text(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.folder.update(cx, |i, cx| i.set_text(text, window, cx));
    }

    /// Ein Ordner wurde hineingezogen oder gewählt.
    pub fn take_dir(&mut self, dir: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.model.is_remote() {
            return;
        }
        self.model.take_dir(dir, &RealFs);
        let text = self.model.folder().to_string();
        self.set_folder_text(&text, window, cx);
    }

    fn save(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.model.name = self.name.read(cx).text().to_string();
        if let Some(group) = self.model.save() {
            cx.emit(EditEvent::Save(group, self.model.is_new));
        }
    }

    /// Tab: im Ordnerfeld ausschreiben; gibt es nichts auszuschreiben oder steht der Fokus woanders, weiter zum nächsten
    /// Bedienelement (Ordnerfeld, Ordnertaste, Farben, Haupttaste).
    fn complete(&mut self, _: &Complete, window: &mut Window, cx: &mut Context<Self>) {
        let in_folder = !self.model.is_remote() && self.folder.read(cx).focus.is_focused(window);
        let text = if in_folder { self.model.completion(&RealFs) } else { None };
        match text {
            Some(text) => self.set_folder_text(&text, window, cx),
            None => window.focus_next(cx),
        }
    }

    /// Strg/⌘+O: Ordnerdialog des Systems.
    fn browse(&mut self, _: &Browse, window: &mut Window, cx: &mut Context<Self>) {
        if self.model.is_remote() {
            return;
        }
        crate::ui::pick::choose(false, window, cx, |this, path, window, cx| {
            if let Some(dir) = files::folder_for(std::path::Path::new(&path)) {
                this.take_dir(&dir, window, cx);
            }
        });
    }

    fn pick_color(&mut self, hex: &str, cx: &mut Context<Self>) {
        self.model.color = hex.to_string();
        cx.notify();
    }
}

impl EditGroup {
    fn hint_line(&self, t: &Theme) -> gpui::Div {
        let (text, error) = hint_text(&self.model);
        let color = if error { t.error_text } else { t.muted };
        div().px(px(16.)).pt(px(6.)).pb(px(14.)).text_size(px(11.)).text_color(color).child(text)
    }

    fn folder_row(&self, t: &Theme, window: &mut Window, cx: &mut Context<Self>) -> gpui::Div {
        let s = crate::ui::overlay::scale();
        // Kompakte neutrale Taste mit Ordnersymbol in 22 x 16 (`InstrumentKeyStyle(.neutral, compact: true)`).
        let icon = div().flex().items_center().justify_center().w(px(22. * s)).h(px(16. * s)).child(tool_icons::folder(t.key_label()));
        let tip = fm("Im Finder wählen (⌘O)");
        let browse = cx.listener(|this, _, window, cx| this.browse(&Browse, window, cx));
        let button = key_focusable("edit-group-browse", icon, KeyRole::Neutral, true, t, window, cx, browse)
            .tooltip(crate::ui::tooltip::text(crate::lang::shortcut(&tip)));
        div().flex().items_center().gap(px(6. * s)).px(px(14. * s)).pt(px(10. * s)).child(div().flex_1().child(self.folder.clone())).child(button)
    }

    fn name_row(&self, t: &Theme) -> gpui::Div {
        let color = rgb(parse_color(&self.model.color));
        let capsule = div().w(px(44.)).h(px(20.)).flex_none().rounded_full().bg(color);
        div().flex().items_center().gap(px(12.)).px(px(14.)).pt(px(14.)).child(div().flex_1().child(self.name.clone())).child(capsule).text_color(t.fg)
    }

    fn swatch(&self, p: usize, i: usize, hex: &'static str, t: &Theme, window: &mut Window, cx: &mut Context<Self>) -> gpui::Stateful<gpui::Div> {
        let on = hex.eq_ignore_ascii_case(&self.model.color);
        let ring = if on { t.fg } else { gpui::transparent_black() };
        let s = crate::ui::overlay::scale();
        // Auswahlring 1,5 pt in `fg`; Bedienhilfe-Beschriftung „<Palette> <Hex>“ gibt GPUI nicht her (keine Accessibility-API).
        let n = p * 100 + i;
        let swatch = border_px(div(), 1.5 * s)
            .id(("swatch", n))
            .relative()
            .size(px(SWATCH * s))
            .flex_none()
            .rounded(px(3. * s))
            .border_color(ring)
            .bg(rgb(parse_color(hex)))
            .cursor_pointer();
        let pick = cx.listener(move |this, _, _, cx| this.pick_color(hex, cx));
        let (swatch, focused) = dialog_keys::control(swatch, ("swatch-focus", n), window, cx, std::rc::Rc::new(pick));
        if focused { swatch.child(focus_ring(t)) } else { swatch }
    }

    fn palettes(&self, t: &Theme, window: &mut Window, cx: &mut Context<Self>) -> gpui::Div {
        let s = crate::ui::overlay::scale();
        let mut col = div().flex().flex_col().gap(px(8. * s)).px(px(16. * s)).pb(px(12. * s));
        for (p, (name, colors)) in PALETTES.iter().enumerate() {
            let mut row = div().flex().items_center().gap(px(4. * s)).child(div().w(px(110. * s)).flex_none().text_color(t.sub).child(*name));
            for (i, hex) in colors.iter().enumerate() {
                row = row.child(self.swatch(p, i, hex, t, window, cx));
            }
            col = col.child(row);
        }
        col
    }
}

/// Text und Fehlerkennzeichen der Hinweiszeile.
pub fn hint_text(model: &EditGroupModel) -> (String, bool) {
    match model.hint() {
        Hint::Remote(host) => (format!("ssh · {host}"), false),
        Hint::NotFound => (fm("Ordner nicht gefunden · Tab vervollständigt, ⌘O wählt im Finder"), true),
        Hint::Help => (fm("Tab vervollständigt · ⌘O Finder · Ordner hineinziehen"), false),
        Hint::Lines(lines) => (lines.iter().map(line_text).collect::<Vec<_>>().join("\n"), false),
    }
}

fn line_text(line: &HintLine) -> String {
    match line {
        HintLine::Resolved(path) => format!("→ {path}"),
        HintLine::Moved => trd!("Neue Sessions starten im neuen Ordner, laufende bleiben in ihrem."),
        HintLine::SharedWith(name) => trd!("Den Ordner hat auch die Gruppe „%@“.", name),
    }
}

impl Render for EditGroup {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme(cx);
        let title = if self.model.is_new { trd!("NEUE GRUPPE") } else { trd!("GRUPPE BEARBEITEN") };
        let button = if self.model.is_new { trd!("Anlegen") } else { trd!("Speichern") };
        let save = cx.listener(|this, _, w, cx| this.save(w, cx));
        let foot = dialog_foot_in(trd!("Esc abbrechen"), format!("{button} ⏎"), KeyRole::Primary, &t, window, cx, save);
        let ring = t.accent;
        let remote = self.model.is_remote();
        let dialog = overlay_width(dialog_frame(&t), 640.).key_context(dialog_keys::context(CONTEXT)).text_color(t.fg).text_size(px(12.));
        let dialog = dialog_keys::keys(dialog, true, cx, Self::save, |_, _, cx| cx.emit(EditEvent::Close))
            .on_action(cx.listener(Self::complete))
            .on_action(cx.listener(Self::browse))
            .on_drop(cx.listener(|this, paths: &ExternalPaths, window, cx| {
                if let Some(dir) = files::first_folder(paths) {
                    this.take_dir(&dir, window, cx);
                }
            }))
            .can_drop(move |_, _, _| !remote)
            // Beim Darüberziehen ein 2-pt-Akzentrand um den ganzen Dialog (Radius 14), nicht nur die Fuge.
            .drag_over::<ExternalPaths>(move |s, _, _, _| s.border_2().border_color(ring))
            .child(dialog_title(title, &t));
        // Der Inhalt scrollt, die Fußzeile steht fest darunter (`OverlayScroll`): im kleinen Fenster bleibt sie sichtbar.
        let mut body = div().id("edit-group-body").flex().flex_col().flex_1().min_h_0().overflow_y_scroll().child(self.name_row(&t));
        if !remote {
            body = body.child(self.folder_row(&t, window, cx));
        }
        let body = body.child(self.hint_line(&t)).child(self.palettes(&t, window, cx));
        let dialog = dialog.child(body).child(h_seam(&t)).child(foot);
        dialog_keys::nav(backdrop(cx.listener(|_, _, _, cx| cx.emit(EditEvent::Close)))).child(dialog)
    }
}

impl Focusable for EditGroup {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.name.read(cx).focus.clone()
    }
}
