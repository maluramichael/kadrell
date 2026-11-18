//! Kacheln der Arbeitsfläche: reine Layout-Mathematik (Port von `app/Kadrell/Workspace/Tiling.swift`),
//! Auswahlmodell (wie `WorkspaceView`) und die Ansicht, die die Kacheln absolut platziert.

use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;

use gpui::{
    AnyElement, Context, InteractiveElement, IntoElement, MouseButton, MouseDownEvent, MouseMoveEvent, ParentElement,
    Stateful, Styled, canvas, div, px,
};

use super::model::Group;
use super::tile::{self, TileStyle};
use super::theme;
use crate::Shell;

/// Abstand zwischen den Kacheln und zum Rand (`Settings.tileGap` der Mac-App).
pub const GAP: f32 = 6.;
/// Titelzeile einer Stack-Zeile.
pub const ROW_HEIGHT: f32 = 26.;
/// Kleinster Anteil eines Feldes beim Ziehen.
const MIN_RATIO: f64 = 0.05;
const MAX_COLS: usize = 9;

/// Gruppe und Session einer Kachel.
pub type Id = (usize, usize);

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }
    pub fn max_x(&self) -> f32 {
        self.x + self.w
    }
    pub fn max_y(&self) -> f32 {
        self.y + self.h
    }
    fn inset(&self, d: f32) -> Rect {
        Rect::new(self.x + d, self.y + d, (self.w - 2. * d).max(0.), (self.h - 2. * d).max(0.))
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum LayoutMode {
    Grid,
    Main,
    Spiral,
    Stack,
}

impl LayoutMode {
    pub const ALL: [LayoutMode; 4] = [LayoutMode::Grid, LayoutMode::Main, LayoutMode::Spiral, LayoutMode::Stack];

    /// Reihum wie tmux `next-layout`.
    pub fn next(self) -> LayoutMode {
        let i = Self::ALL.iter().position(|m| *m == self).unwrap_or(0);
        Self::ALL[(i + 1) % Self::ALL.len()]
    }

    pub fn from_name(name: &str) -> Option<LayoutMode> {
        match name {
            "grid" => Some(LayoutMode::Grid),
            "main" => Some(LayoutMode::Main),
            "spiral" => Some(LayoutMode::Spiral),
            "stack" => Some(LayoutMode::Stack),
            _ => None,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            LayoutMode::Grid => "grid",
            LayoutMode::Main => "main",
            LayoutMode::Spiral => "spiral",
            LayoutMode::Stack => "stack",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}

/// Ziehbare Grenze einer Vorlage: gehört zur Verhältnisliste `key` mit `count` Feldern, liegt hinter Feld `index`.
#[derive(Clone, PartialEq, Debug)]
pub struct SplitLine {
    pub key: String,
    pub index: usize,
    pub count: usize,
    /// Senkrechte Linie, wird waagerecht gezogen.
    pub vertical: bool,
    /// Griffzone, mindestens 8 px breit, auch bei Abstand 0.
    pub rect: Rect,
    /// Fläche, die diese Liste aufteilt.
    pub span: Rect,
}

pub fn columns(n: usize) -> usize {
    if n <= 1 { 1 } else { (n as f64).sqrt().ceil() as usize }
}

/// Verhältnisse einer Vorlage nach Schlüssel und Feldzahl, `None` = gleich verteilt.
pub type Ratios<'a> = &'a dyn Fn(&str, usize) -> Option<Vec<f64>>;

/// Abschnitte auf einer Achse nach Verhältnissen, dazwischen `gap`; Kanten auf ganze Pixel,
/// das letzte Ende sitzt genau am Rand.
pub fn segments(start: f32, length: f32, gap: f32, ratios: &[f64]) -> Vec<(f32, f32)> {
    let usable = (length - gap * (ratios.len() as f32 - 1.)).max(0.);
    let mut acc = 0.0f64;
    let mut out = Vec::new();
    for (i, v) in ratios.iter().enumerate() {
        let s = (start + acc as f32 * usable + i as f32 * gap).round();
        acc += v;
        let e = if i == ratios.len() - 1 { start + length } else { (start + acc as f32 * usable + i as f32 * gap).round() };
        out.push((s, e));
    }
    out
}

/// Teilt Flächen nach den Verhältnissen der Vorlage und sammelt dabei die ziehbaren Grenzen.
struct Splitter<'a> {
    gap: f32,
    ratios: Ratios<'a>,
    dividers: Vec<SplitLine>,
}

impl<'a> Splitter<'a> {
    /// Gespeicherte Verhältnisse normiert, ungültige oder fehlende gleich verteilt.
    fn normalized(&self, key: &str, count: usize) -> Vec<f64> {
        match (self.ratios)(key, count) {
            Some(v) if v.len() == count && v.iter().all(|x| *x > 0.) => {
                let sum: f64 = v.iter().sum();
                v.iter().map(|x| x / sum).collect()
            }
            _ => vec![1. / count as f64; count],
        }
    }

    fn grip(&self, area: Rect, vertical: bool, a: f32, z: f32) -> Rect {
        let mid = (a + z) / 2.;
        let w = (z - a).max(8.);
        if vertical { Rect::new(mid - w / 2., area.y, w, area.h) } else { Rect::new(area.x, mid - w / 2., area.w, w) }
    }

    fn part_rect(area: Rect, vertical: bool, s: f32, e: f32) -> Rect {
        if vertical { Rect::new(s, area.y, e - s, area.h) } else { Rect::new(area.x, s, area.w, e - s) }
    }

    /// Teilt `area` entlang einer Achse nach `key` und merkt sich die Grenzen.
    fn split(&mut self, area: Rect, vertical: bool, key: &str, count: usize) -> Vec<Rect> {
        let (start, length) = if vertical { (area.x, area.w) } else { (area.y, area.h) };
        let parts = segments(start, length, self.gap, &self.normalized(key, count));
        for i in 0..count - 1 {
            let rect = self.grip(area, vertical, parts[i].1, parts[i + 1].0);
            self.dividers.push(SplitLine { key: key.to_string(), index: i, count, vertical, rect, span: area });
        }
        parts.iter().map(|(s, e)| Self::part_rect(area, vertical, *s, *e)).collect()
    }

    fn grid(&mut self, n: usize, b: Rect, fixed: usize) -> Vec<Rect> {
        let cols = if fixed > 0 { fixed.min(n) } else { columns(n) };
        let rows = n.div_ceil(cols);
        let xs = if cols > 1 { self.split(b, true, &format!("grid.cols.{cols}"), cols) } else { vec![b] };
        let ys = if rows > 1 { self.split(b, false, &format!("grid.rows.{rows}"), rows) } else { vec![b] };
        (0..n).map(|i| Rect::new(xs[i % cols].x, ys[i / cols].y, xs[i % cols].w, ys[i / cols].h)).collect()
    }

    fn main(&mut self, n: usize, b: Rect) -> Vec<Rect> {
        if n <= 1 {
            return vec![b];
        }
        let cols = self.split(b, true, "main.cols", 2);
        let rest = if n > 2 { self.split(cols[1], false, &format!("main.rows.{}", n - 1), n - 1) } else { vec![cols[1]] };
        let mut out = vec![cols[0]];
        out.extend(rest);
        out
    }

    /// Spirale (bspwm): jede Kachel teilt den Rest, abwechselnd senkrecht und waagerecht.
    fn spiral(&mut self, n: usize, b: Rect) -> Vec<Rect> {
        let mut area = b;
        let mut frames = Vec::new();
        for i in 0..n.saturating_sub(1) {
            let parts = self.split(area, i % 2 == 0, &format!("spiral.{i}"), 2);
            frames.push(parts[0]);
            area = parts[1];
        }
        frames.push(area);
        frames
    }
}

/// Die Vorlage des Layouts für `n` Kacheln: Felder in Reihenfolge und ihre ziehbaren Grenzen. Stack hat
/// keine Vorlage (siehe `stack`). `fixed_cols` 0 = Grid wählt ⌈√n⌉ Spalten.
pub fn layout(mode: LayoutMode, n: usize, b: Rect, gap: f32, fixed_cols: usize, ratios: Ratios) -> (Vec<Rect>, Vec<SplitLine>) {
    if n == 0 {
        return (Vec::new(), Vec::new());
    }
    let mut s = Splitter { gap, ratios, dividers: Vec::new() };
    let frames = match mode {
        LayoutMode::Grid | LayoutMode::Stack => s.grid(n, b, fixed_cols),
        LayoutMode::Main => s.main(n, b),
        LayoutMode::Spiral => s.spiral(n, b),
    };
    (frames, s.dividers)
}

/// Neue Verhältnisse, wenn Grenze `d` an Position `p` (x, y) gezogen wird. Kein Feld wird kleiner als 5 %.
pub fn drag(d: &SplitLine, p: (f32, f32), gap: f32, current: &[f64]) -> Vec<f64> {
    let mut v = if current.len() == d.count { current.to_vec() } else { vec![1. / d.count as f64; d.count] };
    let (start, length) = if d.vertical { (d.span.x, d.span.w) } else { (d.span.y, d.span.h) };
    let usable = length - gap * (d.count as f32 - 1.);
    if usable <= 0. {
        return v;
    }
    let before: f64 = v[..d.index].iter().sum();
    let pair = v[d.index] + v[d.index + 1];
    let at = if d.vertical { p.0 } else { p.1 };
    let pos = ((at - start - d.index as f32 * gap - gap / 2.) / usable) as f64;
    v[d.index] = (pos - before).max(MIN_RATIO).min(pair - MIN_RATIO);
    v[d.index + 1] = pair - v[d.index];
    v
}

/// Überlappung zweier Kacheln auf der Achse quer zur Richtung.
fn overlap(a: Rect, b: Rect, d: Direction) -> f32 {
    match d {
        Direction::Left | Direction::Right => a.max_y().min(b.max_y()) - a.y.max(b.y),
        Direction::Up | Direction::Down => a.max_x().min(b.max_x()) - a.x.max(b.x),
    }
}

/// Abstand von `a` zu `b` in Richtung `d`, `None` wenn `b` nicht in dieser Richtung liegt.
fn distance(a: Rect, b: Rect, d: Direction) -> Option<f32> {
    let (dist, ok) = match d {
        Direction::Left => (a.x - b.max_x(), b.max_x() <= a.x + 1.),
        Direction::Right => (b.x - a.max_x(), b.x >= a.max_x() - 1.),
        Direction::Up => (a.y - b.max_y(), b.max_y() <= a.y + 1.),
        Direction::Down => (b.y - a.max_y(), b.y >= a.max_y() - 1.),
    };
    ok.then_some(dist)
}

/// Nachbar nach Geometrie: nächstes Feld in Richtung `d`, bevorzugt eines, das sich auf der anderen Achse überlappt.
pub fn neighbor(i: usize, frames: &[Rect], d: Direction) -> Option<usize> {
    let a = *frames.get(i)?;
    let score = |b: Rect| distance(a, b, d).map(|dist| (overlap(a, b, d) <= 0., dist, -overlap(a, b, d)));
    let mut best: Option<(usize, (bool, f32, f32))> = None;
    for (j, b) in frames.iter().enumerate().filter(|(j, _)| *j != i) {
        let Some(s) = score(*b) else { continue };
        if best.is_none_or(|(_, o)| s.partial_cmp(&o) == Some(std::cmp::Ordering::Less)) {
            best = Some((j, s));
        }
    }
    best.map(|(j, _)| j)
}

/// Stack (i3-Akkordeon): jede Kachel eine Titelzeile, die aktive bekommt ihren Körper direkt darunter,
/// die Zeilen danach bleiben darunter.
pub fn stack(n: usize, active: usize, b: Rect, row_h: f32) -> (Vec<Rect>, Rect) {
    let mut rows = Vec::new();
    let mut body = Rect::new(0., 0., 0., 0.);
    let mut y = b.y.round();
    let body_h = (b.h - n as f32 * row_h).max(0.).round();
    for i in 0..n {
        rows.push(Rect::new(b.x, y, b.w, row_h));
        y += row_h;
        if i == active {
            body = Rect::new(b.x, y, b.w, body_h);
            y += body_h;
        }
    }
    (rows, body)
}

/// Spaltenzahl nach ‹ ›: 0 = automatisch, davor und danach schrittweise.
pub fn step_cols(current: usize, n: usize, delta: isize) -> usize {
    let base = if current == 0 { columns(n) } else { current };
    let next = base as isize + delta;
    if next < 1 { 0 } else { (next as usize).min(MAX_COLS) }
}

// ---------------------------------------------------------------------------------------------------------
// Auswahl

/// Mehrfachauswahl wie in der Mac-App: Klick zeigt nur diese Session, Strg/Cmd nimmt sie dazu oder weg,
/// Shift markiert den Bereich seit dem letzten Klick, Gruppenklick zeigt alle ihre Sessions.
#[derive(Clone, PartialEq, Debug)]
pub struct Selection {
    /// In Anzeigereihenfolge.
    pub items: Vec<Id>,
    pub focus: Id,
    pub anchor: Id,
}

fn sorted(mut ids: Vec<Id>, flat: &[Id]) -> Vec<Id> {
    ids.sort_by_key(|i| flat.iter().position(|f| f == i).unwrap_or(usize::MAX));
    ids.dedup();
    ids
}

impl Selection {
    pub fn single(id: Id) -> Self {
        Self { items: vec![id], focus: id, anchor: id }
    }

    pub fn contains(&self, id: Id) -> bool {
        self.items.contains(&id)
    }

    /// Strg/Cmd-Klick: dazunehmen oder wegnehmen, die letzte Session bleibt.
    pub fn toggle(&mut self, id: Id, flat: &[Id]) {
        self.anchor = id;
        if !self.contains(id) {
            self.items = sorted([self.items.clone(), vec![id]].concat(), flat);
            self.focus = id;
        } else if self.items.len() > 1 {
            self.items.retain(|i| *i != id);
            if self.focus == id {
                self.focus = *self.items.last().unwrap_or(&id);
            }
        }
    }

    /// Shift-Klick: Bereich vom Anker bis `id` in Anzeigereihenfolge.
    pub fn range(&mut self, id: Id, flat: &[Id]) {
        let pos = |x: Id| flat.iter().position(|f| *f == x);
        let (Some(a), Some(b)) = (pos(self.anchor), pos(id)) else { return *self = Selection::single(id) };
        self.items = flat[a.min(b)..=a.max(b)].to_vec();
        self.focus = id;
    }

    /// Klick auf eine Gruppe: alle ihre Sessions, der Fokus bleibt, wenn er dazugehört.
    pub fn group(&mut self, ids: Vec<Id>) {
        let Some(first) = ids.first().copied() else { return };
        if !ids.contains(&self.focus) {
            self.focus = first;
        }
        self.anchor = first;
        self.items = ids;
    }

    /// Fokus auf eine sichtbare Kachel, die Auswahl bleibt.
    pub fn focus_on(&mut self, id: Id) {
        if self.contains(id) {
            self.focus = id;
        }
    }
}

// ---------------------------------------------------------------------------------------------------------
// Zustand und Ansicht

/// Letzte gemessene Fläche (x, y, Breite, Höhe) der Arbeitsfläche, aus dem Zeichnen zurückgemeldet.
pub type Measured = Rc<Cell<Option<(f32, f32, f32, f32)>>>;

pub struct LayoutState {
    pub mode: LayoutMode,
    /// Grid: feste Spaltenzahl, 0 = automatisch.
    pub cols: usize,
    /// Verhältnisse je Vorlage, nicht je Session.
    pub ratios: HashMap<String, Vec<f64>>,
    pub dragging: Option<SplitLine>,
    pub measured: Measured,
    /// Zuletzt berechnete Rahmen (Fokusbewegung).
    pub frames: Vec<Rect>,
}

impl LayoutState {
    pub fn new(mode: LayoutMode) -> Self {
        Self { mode, cols: 0, ratios: HashMap::new(), dragging: None, measured: Rc::new(Cell::new(None)), frames: Vec::new() }
    }

    pub fn ratios_for(&self, key: &str, count: usize) -> Option<Vec<f64>> {
        self.ratios.get(key).filter(|v| v.len() == count).cloned()
    }

    fn area(&self) -> Rect {
        let (_, _, w, h) = self.measured.get().unwrap_or((0., 0., 1000., 600.));
        Rect::new(0., 0., w, h)
    }

    /// Rahmen und Grenzen für `n` Kacheln mit Fokus `active`; die Rahmen der Stack-Zeilen enthalten beim aktiven den Körper.
    pub fn plan(&self, n: usize, active: usize) -> (Vec<Rect>, Vec<SplitLine>) {
        let area = if n > 1 { self.area().inset(GAP) } else { self.area() };
        if self.mode == LayoutMode::Stack && n > 1 {
            let (mut rows, body) = stack(n, active, area, ROW_HEIGHT);
            rows[active].h += body.h;
            return (rows, Vec::new());
        }
        let ratios = |k: &str, c: usize| self.ratios_for(k, c);
        layout(self.mode, n, area, GAP, self.cols, &ratios)
    }

    pub fn drag_to(&mut self, p: (f32, f32)) {
        let Some(d) = self.dragging.clone() else { return };
        let current = self.ratios.get(&d.key).cloned().unwrap_or_default();
        self.ratios.insert(d.key.clone(), drag(&d, p, GAP, &current));
    }

    pub fn reset(&mut self, key: &str) {
        self.ratios.remove(key);
    }

    pub fn label(&self) -> String {
        match self.cols {
            0 => "AUTO COL".to_string(),
            c => format!("{c} COL"),
        }
    }

    pub fn mode_key(&self) -> &'static str {
        self.mode.key()
    }
}

fn grip(line: &SplitLine, i: usize, cx: &mut Context<Shell>) -> AnyElement {
    let r = line.rect;
    let base = div().id(("grip", i)).absolute().left(px(r.x)).top(px(r.y)).w(px(r.w)).h(px(r.h));
    let base = if line.vertical { base.cursor_col_resize() } else { base.cursor_row_resize() };
    let line = line.clone();
    base.on_mouse_down(MouseButton::Left, cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
        if ev.click_count >= 2 {
            this.layout.reset(&line.key);
            this.layout.dragging = None;
        } else {
            this.layout.dragging = Some(line.clone());
        }
        cx.stop_propagation();
        cx.notify();
    }))
    .into_any_element()
}

