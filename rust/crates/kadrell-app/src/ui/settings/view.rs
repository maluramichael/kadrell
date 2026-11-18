//! Zeichnen des Einstellungsdialogs: Kopf, Filterzeile, zwei Spalten mit Abschnitten in einem Scrollbereich,
//! Fußzeile und die offene Auswahlliste. Alles wird in jedem Frame aus den Daten (`sections`) neu gebaut, die Größe
//! kommt aus dem aktuellen Fenster (Overlays folgen der Fenstergröße).

use gpui::{
    Anchor, AnyElement, ClickEvent, Context, Div, InteractiveElement, IntoElement, MouseButton, MouseDownEvent,
    MouseMoveEvent, ParentElement, Render, SharedString, StatefulInteractiveElement, Styled, Window, anchored, deferred,
    div, point, prelude::*, px,
};

use kadrell_i18n::trd;
use super::controls::{self, key_cap, key_content, keyboard_ring, measure, section_heading, select_face, subtitle, table, value_face, wide_row};
use super::dialog::SettingsDialog;
use super::model::{Block, Btn, Control, Row, Tone};
use super::shortcut;
use super::store::AppSettings;
use super::{layout, model};
use crate::theme::Theme;
use crate::ui::chrome::h_seam;
use crate::ui::overlay::{backdrop, dialog_foot, dialog_frame};
use crate::ui::theme;
use kadrell_core::hotkeys::{self, HotkeyAction};

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Abstand des Dialogs zum Fensterrand; oben bleibt die Titelleiste frei.
const MARGIN: f32 = 10.;
const TOP_INSET: f32 = 40.;

fn tone_color(tone: Tone, t: &Theme) -> gpui::Hsla {
    match tone {
        Tone::Normal => t.fg,
        Tone::Accent => t.accent_text,
        Tone::Muted => t.muted,
        Tone::Error => t.error_text,
    }
}

/// Balken eines Downloads: gefüllt nach `fraction`, ohne Gesamtgröße ein kurzes Stück als „läuft“-Anzeige.
fn progress_bar(fraction: Option<f64>, t: &Theme) -> Div {
    let filled = fraction.map_or(0.15, |f| f.clamp(0., 1.)) as f32;
    let track = div().flex().h(px(3.)).rounded(px(2.)).bg(t.well()).child(div().h_full().rounded(px(2.)).bg(t.accent).w(gpui::relative(filled)));
    div().px(px(10.)).pb(px(6.)).child(track)
}

fn id(prefix: &str, row: &str) -> SharedString {
    SharedString::from(format!("{prefix}-{row}"))
}

impl SettingsDialog {
    // MARK: Zeilen

