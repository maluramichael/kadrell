//! Einzeiliges Textfeld im Kadrell-Look (eingelassene Fläche, Akzentring bei Fokus). Aufbau nach
//! `examples/input.rs` von gpui: `EntityInputHandler` für Eingabe, IME und markierten Text, dazu Auswahl, Wortsprung,
//! Einfügen. Zeichen werden als `char` gezählt (keine Graphem-Cluster), das reicht für Pfade und Namen.

use std::ops::Range;

use gpui::{
    App, Bounds, ClipboardItem, Context, CursorStyle, ElementId, ElementInputHandler, Entity, EntityInputHandler,
    EventEmitter, FocusHandle, Focusable, GlobalElementId, Hsla, KeyBinding, LayoutId, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, PaintQuad, Pixels, Point, ShapedLine, SharedString, Style, TextRun, UTF16Selection,
    UnderlineStyle, Window, actions, div, fill, point, prelude::*, px, relative, size,
};

use super::chrome::seam_dark;
use super::theme;
use crate::theme::Theme;

actions!(
    kadrell_text_input,
    [
        Backspace, Delete, Left, Right, SelectLeft, SelectRight, WordLeft, WordRight, SelectWordLeft, SelectWordRight,
        DeleteWordLeft, DeleteWordRight, SelectAll, Home, End, SelectHome, SelectEnd, Paste, Cut, Copy,
    ]
);

pub const CONTEXT: &str = "KadrellInput";

/// Belegung mit zur Laufzeit gebautem Tastennamen (einmal beim Start, das kleine Leck ist gewollt).
fn bind(keys: String, action: impl gpui::Action, context: Option<&'static str>) -> KeyBinding {
    KeyBinding::new(Box::leak(keys.into_boxed_str()), action, context)
}

/// Tastenbelegung der Felder; einmal beim Start aufrufen.
pub fn bind_keys(cx: &mut App) {
    let mac = cfg!(target_os = "macos");
    let word = if mac { "alt" } else { "ctrl" };
    let cmd = if mac { "cmd" } else { "ctrl" };
    let c = Some(CONTEXT);
    let mut keys = vec![
        KeyBinding::new("backspace", Backspace, c),
        KeyBinding::new("delete", Delete, c),
        KeyBinding::new("left", Left, c),
        KeyBinding::new("right", Right, c),
        KeyBinding::new("shift-left", SelectLeft, c),
        KeyBinding::new("shift-right", SelectRight, c),
        KeyBinding::new("home", Home, c),
        KeyBinding::new("end", End, c),
        KeyBinding::new("shift-home", SelectHome, c),
        KeyBinding::new("shift-end", SelectEnd, c),
    ];
    keys.push(bind(format!("{word}-left"), WordLeft, c));
    keys.push(bind(format!("{word}-right"), WordRight, c));
    keys.push(bind(format!("{word}-shift-left"), SelectWordLeft, c));
    keys.push(bind(format!("{word}-shift-right"), SelectWordRight, c));
    keys.push(bind(format!("{word}-backspace"), DeleteWordLeft, c));
    keys.push(bind(format!("{word}-delete"), DeleteWordRight, c));
    keys.push(bind(format!("{cmd}-a"), SelectAll, c));
    keys.push(bind(format!("{cmd}-v"), Paste, c));
    keys.push(bind(format!("{cmd}-c"), Copy, c));
    keys.push(bind(format!("{cmd}-x"), Cut, c));
    if mac {
        keys.push(KeyBinding::new("cmd-left", Home, c));
        keys.push(KeyBinding::new("cmd-right", End, c));
    }
    cx.bind_keys(keys);
}

/// Das Feld meldet jede Änderung des Textes.
pub enum TextInputEvent {
    Changed,
}

impl EventEmitter<TextInputEvent> for TextInput {}