fn placed(r: Rect, g: usize, s: usize, tile: AnyElement, cx: &mut Context<Shell>) -> AnyElement {
    div()
        .id(("tile", g * 100 + s))
        .absolute()
        .left(px(r.x))
        .top(px(r.y))
        .w(px(r.w))
        .h(px(r.h))
        .capture_any_mouse_down(cx.listener(move |this, _, window, cx| this.focus_tile(g, s, window, cx)))
        .child(tile)
        .into_any_element()
}

/// Sitzt die Fläche anders als zuletzt gemessen, neu zeichnen, damit die Rahmen stimmen.
fn measure(measured: Measured, entity: gpui::EntityId) -> impl IntoElement {
    canvas(
        move |b, _, cx| {
            let now = (b.origin.x.as_f32(), b.origin.y.as_f32(), b.size.width.as_f32(), b.size.height.as_f32());
            let old = measured.get();
            if old.is_none_or(|o| (o.0 - now.0).abs() + (o.1 - now.1).abs() + (o.2 - now.2).abs() + (o.3 - now.3).abs() > 0.5) {
                measured.set(Some(now));
                cx.notify(entity);
            }
        },
        |_, _, _, _| (),
    )
    .absolute()
    .size_full()
}

fn tiles(shell: &Shell, frames: &[Rect], cx: &mut Context<Shell>) -> Vec<AnyElement> {
    let visible = shell.visible();
    let style = TileStyle::for_count(visible.len());
    let stacked = shell.layout.mode == LayoutMode::Stack && visible.len() > 1;
    let mut out = Vec::new();
    for (i, &(g, s)) in visible.iter().enumerate() {
        let focused = Some((g, s)) == shell.selected;
        let body = !stacked || focused;
        let group: &Group = &shell.groups[g];
        let el = tile::tile(group, &group.sessions[s], style, focused, body, cx);
        out.push(placed(frames[i], g, s, el, cx));
    }
    out
}