    fn button(&self, row: &str, index: usize, btn: &Btn, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let color = if btn.destructive { t.pill_text_on(t.error) } else { t.key_label() };
        let content = key_content(&btn.label, btn.icon, color);
        let btn = btn.clone();
        key_cap(content, btn.destructive, t)
            .id(id(&format!("btn{index}"), row))
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.press(&btn, window, cx)))
            .into_any_element()
    }

    fn select_view(&self, row: &Row, label: &str, focused: bool, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let open = self.popover.as_ref().is_some_and(|p| p.row == row.id);
        let target = row.clone();
        div()
            .id(id("sel", &row.id))
            .relative()
            .cursor_pointer()
            .child(select_face(label, focused || open, t))
            .child(measure(&self.geometry, format!("face:{}", row.id)))
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.open_select(&target, window, cx)))
            .into_any_element()
    }

    fn slider_view(&self, row: &Row, shown: f64, bounds: (f64, f64, &str), t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let (min, max, unit) = bounds;
        let text = format!("{} {unit}", shown.round() as i64);
        let frac = if max > min { ((shown - min) / (max - min)) as f32 } else { 0. };
        let (track, value) = controls::slider(frac, &text, &self.geometry, format!("track:{}", row.id), t);
        let target = row.clone();
        let track = track.id(id("trk", &row.id)).on_mouse_down(MouseButton::Left, cx.listener(move |this, ev: &MouseDownEvent, _, cx| this.begin_drag(&target, ev.position.x, cx)));
        div().flex().items_center().gap(px(10.)).child(track).child(value).into_any_element()
    }

    fn text_view(&self, row: &Row, buttons: &[Btn], t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let input = self.inputs.get(&row.id).cloned();
        let keys: Vec<AnyElement> = buttons.iter().enumerate().map(|(i, b)| self.button(&row.id, i, b, t, cx)).collect();
        div().flex().items_center().gap(px(4.)).child(div().flex_1().min_w_0().children(input)).children(keys).into_any_element()
    }

    /// Text links, Tasten rechts: alle Tasten bis auf die letzte stehen links neben dem Text, die letzte rechts außen.
    fn info_view(&self, row: &Row, text: &str, tone: Tone, buttons: &[Btn], t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let last = buttons.len().saturating_sub(1);
        let mut left = div().flex().items_center().gap(px(10.)).flex_1().min_w_0();
        if !text.is_empty() {
            let label = div().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis().text_color(tone_color(tone, t)).child(text.to_string());
            left = left.child(label);
        }
        for (i, b) in buttons.iter().enumerate().take(last) {
            left = left.child(self.button(&row.id, i, b, t, cx));
        }
        let right = buttons.last().map(|b| self.button(&row.id, last, b, t, cx));
        div().flex().items_center().gap(px(8.)).child(left).children(right).into_any_element()
    }

    fn control_view(&self, row: &Row, focused: bool, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        match &row.control {
            Control::Select { options, value, .. } => {
                let label = options.iter().find(|(v, _)| v == value).map_or("–", |(_, l)| l.as_str());
                self.select_view(row, label, focused, t, cx)
            }
            Control::Slider { value, min, max, unit, .. } => {
                let shown = self.draft.as_ref().filter(|(r, _)| *r == row.id).map_or(*value, |(_, v)| *v);
                self.slider_view(row, shown, (*min, *max, unit), t, cx)
            }
            Control::Text { buttons, .. } => self.text_view(row, buttons, t, cx),
            Control::Info { text, tone, buttons } => self.info_view(row, text, *tone, buttons, t, cx),
            Control::Hotkey(_) | Control::Model { .. } | Control::Note(_) | Control::Pull { .. } => div().into_any_element(),
        }
    }

    fn hotkey_row(&self, row: &Row, action: HotkeyAction, ctl_w: f32, focused: bool, t: &Theme, cx: &mut Context<Self>) -> Div {
        let settings = &cx.global::<AppSettings>().settings;
        let key = hotkeys::current(settings).remove(&action);
        let recording = self.recording == Some(action);
        let default = action.default_key();
        let (text, color) = match (&key, recording) {
            (_, true) => (trd!("Tasten drücken …"), t.accent_text),
            (Some(k), false) => (shortcut::format(k, shortcut::current()), t.fg),
            (None, false) => ("–".to_string(), t.fg),
        };
        let reset = (key.as_ref() != Some(&default) && !recording).then(|| {
            let cap = key_cap(div().child(trd!("Standard")), false, t).id(id("reset", &row.id));
            cap.on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.reset_hotkey(action, cx)))
        });
        let face = div()
            .id(id("hk", &row.id))
            .relative()
            .flex_none()
            .w(px(ctl_w))
            .cursor_pointer()
            .child(value_face(&text, color, true, focused || recording, t))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                if this.recording == Some(action) {
                    this.stop_recording();
                    cx.notify();
                } else {
                    this.start_recording(action, cx);
                }
            }));
        let face = if focused { face.child(keyboard_ring(t)) } else { face };
        let title = div().flex_1().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis().text_color(t.fg).child(row.title.clone());
        div().flex().items_center().gap(px(8.)).px(px(10.)).py(px(5.)).text_size(px(12.)).child(title).children(reset).child(face)
    }

    fn model_row(&self, row: &Row, detail: &str, buttons: &[Btn], t: &Theme, cx: &mut Context<Self>) -> Div {
        let name = div().flex_1().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis().text_color(t.fg).child(row.title.clone());
        // Lange Beschreibungen (Suchtreffer) kürzen, der Name behält mindestens Platz für den Anfang.
        let detail = div().min_w_0().max_w(px(300.)).overflow_hidden().whitespace_nowrap().text_ellipsis().text_color(t.muted).child(detail.to_string());
        let name = name.min_w(px(90.)).max_w(px(320.));
        let keys: Vec<AnyElement> = buttons.iter().enumerate().map(|(i, b)| self.button(&row.id, i, b, t, cx)).collect();
        wide_row(div().child(name).child(div().flex_1().min_w_0().flex().justify_end().child(detail)).children(keys))
    }

    /// Download: Name, Fortschritt (oder Fehler) und Taste in einer Zeile, darunter der Balken.
    fn pull_row(&self, row: &Row, state: (&str, Tone, Option<f64>), buttons: &[Btn], t: &Theme, cx: &mut Context<Self>) -> Div {
        let (text, tone, fraction) = state;
        let name = div().flex_1().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis().text_color(t.fg).child(row.title.clone());
        let detail = div().flex_none().max_w(px(260.)).overflow_hidden().whitespace_nowrap().text_ellipsis().text_color(tone_color(tone, t)).child(text.to_string());
        let keys: Vec<AnyElement> = buttons.iter().enumerate().map(|(i, b)| self.button(&row.id, i, b, t, cx)).collect();
        let line = wide_row(div().child(name).child(detail).children(keys));
        let failed = tone == Tone::Error;
        let bar = (!failed).then(|| progress_bar(fraction, t));
        div().flex().flex_col().child(line).children(bar)
    }

    fn note_row(text: &str, t: &Theme) -> Div {
        div().px(px(10.)).py(px(6.)).text_size(px(11.)).text_color(t.muted).child(text.to_string())
    }

    fn row_view(&self, row: &Row, ctl_w: f32, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let focused = self.cursor.as_deref() == Some(row.id.as_str());
        let body = match &row.control {
            Control::Note(text) => Self::note_row(text, t),
            Control::Model { detail, buttons } => self.model_row(row, detail, buttons, t, cx),
            Control::Pull { text, tone, fraction, buttons } => self.pull_row(row, (text, *tone, *fraction), buttons, t, cx),
            Control::Hotkey(action) => self.hotkey_row(row, *action, ctl_w, focused, t, cx),
            _ => {
                let control = div().relative().child(self.control_view(row, focused, t, cx)).when(focused, |d| d.child(keyboard_ring(t)));
                controls::row(&row.title, control.into_any_element(), ctl_w, t)
            }
        };
        body.relative().child(measure(&self.geometry, format!("row:{}", row.id))).into_any_element()
    }

    // MARK: Abschnitte und Spalten

    fn block_view(&self, block: &Block, rows: &[usize], ctl_w: f32, t: &Theme, cx: &mut Context<Self>) -> Div {
        let mut view = div().flex().flex_col().w_full();
        if let Some((title, note)) = &block.heading {
            view = view.child(section_heading(title, note, t));
        }
        if let Some(sub) = &block.subtitle {
            view = view.child(subtitle(sub, block.heading.is_some(), t));
        }
        let rows: Vec<AnyElement> = rows.iter().map(|&i| self.row_view(&block.rows[i], ctl_w, t, cx)).collect();
        view.child(table(rows, t))
    }

    fn columns(&self, blocks: &[Block], shown: &[(usize, Vec<usize>)], width: f32, t: &Theme, cx: &mut Context<Self>) -> Div {
        let count = layout::count(width);
        let ctl_w = layout::control_width(width / count as f32);
        let heights: Vec<f32> = shown.iter().map(|(b, rows)| blocks[*b].estimated_height(rows.len())).collect();
        let left = if count == 2 { layout::split(&heights, layout::SECTION_SPACING) } else { shown.len() };
        let mut column = |part: &[(usize, Vec<usize>)]| {
            let views: Vec<Div> = part.iter().map(|(b, rows)| self.block_view(&blocks[*b], rows, ctl_w, t, cx)).collect();
            div().flex().flex_col().gap(px(layout::SECTION_SPACING)).flex_1().min_w_0().children(views)
        };
        let first = column(&shown[..left]);
        let mut body = div().flex().items_start().w_full().child(first);
        if count == 2 {
            body = body.child(column(&shown[left..]));
        }
        body
    }

    // MARK: Kopf, Filter, Liste

    fn header(t: &Theme) -> Div {
        let label = div().text_size(px(11.)).text_color(t.muted).child(trd!("EINSTELLUNGEN"));
        let version = div().text_size(px(11.)).text_color(t.muted).child(format!("Kadrell v{VERSION}"));
        div().flex().flex_none().items_center().justify_between().px(px(16.)).pt(px(12.)).child(label).child(version)
    }

    fn filter_bar(&self, hits: usize, t: &Theme) -> Option<Div> {
        if !self.filter.open {
            return None;
        }
        let slash = div().flex_none().font_weight(gpui::FontWeight::BOLD).text_color(t.accent_text).child("/");
        let count = self.filter.is_active().then(|| {
            let text = if hits == 0 { trd!("Keine Treffer") } else { trd!("%lld Treffer", hits) };
            div().flex_none().text_color(t.muted).child(text)
        });
        let bar = div().flex().flex_none().items_center().gap(px(8.)).px(px(16.)).pt(px(10.)).text_size(px(12.));
        Some(bar.child(slash).child(div().flex_1().min_w_0().child(self.filter_input.clone())).children(count))
    }

    fn popover_view(&self, t: &Theme, cx: &mut Context<Self>) -> Option<AnyElement> {
        let p = self.popover.as_ref()?;
        let target = self.geometry.borrow().get(&format!("face:{}", p.row)).copied()?;
        let mut list = controls::popover(f32::from(target.size.width), t).id("popover-list").overflow_y_scroll().track_scroll(&self.pop_scroll).occlude();
        list = list.on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());
        for (i, (value, label)) in p.options.iter().enumerate() {
            let item = controls::popover_item(label, i == p.cursor, *value == p.value, t);
            let item = item
                .id(("opt", i))
                .cursor_pointer()
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.pick(i, cx)))
                .on_mouse_move(cx.listener(move |this, _: &MouseMoveEvent, _, cx| this.hover_option(i, cx)));
            list = list.child(item);
        }
        let placed = anchored().anchor(Anchor::TopLeft).attach(target, Anchor::BottomLeft).offset(point(px(0.), px(2.))).snap_to_window_with_margin(px(8.)).child(list);
        Some(deferred(placed).priority(1).into_any_element())
    }
}