pub struct TextInput {
    pub focus: FocusHandle,
    content: SharedString,
    placeholder: SharedString,
    selected: Range<usize>,
    reversed: bool,
    marked: Option<Range<usize>>,
    last_layout: Option<ShapedLine>,
    last_bounds: Option<Bounds<Pixels>>,
    scroll_x: Pixels,
    selecting: bool,
    /// Schmale Variante für Tabellenzeilen (Einstellungen): 22 px hoch, 12 pt, Fuge statt dicker Rand.
    compact: bool,
    /// Suchfeld der Palette: 15 pt, nie ein Akzentring (die Mulde bleibt beim Tippen unverändert).
    large: bool,
}

// Zeichengrenzen und Wortsprünge als reine Funktionen, damit sie testbar bleiben.

fn prev_boundary(text: &str, offset: usize) -> usize {
    text[..offset].char_indices().next_back().map_or(0, |(i, _)| i)
}

fn next_boundary(text: &str, offset: usize) -> usize {
    text[offset..].chars().next().map_or(text.len(), |c| offset + c.len_utf8())
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Anfang des Worts links von `offset` (erst Trenner überspringen, dann das Wort).
pub fn prev_word(text: &str, offset: usize) -> usize {
    let head = &text[..offset];
    let trimmed = head.trim_end_matches(|c| !is_word(c));
    let word = trimmed.trim_end_matches(is_word);
    word.len()
}

/// Ende des Worts rechts von `offset`.
pub fn next_word(text: &str, offset: usize) -> usize {
    let tail = &text[offset..];
    let after_sep = tail.trim_start_matches(|c| !is_word(c));
    let after_word = after_sep.trim_start_matches(is_word);
    text.len() - after_word.len()
}

impl TextInput {
    pub fn new(placeholder: &str, cx: &mut Context<Self>) -> Self {
        Self {
            focus: cx.focus_handle(),
            content: "".into(),
            placeholder: placeholder.to_string().into(),
            selected: 0..0,
            reversed: false,
            marked: None,
            last_layout: None,
            last_bounds: None,
            scroll_x: px(0.),
            selecting: false,
            compact: false,
            large: false,
        }
    }

    /// Suchfeld der Palette (`PaletteWindow`): 15 pt, Fokus ohne Akzentring.
    pub fn large(mut self) -> Self {
        self.large = true;
        self
    }

    /// Schmale Variante für Tabellenzeilen.
    pub fn compact(mut self) -> Self {
        self.compact = true;
        self
    }

    pub fn text(&self) -> &str {
        &self.content
    }

    /// Ersetzt den Text, Cursor ans Ende.
    pub fn set_text(&mut self, text: &str, window: &Window, cx: &mut Context<Self>) {
        self.content = text.to_string().into();
        self.selected = text.len()..text.len();
        self.reversed = false;
        self.marked = None;
        self.changed(window, cx);
    }

    fn changed(&mut self, window: &Window, cx: &mut Context<Self>) {
        window.notify_text_input_changed(&self.focus);
        cx.emit(TextInputEvent::Changed);
        cx.notify();
    }

    fn cursor(&self) -> usize {
        if self.reversed { self.selected.start } else { self.selected.end }
    }

    fn move_to(&mut self, offset: usize, window: &Window, cx: &mut Context<Self>) {
        self.selected = offset..offset;
        self.reversed = false;
        window.notify_text_selection_changed(&self.focus);
        cx.notify();
    }

    fn select_to(&mut self, offset: usize, window: &Window, cx: &mut Context<Self>) {
        if self.reversed {
            self.selected.start = offset;
        } else {
            self.selected.end = offset;
        }
        if self.selected.end < self.selected.start {
            self.reversed = !self.reversed;
            self.selected = self.selected.end..self.selected.start;
        }
        window.notify_text_selection_changed(&self.focus);
        cx.notify();
    }

    fn left(&mut self, _: &Left, w: &mut Window, cx: &mut Context<Self>) {
        let to = if self.selected.is_empty() { prev_boundary(&self.content, self.cursor()) } else { self.selected.start };
        self.move_to(to, w, cx);
    }

    fn right(&mut self, _: &Right, w: &mut Window, cx: &mut Context<Self>) {
        let to = if self.selected.is_empty() { next_boundary(&self.content, self.selected.end) } else { self.selected.end };
        self.move_to(to, w, cx);
    }

    fn select_left(&mut self, _: &SelectLeft, w: &mut Window, cx: &mut Context<Self>) {
        self.select_to(prev_boundary(&self.content, self.cursor()), w, cx);
    }

    fn select_right(&mut self, _: &SelectRight, w: &mut Window, cx: &mut Context<Self>) {
        self.select_to(next_boundary(&self.content, self.cursor()), w, cx);
    }

    fn word_left(&mut self, _: &WordLeft, w: &mut Window, cx: &mut Context<Self>) {
        self.move_to(prev_word(&self.content, self.cursor()), w, cx);
    }

    fn word_right(&mut self, _: &WordRight, w: &mut Window, cx: &mut Context<Self>) {
        self.move_to(next_word(&self.content, self.cursor()), w, cx);
    }

    fn select_word_left(&mut self, _: &SelectWordLeft, w: &mut Window, cx: &mut Context<Self>) {
        self.select_to(prev_word(&self.content, self.cursor()), w, cx);
    }

    fn select_word_right(&mut self, _: &SelectWordRight, w: &mut Window, cx: &mut Context<Self>) {
        self.select_to(next_word(&self.content, self.cursor()), w, cx);
    }

    fn select_all(&mut self, _: &SelectAll, w: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, w, cx);
        self.select_to(self.content.len(), w, cx);
    }

    fn home(&mut self, _: &Home, w: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, w, cx);
    }

    fn end(&mut self, _: &End, w: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.content.len(), w, cx);
    }

    fn select_home(&mut self, _: &SelectHome, w: &mut Window, cx: &mut Context<Self>) {
        self.select_to(0, w, cx);
    }

    fn select_end(&mut self, _: &SelectEnd, w: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.content.len(), w, cx);
    }

    /// Löscht die Auswahl, sonst bis `target` (Zeichen oder Wort).
    fn delete_to(&mut self, target: usize, w: &mut Window, cx: &mut Context<Self>) {
        if self.selected.is_empty() {
            if target == self.cursor() {
                return;
            }
            self.select_to(target, w, cx);
        }
        self.replace_text_in_range(None, "", w, cx);
    }

    fn backspace(&mut self, _: &Backspace, w: &mut Window, cx: &mut Context<Self>) {
        self.delete_to(prev_boundary(&self.content, self.cursor()), w, cx);
    }

    fn delete(&mut self, _: &Delete, w: &mut Window, cx: &mut Context<Self>) {
        self.delete_to(next_boundary(&self.content, self.cursor()), w, cx);
    }

    fn delete_word_left(&mut self, _: &DeleteWordLeft, w: &mut Window, cx: &mut Context<Self>) {
        self.delete_to(prev_word(&self.content, self.cursor()), w, cx);
    }

    fn delete_word_right(&mut self, _: &DeleteWordRight, w: &mut Window, cx: &mut Context<Self>) {
        self.delete_to(next_word(&self.content, self.cursor()), w, cx);
    }

    fn paste(&mut self, _: &Paste, w: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.replace_text_in_range(None, &text.replace(['\r', '\n'], " "), w, cx);
        }
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selected.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(self.content[self.selected.clone()].to_string()));
        }
    }

    fn cut(&mut self, _: &Cut, w: &mut Window, cx: &mut Context<Self>) {
        if !self.selected.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(self.content[self.selected.clone()].to_string()));
            self.replace_text_in_range(None, "", w, cx);
        }
    }

    fn on_mouse_down(&mut self, ev: &MouseDownEvent, w: &mut Window, cx: &mut Context<Self>) {
        self.selecting = true;
        let at = self.index_for_mouse(ev.position);
        if ev.modifiers.shift {
            self.select_to(at, w, cx);
        } else {
            self.move_to(at, w, cx);
        }
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.selecting = false;
    }

    fn on_mouse_move(&mut self, ev: &MouseMoveEvent, w: &mut Window, cx: &mut Context<Self>) {
        if self.selecting {
            self.select_to(self.index_for_mouse(ev.position), w, cx);
        }
    }

    fn index_for_mouse(&self, position: Point<Pixels>) -> usize {
        let (Some(bounds), Some(line)) = (self.last_bounds.as_ref(), self.last_layout.as_ref()) else { return 0 };
        if self.content.is_empty() {
            return 0;
        }
        line.closest_index_for_x(position.x - bounds.left() + self.scroll_x)
    }

    fn offset_from_utf16(&self, offset: usize) -> usize {
        let (mut utf8, mut utf16) = (0, 0);
        for ch in self.content.chars() {
            if utf16 >= offset {
                break;
            }
            utf16 += ch.len_utf16();
            utf8 += ch.len_utf8();
        }
        utf8
    }

    fn offset_to_utf16(&self, offset: usize) -> usize {
        let (mut utf16, mut utf8) = (0, 0);
        for ch in self.content.chars() {
            if utf8 >= offset {
                break;
            }
            utf8 += ch.len_utf8();
            utf16 += ch.len_utf16();
        }
        utf16
    }

    fn range_to_utf16(&self, r: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(r.start)..self.offset_to_utf16(r.end)
    }

    fn range_from_utf16(&self, r: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(r.start)..self.offset_from_utf16(r.end)
    }

    /// Zu ersetzender Bereich: ausdrücklich genannt, sonst markierter Text, sonst Auswahl.
    fn target_range(&self, range_utf16: Option<Range<usize>>) -> Range<usize> {
        range_utf16.as_ref().map(|r| self.range_from_utf16(r)).or(self.marked.clone()).unwrap_or(self.selected.clone())
    }
}