fn on_move(this: &mut Shell, ev: &MouseMoveEvent, cx: &mut Context<Shell>) {
    if this.layout.dragging.is_none() {
        return;
    }
    if ev.pressed_button != Some(MouseButton::Left) {
        this.layout.dragging = None;
        return;
    }
    let (ox, oy, _, _) = this.layout.measured.get().unwrap_or((0., 0., 0., 0.));
    this.layout.drag_to((ev.position.x.as_f32() - ox, ev.position.y.as_f32() - oy));
    cx.notify();
}

/// Arbeitsfläche: alle sichtbaren Kacheln samt ziehbaren Grenzen.
pub fn workspace(shell: &mut Shell, cx: &mut Context<Shell>) -> Stateful<gpui::Div> {
    let t = theme(cx);
    let visible_n = shell.visible().len();
    let active = shell.selected.and_then(|sel| shell.visible().iter().position(|id| *id == sel)).unwrap_or(0);
    let (frames, dividers) = shell.layout.plan(visible_n, active);
    shell.layout.frames = frames.clone();
    let empty = (visible_n == 0).then(|| crate::misc::empty::view(shell, &t, cx));
    let mut area = div()
        .id("workspace")
        .relative()
        .flex_1()
        .min_w_0()
        .min_h_0()
        .overflow_hidden()
        .bg(t.bg)
        .child(measure(shell.layout.measured.clone(), cx.entity_id()))
        .children(tiles(shell, &frames, cx))
        .children(empty);
    for (i, d) in dividers.iter().enumerate() {
        area = area.child(grip(d, i, cx));
    }
    area.on_mouse_move(cx.listener(|this, ev: &MouseMoveEvent, _, cx| on_move(this, ev, cx)))
        .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, _| this.layout.dragging = None))
        .on_mouse_up_out(MouseButton::Left, cx.listener(|this, _, _, _| this.layout.dragging = None))
}

