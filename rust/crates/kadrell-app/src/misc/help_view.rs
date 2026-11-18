//! Hilfe und „Über Kadrell“ (⌥F1 und F1) als eine Ansicht wie `AboutView` der Mac-App: Version, Autor, Lizenzen und alle
//! Kürzel aus den Einstellungen. `/` filtert wie in less und vim, Esc beendet erst den Filter, dann den Dialog.

use gpui::{
    App, Context, Entity, EventEmitter, FocusHandle, Focusable, FontWeight, InteractiveElement, IntoElement, KeyBinding, KeyDownEvent,
    ParentElement, Render, ScrollHandle, StatefulInteractiveElement, Styled, Subscription, Window, actions, div, prelude::*, px,
};
use kadrell_core::Settings;
use kadrell_core::hotkeys::HotkeyAction;
use kadrell_i18n::trd;

use super::changelog;
use super::help::{self, Section};
use super::open::{self, Opening};
use crate::theme::Theme;
use crate::ui::chrome::h_seam;
use crate::ui::ext::keys;
use crate::ui::overlay::{backdrop, dialog_frame, overlay_height, overlay_width};
use crate::ui::settings::filter::Filter;
use crate::ui::text_input::{TextInput, TextInputEvent};
use crate::ui::theme;

actions!(kadrell_help, [Confirm, Cancel]);

const CONTEXT: &str = "KadrellHelp";
const WIDTH: f32 = 640.;
const HEIGHT: f32 = 760.;
/// Breite der Kürzel-Spalte, Abschnittsnamen stehen darüber in derselben Flucht.
const KEY_COLUMN: f32 = 160.;

pub const AUTHOR_URL: &str = "https://malura.de";
pub const EXTENSIONS_URL: &str = "https://kadrell.malura.de/extensions";
/// Lizenzhinweis der mitgelieferten Schrift (Lua und Farbwerte stehen im übersetzten Satz darüber).
pub const FONT_CREDIT: &str = "JetBrainsMono Nerd Font Mono (SIL OFL 1.1, JetBrains, Nerd Fonts)";
/// Wie viele Changelog-Zeilen der About-Dialog zeigt.
const EXCERPT_LINES: usize = 6;

pub fn bind_keys(cx: &mut App) {
    let c = Some(CONTEXT);
    cx.bind_keys([KeyBinding::new("enter", Confirm, c), KeyBinding::new("escape", Cancel, c)]);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// ⌥F1: Kürzel und Bedienung.
    Help,
    /// F1 und Menü „Über Kadrell“.
    About,
}

pub enum HelpEvent {
    Close,
}

impl EventEmitter<HelpEvent> for HelpView {}

pub struct HelpView {
    kind: Kind,
    focus: FocusHandle,
    sections: Vec<Section>,
    filter: Filter,
    filter_input: Entity<TextInput>,
    scroll: ScrollHandle,
    /// Kürzel, das den Dialog wieder schließt („Esc oder ⌥F1 schließen“), leer ohne Belegung.
    close_hint: String,
    _sub: Subscription,
}

impl HelpView {
    pub fn new(kind: Kind, settings: &Settings, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mac = cfg!(target_os = "macos");
        let filter_input = cx.new(|cx| TextInput::new(&trd!("Filtern …"), cx).compact());
        let sub = cx.subscribe(&filter_input, |this, input, _: &TextInputEvent, cx| {
            this.filter.query = input.read(cx).text().to_string();
            cx.notify();
        });
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        let help_key = keys::binding(settings, HotkeyAction::ShowHelp, mac).map(|k| keys::label(&k, mac));
        // Eine Ansicht für Hilfe und Über (`AboutView` der Mac-App): der Fuß nennt in beiden das Kürzel der Hilfe.
        let close_hint = help_key.unwrap_or_default();
        Self { kind, focus, sections: help::sections(settings, mac), filter: Filter::default(), filter_input, scroll: ScrollHandle::new(), close_hint, _sub: sub }
    }

    pub fn kind(&self) -> Kind {
        self.kind
    }

    fn shown(&self) -> Vec<Section> {
        if self.filter.is_active() { help::filtered(&self.sections, self.filter.needle()) } else { self.sections.clone() }
    }

    fn open_filter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.filter.open = true;
        let focus = self.filter_input.read(cx).focus.clone();
        focus.focus(window, cx);
        cx.notify();
    }

    fn close_filter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.filter = Filter::default();
        self.filter_input.update(cx, |i, cx| i.set_text("", window, cx));
        self.focus.focus(window, cx);
        cx.notify();
    }

    /// Esc: der Filter zuerst, dann der Dialog.
    fn cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.filter.open {
            self.close_filter(window, cx);
        } else {
            cx.emit(HelpEvent::Close);
        }
    }

    /// ⏎: im Filter zurück zur Liste (der Text bleibt), sonst schließen.
    fn confirm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.filter.open && self.filter_input.read(cx).focus.is_focused(window) {
            self.focus.focus(window, cx);
            cx.notify();
        } else {
            cx.emit(HelpEvent::Close);
        }
    }

    fn on_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let m = ev.keystroke.modifiers;
        if !self.filter.open && ev.keystroke.key == "/" && !m.control && !m.alt && !m.platform {
            self.open_filter(window, cx);
            cx.stop_propagation();
        }
    }

    fn foot_hint(&self) -> String {
        let closing = if self.close_hint.is_empty() { trd!("Esc schließt") } else { trd!("Esc oder %@ schließen", self.close_hint.clone()) };
        if self.filter.open { trd!("Esc beendet die Suche") } else { trd!("/ sucht · %@", closing) }
    }
}