impl EntityInputHandler for TextInput {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        actual.replace(self.range_to_utf16(&range));
        Some(self.content[range].to_string())
    }

    fn selected_text_range(&mut self, _: bool, _: &mut Window, _: &mut Context<Self>) -> Option<UTF16Selection> {
        Some(UTF16Selection { range: self.range_to_utf16(&self.selected), reversed: self.reversed })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked.as_ref().map(|r| self.range_to_utf16(r))
    }

    fn unmark_text(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        self.marked = None;
        w.notify_text_input_changed(&self.focus);
        cx.notify();
    }

    fn replace_text_in_range(&mut self, range_utf16: Option<Range<usize>>, text: &str, w: &mut Window, cx: &mut Context<Self>) {
        let range = self.target_range(range_utf16);
        self.content = format!("{}{}{}", &self.content[..range.start], text, &self.content[range.end..]).into();
        let end = range.start + text.len();
        self.selected = end..end;
        self.reversed = false;
        self.marked = None;
        self.changed(w, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        text: &str,
        new_selected_utf16: Option<Range<usize>>,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = self.target_range(range_utf16);
        self.content = format!("{}{}{}", &self.content[..range.start], text, &self.content[range.end..]).into();
        self.marked = (!text.is_empty()).then(|| range.start..range.start + text.len());
        let start16 = self.offset_to_utf16(range.start);
        let len16 = text.encode_utf16().count();
        self.selected = new_selected_utf16
            .map(|s| self.range_from_utf16(&(start16 + s.start.min(len16)..start16 + s.end.min(len16))))
            .unwrap_or(range.start + text.len()..range.start + text.len());
        self.reversed = false;
        self.changed(w, cx);
    }

    fn set_selected_text_range(&mut self, range_utf16: Range<usize>, w: &mut Window, cx: &mut Context<Self>) {
        self.selected = self.range_from_utf16(&range_utf16);
        self.reversed = false;
        w.notify_text_selection_changed(&self.focus);
        cx.notify();
    }

    fn text_length_utf16(&mut self, _: &mut Window, _: &mut Context<Self>) -> Option<usize> {
        Some(self.content.encode_utf16().count())
    }

    fn bounds_for_range(&mut self, range_utf16: Range<usize>, bounds: Bounds<Pixels>, _: &mut Window, _: &mut Context<Self>) -> Option<Bounds<Pixels>> {
        let layout = self.last_layout.as_ref().filter(|l| l.text == self.content)?;
        let r = self.range_from_utf16(&range_utf16);
        let left = bounds.left() - self.scroll_x;
        Some(Bounds::from_corners(
            point(left + layout.x_for_index(r.start), bounds.top()),
            point(left + layout.x_for_index(r.end), bounds.bottom()),
        ))
    }

    fn character_index_for_point(&mut self, p: Point<Pixels>, _: &mut Window, _: &mut Context<Self>) -> Option<usize> {
        let local = self.last_bounds?.localize(&p)?;
        let layout = self.last_layout.as_ref().filter(|l| l.text == self.content)?;
        Some(self.offset_to_utf16(layout.closest_index_for_x(local.x + self.scroll_x)))
    }
}