#[cfg(test)]
mod tests {
    use super::*;

    const B: Rect = Rect { x: 0., y: 0., w: 1000., h: 600. };

    fn none(_: &str, _: usize) -> Option<Vec<f64>> {
        None
    }

    fn grid(n: usize, gap: f32) -> Vec<Rect> {
        layout(LayoutMode::Grid, n, B, gap, 0, &none).0
    }

    #[test]
    fn grid_columns_and_fill() {
        assert!(grid(0, 6.).is_empty());
        assert_eq!(grid(1, 6.), vec![B]);
        let five = grid(5, 6.);
        assert_eq!(five.len(), 5);
        assert_eq!(five[0].x, 0.);
        assert_eq!(five[2].max_x(), 1000.);
        assert_eq!(five[1].x, five[0].max_x() + 6.);
        assert_eq!(five[3].y, five[0].max_y() + 6.);
        assert_eq!(five[3].x, 0.);
        assert_eq!(five[4].max_y(), 600.);
        for r in &five {
            assert_eq!(r.x, r.x.round());
            assert_eq!(r.w, r.w.round());
        }
        let wide = grid(2, 40.);
        assert_eq!(wide[1].x, wide[0].max_x() + 40.);
        assert_eq!(wide[1].max_x(), 1000.);
    }