impl Render for SettingsDialog {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme(cx);
        let blocks = self.blocks(cx);
        self.ensure_inputs(&blocks, window, cx);
        let shown = model::visible(&blocks, &self.filter);
        let hits = model::hit_count(&blocks, &self.filter);
        // Jeden Frame aus dem Fenster gerechnet, nie einmal beim Öffnen.
        let width = f32::from(window.viewport_size().width) - 2. * MARGIN;
        let body = self.columns(&blocks, &shown, width, &t, cx);
        let scroll = div()
            .id("settings-scroll")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .on_scroll(cx.listener(|this, _, _, cx| this.close_popover(cx)))
            .child(body)
            .child(div().h(px(12.)));
        let hint = if self.filter.open { trd!("⏎ zum ersten Treffer · Esc beendet die Suche") } else { trd!("Änderungen gelten sofort · / sucht · Esc schließt") };
        let foot = dialog_foot(hint, format!("{} ⏎", trd!("Fertig")), &t, cx.listener(|this, _, _, cx| {
            this.stop_recording();
            cx.emit(super::dialog::SettingsEvent::Close);
        }));
        let frame = dialog_frame(&t)
            .size_full()
            .key_context(super::CONTEXT)
            .track_focus(&self.focus)
            .text_color(t.fg)
            .child(Self::header(&t))
            .children(self.filter_bar(hits, &t))
            .child(div().pt(px(12.)).child(h_seam(&t)))
            .child(scroll)
            .child(foot)
            .children(self.popover_view(&t, cx));
        let frame = super::attach_handlers(frame, cx);
        backdrop(cx.listener(|_, _, _, cx| cx.emit(super::dialog::SettingsEvent::Close)))
            .pt(px(TOP_INSET))
            .px(px(MARGIN))
            .pb(px(MARGIN))
            .child(frame)
    }
}