struct TextElement {
    input: Entity<TextInput>,
    colors: Theme,
}

struct Prepaint {
    line: ShapedLine,
    cursor: Option<PaintQuad>,
    selection: Option<PaintQuad>,
    scroll_x: Pixels,
}

fn runs_for(len: usize, marked: Option<&Range<usize>>, base: TextRun) -> Vec<TextRun> {
    let Some(m) = marked.filter(|m| m.end <= len) else { return vec![TextRun { len, ..base }] };
    let underline = UnderlineStyle { color: Some(base.color), thickness: px(1.), wavy: false };
    [
        TextRun { len: m.start, ..base.clone() },
        TextRun { len: m.end - m.start, underline: Some(underline), ..base.clone() },
        TextRun { len: len - m.end, ..base },
    ]
    .into_iter()
    .filter(|r| r.len > 0)
    .collect()
}

fn with_alpha(c: Hsla, a: f32) -> Hsla {
    Hsla { a, ..c }
}

impl Element for TextElement {
    type RequestLayoutState = ();
    type PrepaintState = Prepaint;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(&mut self, _: Option<&GlobalElementId>, _: Option<&gpui::InspectorElementId>, window: &mut Window, cx: &mut App) -> (LayoutId, ()) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = window.line_height().into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(&mut self, _: Option<&GlobalElementId>, _: Option<&gpui::InspectorElementId>, bounds: Bounds<Pixels>, _: &mut (), window: &mut Window, cx: &mut App) -> Prepaint {
        let input = self.input.read(cx);
        let style = window.text_style();
        let empty = input.content.is_empty();
        let (text, color) = if empty { (input.placeholder.clone(), self.colors.muted) } else { (input.content.clone(), style.color) };
        let base = TextRun { len: text.len(), font: style.font(), color, background_color: None, underline: None, strikethrough: None };
        let runs = runs_for(text.len(), if empty { None } else { input.marked.as_ref() }, base);
        let font_size = style.font_size.to_pixels(window.rem_size());
        let line = window.text_system().shape_line(text, font_size, &runs, None);
        let cursor_x = line.x_for_index(if empty { 0 } else { input.cursor() });
        let margin = px(4.);
        let scroll_x = (cursor_x + margin - bounds.size.width).max(px(0.));
        let at = |x: Pixels| bounds.left() - scroll_x + x;
        let (selection, cursor) = if input.selected.is_empty() {
            let rect = Bounds::new(point(at(cursor_x), bounds.top()), size(px(1.5), bounds.size.height));
            (None, Some(fill(rect, self.colors.accent)))
        } else {
            let (a, b) = (line.x_for_index(input.selected.start), line.x_for_index(input.selected.end));
            let rect = Bounds::from_corners(point(at(a), bounds.top()), point(at(b), bounds.bottom()));
            (Some(fill(rect, with_alpha(self.colors.accent, 0.3))), None)
        };
        Prepaint { line, cursor, selection, scroll_x }
    }