    #[test]
    fn stack_accordion_keeps_rows_below_active() {
        let (rows, body) = stack(4, 1, B, ROW_HEIGHT);
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[0].y, 0.);
        assert_eq!(rows[1].y, ROW_HEIGHT);
        assert_eq!(body.y, rows[1].max_y());
        assert_eq!(rows[2].y, body.max_y());
        assert_eq!(rows[3].max_y(), 600.);
        assert_eq!(body.h, 600. - 4. * ROW_HEIGHT);
    }

    #[test]
    fn templates_fill_in_order() {
        let r = |k: &str, _: usize| (k == "main.cols").then(|| vec![0.7, 0.3]);
        let (f, d) = layout(LayoutMode::Main, 3, B, 0., 0, &r);
        assert_eq!(f[0], Rect::new(0., 0., 700., 600.));
        assert_eq!(f[1], Rect::new(700., 0., 300., 300.));
        assert_eq!(f[2].max_y(), 600.);
        assert_eq!(d.iter().map(|d| d.key.as_str()).collect::<Vec<_>>(), ["main.cols", "main.rows.2"]);
        let sp = layout(LayoutMode::Spiral, 3, B, 0., 0, &none).0;
        assert_eq!(sp[0], Rect::new(0., 0., 500., 600.));
        assert_eq!(sp[1], Rect::new(500., 0., 500., 300.));
        assert_eq!(sp[2], Rect::new(500., 300., 500., 300.));
        let r3 = |k: &str, _: usize| (k == "grid.cols.3").then(|| vec![0.5, 0.25, 0.25]);
        let g = layout(LayoutMode::Grid, 3, B, 0., 3, &r3).0;
        assert_eq!(g.iter().map(|r| r.w).collect::<Vec<_>>(), [500., 250., 250.]);
        let bad = |_: &str, _: usize| Some(vec![1.]);
        let g = layout(LayoutMode::Grid, 2, B, 0., 0, &bad).0;
        assert_eq!(g.iter().map(|r| r.w).collect::<Vec<_>>(), [500., 500.]);
    }

    #[test]
    fn drag_moves_boundary_and_clamps() {
        let d = layout(LayoutMode::Main, 2, B, 10., 0, &none).1[0].clone();
        assert!(d.vertical);
        let moved = drag(&d, (305., 0.), 10., &[]);
        assert!((moved[0] - 300. / 990.).abs() < 0.001);
        assert!((moved.iter().sum::<f64>() - 1.).abs() < 0.0001);
        assert!((drag(&d, (-500., 0.), 10., &[])[0] - 0.05).abs() < 0.0001);
        assert!((drag(&d, (5000., 0.), 10., &[])[1] - 0.05).abs() < 0.0001);
    }

    #[test]
    fn drag_keeps_other_fields() {
        let d = layout(LayoutMode::Grid, 3, B, 0., 3, &none).1[1].clone();
        let v = drag(&d, (600., 0.), 0., &[0.2, 0.4, 0.4]);
        assert!((v[0] - 0.2).abs() < 1e-6);
        assert!((v[1] - 0.4).abs() < 1e-6);
        assert!((v[2] - 0.4).abs() < 1e-6);
        let v = drag(&d, (800., 0.), 0., &[0.2, 0.4, 0.4]);
        assert!((v[1] - 0.6).abs() < 1e-6 && (v[2] - 0.2).abs() < 1e-6);
    }

    #[test]
    fn neighbor_by_frames() {
        let f = layout(LayoutMode::Main, 3, B, 0., 0, &none).0;
        assert_eq!(neighbor(0, &f, Direction::Right), Some(1));
        assert_eq!(neighbor(2, &f, Direction::Left), Some(0));
        assert_eq!(neighbor(1, &f, Direction::Down), Some(2));
        assert_eq!(neighbor(0, &f, Direction::Left), None);
        assert_eq!(neighbor(9, &f, Direction::Left), None);
    }

    #[test]
    fn mode_cycles_and_cols_step() {
        assert_eq!(LayoutMode::Stack.next(), LayoutMode::Grid);
        assert_eq!(LayoutMode::Grid.next(), LayoutMode::Main);
        assert_eq!(columns(5), 3);
        assert_eq!(step_cols(0, 4, 1), 3);
        assert_eq!(step_cols(0, 4, -1), 1);
        assert_eq!(step_cols(1, 4, -1), 0);
        assert_eq!(step_cols(9, 4, 1), 9);
    }

    #[test]
    fn selection_click_toggle_range_group() {
        let flat: Vec<Id> = vec![(0, 0), (0, 1), (1, 0), (1, 1)];
        let mut s = Selection::single((0, 1));
        s.toggle((1, 1), &flat);
        assert_eq!(s.items, vec![(0, 1), (1, 1)]);
        assert_eq!(s.focus, (1, 1));
        s.toggle((0, 0), &flat);
        assert_eq!(s.items, vec![(0, 0), (0, 1), (1, 1)]);
        s.toggle((0, 0), &flat);
        s.toggle((1, 1), &flat);
        assert_eq!(s.items, vec![(0, 1)]);
        s.toggle((0, 1), &flat);
        assert_eq!(s.items, vec![(0, 1)], "die letzte Session bleibt");
        let mut s = Selection::single((0, 1));
        s.range((1, 1), &flat);
        assert_eq!(s.items, vec![(0, 1), (1, 0), (1, 1)]);
        assert_eq!(s.anchor, (0, 1));
        s.range((0, 0), &flat);
        assert_eq!(s.items, vec![(0, 0), (0, 1)]);
        s.group(vec![(1, 0), (1, 1)]);
        assert_eq!(s.items, vec![(1, 0), (1, 1)]);
        assert_eq!(s.focus, (1, 0));
        s.focus_on((1, 1));
        s.group(vec![(1, 0), (1, 1)]);
        assert_eq!(s.focus, (1, 1));
    }

    #[test]
    fn plan_stack_and_flat() {
        let st = LayoutState::new(LayoutMode::Stack);
        st.measured.set(Some((0., 0., 1000., 600.)));
        let (f, d) = st.plan(3, 1);
        assert!(d.is_empty());
        assert_eq!(f[0].h, ROW_HEIGHT);
        assert!(f[1].h > ROW_HEIGHT);
        assert_eq!(st.plan(1, 0).0, vec![B]);
    }
}