impl HelpView {
    /// „/“, Feld und Trefferzahl (`FilterBar` der Mac-App).
    fn filter_bar(&self, hits: usize, t: &Theme) -> gpui::Div {
        let slash = div().flex_none().font_weight(FontWeight::BOLD).text_color(t.accent_text).child("/");
        let count = self.filter.is_active().then(|| {
            let text = if hits == 0 { trd!("Keine Treffer") } else { trd!("%lld Treffer", hits) };
            div().flex_none().text_color(t.muted).child(text)
        });
        let bar = div().flex().items_center().gap(px(8.)).px(px(16.)).pt(px(10.)).text_size(px(12.));
        bar.child(slash).child(div().flex_1().min_w_0().child(self.filter_input.clone())).children(count)
    }
}

impl Focusable for HelpView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

fn link(id: &'static str, label: &str, url: &'static str, t: &Theme) -> impl IntoElement + use<> {
    div().id(id).cursor_pointer().text_color(t.accent_text).child(label.to_string()).on_click(move |_, _, _| {
        let _ = open::run(&Opening::Url(url.to_string()));
    })
}

fn line(prefix: String, id: &'static str, label: &str, url: &'static str, t: &Theme) -> impl IntoElement + use<> {
    div().flex().gap(px(6.)).text_size(px(12.)).child(div().text_color(t.muted).child(prefix)).child(link(id, label, url, t))
}

fn header(t: &Theme) -> impl IntoElement + use<> {
    let title = div()
        .flex()
        .items_baseline()
        .gap(px(10.))
        .child(div().text_size(px(18.)).font_weight(FontWeight::BOLD).child("Kadrell"))
        .child(div().text_size(px(12.)).text_color(t.muted).child(format!("v{}", changelog::version())));
    let mut block = div().flex().flex_col().gap(px(4.)).px(px(16.)).pt(px(16.)).child(title);
    block = block.child(
        div()
            .pt(px(2.))
            .text_size(px(12.))
            .text_color(t.muted)
            .child(trd!("Claude-Code-Sessions als Terminals: Baum links, rechts als Grid oder Stack. Beim Beenden enden sie, beim nächsten Start geht es mit --resume weiter.")),
    );
    block = block.child(line(trd!("von Michael Malura ·"), "author", "malura.de", AUTHOR_URL, t));
    block = block.child(line(trd!("Eigene Erweiterungen in Lua:"), "extensions", "kadrell.malura.de/extensions", EXTENSIONS_URL, t));
    let credits = trd!("Enthält Lua 5.5.1 (MIT, lua.org) und json.lua (MIT, rxi) sowie Farbwerte des Schemas Instrument aus Monsters (Apache-2.0, Saturday Company)");
    block = block.child(div().text_size(px(11.)).text_color(t.muted).child(credits));
    block = block.child(div().text_size(px(11.)).text_color(t.muted).child(FONT_CREDIT));
    block
}

fn section_view(s: &Section, first: bool, t: &Theme) -> impl IntoElement + use<> {
    let title = div()
        .pl(px(KEY_COLUMN + 12.))
        .pt(px(if first { 0. } else { 12. }))
        .pb(px(2.))
        .child(crate::ui::instrument::kerned(&s.title.to_uppercase(), 11., crate::ui::instrument::LABEL_KERN, true, t.accent_text));
    let rows = s.rows.iter().map(|r| {
        div()
            .flex()
            .items_start()
            .gap(px(12.))
            .py(px(2.6))
            .child(div().flex_none().w(px(KEY_COLUMN)).text_right().font_weight(FontWeight::BOLD).child(r.keys.clone()))
            .child(div().flex_1().min_w_0().text_color(t.muted).child(r.text.clone()))
    });
    div().flex().flex_col().child(title).children(rows)
}

impl Render for HelpView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme(cx);
        let shown = self.shown();
        let filter_bar = self.filter.open.then(|| self.filter_bar(help::row_count(&shown), &t));
        let sections = shown.iter().enumerate().map(|(i, s)| section_view(s, i == 0, &t));
        let list = div().id("help-scroll").flex().flex_col().flex_1().min_h_0().overflow_y_scroll().track_scroll(&self.scroll).p(px(16.)).gap(px(5.)).children(sections);
        let foot = div().flex().justify_end().px(px(16.)).py(px(10.)).text_size(px(11.)).text_color(t.muted).child(self.foot_hint());
        let frame = overlay_height(overlay_width(dialog_frame(&t), WIDTH), HEIGHT)
            .key_context(CONTEXT)
            .track_focus(&self.focus)
            .text_color(t.fg)
            .on_action(cx.listener(|this, _: &Cancel, window, cx| this.cancel(window, cx)))
            .on_action(cx.listener(|this, _: &Confirm, window, cx| this.confirm(window, cx)))
            .on_key_down(cx.listener(|this, ev, window, cx| this.on_key(ev, window, cx)))
            .child(header(&t))
            .children(filter_bar)
            .child(div().pt(px(14.)).child(h_seam(&t)))
            .child(list)
            .child(h_seam(&t))
            .child(foot);
        backdrop(cx.listener(|_, _, _, cx| cx.emit(HelpEvent::Close))).child(frame)
    }
}