    fn paint(&mut self, _: Option<&GlobalElementId>, _: Option<&gpui::InspectorElementId>, bounds: Bounds<Pixels>, _: &mut (), pre: &mut Prepaint, window: &mut Window, cx: &mut App) {
        let focus = self.input.read(cx).focus.clone();
        window.handle_input(&focus, ElementInputHandler::new(bounds, self.input.clone()), cx);
        window.with_content_mask(Some(gpui::ContentMask { bounds }), |window| {
            if let Some(sel) = pre.selection.take() {
                window.paint_quad(sel);
            }
            let origin = point(bounds.origin.x - pre.scroll_x, bounds.origin.y);
            pre.line.paint(origin, window.line_height(), gpui::TextAlign::Left, None, window, cx).ok();
            if focus.is_focused(window)
                && let Some(cursor) = pre.cursor.take()
            {
                window.paint_quad(cursor);
            }
        });
        let (line, scroll_x) = (pre.line.clone(), pre.scroll_x);
        self.input.update(cx, |input, _| {
            input.last_layout = Some(line);
            input.last_bounds = Some(bounds);
            input.scroll_x = scroll_x;
        });
    }
}

impl Render for TextInput {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme(cx);
        let focused = self.focus.is_focused(window);
        let ring = if focused && !self.large { t.accent } else { seam_dark(&t) };
        // Schmal: 1 px Fuge, mit Fokus 2 px Akzentring; der Innenabstand gleicht den Unterschied aus.
        let thick = focused || !self.compact;
        let text_size = if self.large { 15. } else { 13. };
        let (height, text, pad) = if self.compact { (22., 12., if focused { 6. } else { 7. }) } else { (32., text_size, 6.) };
        div()
            .flex()
            .items_center()
            .w_full()
            .h(px(height))
            .px(px(pad))
            .rounded(px(5.))
            .when(thick, |d| d.border_2())
            .when(!thick, |d| d.border_1())
            .border_color(ring)
            .bg(super::overlay::well(&t))
            .text_color(t.fg)
            .text_size(px(text))
            .key_context(CONTEXT)
            .track_focus(&self.focus)
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::word_left))
            .on_action(cx.listener(Self::word_right))
            .on_action(cx.listener(Self::select_word_left))
            .on_action(cx.listener(Self::select_word_right))
            .on_action(cx.listener(Self::delete_word_left))
            .on_action(cx.listener(Self::delete_word_right))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_action(cx.listener(Self::select_home))
            .on_action(cx.listener(Self::select_end))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::copy))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .child(div().flex_1().min_w_0().overflow_hidden().child(TextElement { input: cx.entity(), colors: t }))
    }
}

impl Focusable for TextInput {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn char_boundaries_respect_multibyte() {
        let s = "aäb";
        assert_eq!(next_boundary(s, 1), 3);
        assert_eq!(prev_boundary(s, 3), 1);
        assert_eq!(prev_boundary(s, 0), 0);
        assert_eq!(next_boundary(s, 4), 4);
    }

    #[test]
    fn word_jumps_skip_separators() {
        let s = "~/dev/projects/kad";
        assert_eq!(prev_word(s, s.len()), 15);
        assert_eq!(prev_word(s, 15), 6);
        assert_eq!(next_word(s, 0), 5);
        assert_eq!(next_word(s, 5), 14);
        assert_eq!(next_word(s, s.len()), s.len());
        assert_eq!(prev_word(s, 0), 0);
    }

    #[test]
    fn word_jumps_with_umlauts() {
        let s = "grüne wiese";
        assert_eq!(prev_word(s, s.len()), 7);
        assert_eq!(next_word(s, 0), 6);
    }
}
