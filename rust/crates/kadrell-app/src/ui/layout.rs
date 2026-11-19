//! Auswahlmodell (wie `WorkspaceView`), Zustand der Layouts und die Ansicht, die die Kacheln absolut platziert.
//! Die Layout-Mathematik steht in `tiling` (Port von `Tiling.swift`).

use kadrell_core::Settings;
use kadrell_i18n::trd;
use std::cell::Cell;
use std::time::Duration;
use std::collections::HashMap;
use std::rc::Rc;

use gpui::{
    Animation, AnimationExt, AnyElement, Bounds, Context, InteractiveElement, IntoElement, MouseButton, MouseDownEvent, MouseMoveEvent, ParentElement,
    Pixels, ScrollEvent, Stateful, StatefulInteractiveElement, Styled, canvas, div, px,
};

use super::groups::drag::DragTile;
use super::model::Group;
use super::tile;
use super::tile_edge::{self, Ghosts};
use super::tile_look;
use super::{module_corner, stack_row};
use super::theme;
pub use super::tiling::*;
use crate::Shell;

/// Gruppe und Session einer Kachel.
pub type Id = (usize, usize);

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

    /// Umschalt- oder Cmd-Klick auf eine Gruppe: ihre Sessions kommen zur Auswahl dazu (`addMissing`), der Fokus bleibt,
    /// wo er ist, sonst springt er auf die erste neue.
    pub fn add_all(&mut self, ids: Vec<Id>, flat: &[Id]) {
        let missing: Vec<Id> = ids.into_iter().filter(|i| !self.contains(*i)).collect();
        let Some(first) = missing.first().copied() else { return };
        self.items = sorted([self.items.clone(), missing].concat(), flat);
        self.focus = first;
        self.anchor = first;
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

/// Schlüssel in den Einstellungen für Verhältnisse (`layout.<vorlage>`), Spaltenzahlen und Teilungen von Frei.
pub const RATIO_PREFIX: &str = "layout.";
pub const GRID_COLUMNS_KEY: &str = "layout.grid.columns";
pub const SCROLL_COLUMNS_KEY: &str = "layout.scroll.columns";
pub const SPLITS_KEY: &str = "workspace.custom.splits";

pub struct LayoutState {
    pub mode: LayoutMode,
    /// Grid: feste Spaltenzahl, 0 = automatisch.
    pub cols: usize,
    /// Scrollen: sichtbare Spalten ohne eigene Breite, mindestens 1.
    pub scroll_cols: usize,
    /// Frei: Teilungsrichtung je Position (`r`, `d`, `a`).
    pub splits: String,
    /// Verhältnisse je Vorlage, nicht je Session.
    pub ratios: HashMap<String, Vec<f64>>,
    pub dragging: Option<SplitLine>,
    pub measured: Measured,
    /// Zuletzt berechnete Rahmen aller Kacheln (Fokusbewegung), beim Scrollen samt der außerhalb.
    pub frames: Vec<Rect>,
    /// Ziehbare Grenzen der aktuellen Vorlage (Pfeiltasten verschieben sie).
    pub dividers: Vec<SplitLine>,
    /// Scrollen: seitlicher Versatz der Spalten.
    pub scroll_x: f32,
    /// Scrollen: nach Fokuswechsel oder neuer Breite rückt die Fokus-Kachel ins Bild.
    pub reveal: bool,
    /// Breite der Fläche beim letzten `plan`: eine neue Breite rückt die Fokus-Kachel ins Bild.
    planned_width: f32,
    /// Abstand zwischen den Kacheln und zum Rand (`tiles.gap`) und UI-Größe, bei jedem Zeichnen aus `appearance` gesetzt.
    pub gap: f32,
    pub ui_scale: f32,
    /// Die Kachel (Schlüssel), über der die Maus steht: Symbole im Kopf, Reserve rechts.
    pub hover: Option<String>,
    /// Kacheln des letzten Zeichnens und davon die, die erst danach dazukamen und noch einblenden (0,18 s).
    seen: std::collections::HashSet<String>,
    fading: std::collections::HashSet<String>,
    /// Das erste Zeichnen ist vorbei: ab jetzt blenden neue Kacheln ein, beim allerersten Laden erscheint alles sofort.
    loaded: bool,
    /// Kacheln, deren Session verschwunden ist, blenden mit dem letzten Rahmen aus (0,15 s).
    pub ghosts: Ghosts,
}

impl LayoutState {
    pub fn new(mode: LayoutMode) -> Self {
        Self {
            mode,
            cols: 0,
            scroll_cols: 2,
            splits: String::new(),
            ratios: HashMap::new(),
            dragging: None,
            measured: Rc::new(Cell::new(None)),
            frames: Vec::new(),
            dividers: Vec::new(),
            scroll_x: 0.,
            reveal: true,
            planned_width: 0.,
            gap: GAP,
            ui_scale: 1.,
            hover: None,
            seen: Default::default(),
            fading: Default::default(),
            loaded: false,
            ghosts: Ghosts::default(),
        }
    }

    /// Ragt eine Kachel seitlich unter die Modul-Ecke (Scrollen)? Die Kachel in der Ecke selbst rundet sich und zählt nicht.
    pub fn under_corner(&self, scale: f32) -> bool {
        let at_corner = |f: &Rect| tile_look::corner_radius((f.x, f.y), self.gap, scale) > 0.;
        let others: Vec<Rect> = self.frames.iter().filter(|f| !at_corner(f)).copied().collect();
        tile_edge::any_under_corner(&others, tile_look::module_radius(scale))
    }

    /// Merkt die sichtbaren Kacheln: wer nach dem ersten Laden neu dazukommt, blendet ein.
    fn note_visible(&mut self, keys: &[String]) {
        for key in keys.iter().filter(|k| self.loaded && !self.seen.contains(*k)) {
            self.fading.insert(key.clone());
        }
        self.seen = keys.iter().cloned().collect();
        self.fading.retain(|k| self.seen.contains(k));
        self.loaded = true;
    }

    /// Blendet die Kachel gerade ein?
    fn is_fading(&self, key: &str) -> bool {
        self.fading.contains(key)
    }

    /// Spaltenzahl steht auf „automatisch“ (Grid, 0): die Taste zeigt „AUTO SP“ gedämpft.
    pub fn cols_auto(&self) -> bool {
        self.mode == LayoutMode::Grid && self.cols == 0
    }

    /// Layout samt gespeicherten Spalten, Teilungen und Verhältnissen aus den Einstellungen des Profils.
    pub fn from_settings(mode: LayoutMode, settings: &Settings) -> Self {
        let mut state = Self::new(mode);
        state.cols = settings.i64_or(GRID_COLUMNS_KEY, 0).clamp(0, MAX_COLS as i64) as usize;
        state.scroll_cols = settings.i64_or(SCROLL_COLUMNS_KEY, 2).clamp(1, 12) as usize;
        state.splits = settings.string_or(SPLITS_KEY, "");
        for key in settings.keys().filter(|k| k.starts_with(RATIO_PREFIX) && *k != GRID_COLUMNS_KEY && *k != SCROLL_COLUMNS_KEY) {
            let numbers = settings.get(key).and_then(|v| v.as_array()).map(|a| a.iter().filter_map(|x| x.as_f64()).collect::<Vec<_>>());
            if let Some(v) = numbers.filter(|v| !v.is_empty()) {
                state.ratios.insert(key[RATIO_PREFIX.len()..].to_string(), v);
            }
        }
        state
    }

    /// Gespeicherte Verhältnisse; die Länge prüft die Vorlage (`Splitter::normalized`), Scrollen kennt beliebige Längen.
    pub fn ratios_for(&self, key: &str, _count: usize) -> Option<Vec<f64>> {
        self.ratios.get(key).cloned()
    }

    pub fn area(&self) -> Rect {
        let (_, _, w, h) = self.measured.get().unwrap_or((0., 0., 1000., 600.));
        Rect::new(0., 0., w, h)
    }

    /// Rahmen und Grenzen für `n` Kacheln mit Fokus `active`; die Rahmen der Stack-Zeilen enthalten beim aktiven den
    /// Körper. Merkt sich Rahmen und Grenzen und führt beim Scrollen den Versatz nach.
    pub fn plan(&mut self, n: usize, active: usize) -> (Vec<Rect>, Vec<SplitLine>) {
        let area = self.inset_area();
        if (area.w - self.planned_width).abs() > 0.5 {
            self.planned_width = area.w;
            self.reveal = true;
        }
        let (frames, dividers) = if self.mode == LayoutMode::Stack && n > 1 {
            let (mut rows, body) = stack(n, active, area, (ROW_HEIGHT * self.ui_scale).round());
            rows[active].h += body.h;
            (rows, Vec::new())
        } else {
            self.template(n, area, active)
        };
        self.frames = frames.clone();
        self.dividers = dividers.clone();
        (frames, dividers)
    }

    /// Die Fläche mit dem Außenrand: der Abstand gilt rundherum, auch bei einer einzigen Kachel, Zoom und Vorschau
    /// (`bounds.insetBy(gap, gap)`).
    fn inset_area(&self) -> Rect {
        self.area().inset(self.gap)
    }

    /// Stack (i3-Akkordeon): eine Zeile je Kachel und der Körper der aktiven direkt unter ihrer Zeile. Gilt schon für eine
    /// einzige Kachel; Zoom und Vorschau planen ohne Stack. Gibt Zeilen und Körper getrennt zurück.
    pub fn plan_stack(&mut self, n: usize, active: usize) -> (Vec<Rect>, Rect) {
        let area = self.inset_area();
        let (rows, body) = stack(n, active.min(n.saturating_sub(1)), area, tile_look::header_height(self.ui_scale));
        self.frames = rows.clone();
        self.dividers = Vec::new();
        (rows, body)
    }

    fn template(&mut self, n: usize, area: Rect, active: usize) -> (Vec<Rect>, Vec<SplitLine>) {
        let ratios = |k: &str, c: usize| self.ratios_for(k, c);
        let opts = Opts { cols: self.cols, scroll_cols: self.scroll_cols, splits: &self.splits };
        let (frames, dividers) = layout(self.mode, n, area, self.gap, opts, &ratios);
        if self.mode != LayoutMode::Scroll || n == 0 {
            return (frames, dividers);
        }
        let reveal = self.reveal.then(|| frames.get(active).copied()).flatten();
        self.reveal = false;
        self.scroll_x = scroll_offset(self.scroll_x, reveal, frames.last().map_or(0., Rect::max_x), area);
        (frames.iter().map(|f| f.offset_x(-self.scroll_x)).collect(), dividers)
    }

    pub fn drag_to(&mut self, p: (f32, f32)) {
        let Some(d) = self.dragging.clone() else { return };
        let current = self.ratios.get(&d.key).cloned().unwrap_or_default();
        self.ratios.insert(d.key.clone(), drag(&d, p, self.gap, &current));
    }

    pub fn reset(&mut self, key: &str) {
        self.ratios.remove(key);
    }

    /// Alle gezogenen Verhältnisse weg, Spaltenzahl und Teilungen bleiben. Gibt die entfernten Schlüssel zurück.
    pub fn reset_all(&mut self) -> Vec<String> {
        self.ratios.drain().map(|(k, _)| k).collect()
    }

    /// ⌃⌥-Pfeile wie tmux `resize-pane`: die Trennlinie an der Kachel `frame` wandert 5 % in Pfeilrichtung. Gibt den
    /// Schlüssel der geänderten Liste zurück, `None` ohne passende Linie.
    pub fn nudge(&mut self, frame: Rect, d: Direction) -> Option<String> {
        let line = divider_for(&self.dividers, frame, d, self.gap + 10.)?.clone();
        let current = self.ratios.get(&line.key).cloned().unwrap_or_default();
        self.ratios.insert(line.key.clone(), drag(&line, nudge_target(&line, d), self.gap, &current));
        Some(line.key)
    }

    /// Scrollen: ← / → schaltet die Breite der Spalte `i` eine Stufe (⅓ ½ ⅔) schmaler bzw. breiter. `false`: nichts zu tun.
    pub fn step_scroll_width(&mut self, i: usize, d: Direction) -> bool {
        if !d.horizontal() {
            return false;
        }
        let mut v = self.ratios.get(SCROLL_KEY).cloned().unwrap_or_default();
        while v.len() <= i {
            v.push(0.5);
        }
        let next = nearest_scroll_width(v[i]) as isize + if d.forward() { 1 } else { -1 };
        let Some(width) = usize::try_from(next).ok().and_then(|n| SCROLL_WIDTHS.get(n)) else { return false };
        v[i] = *width;
        self.ratios.insert(SCROLL_KEY.to_string(), v);
        self.reveal = true;
        true
    }

    /// Seitlich scrollen (Mausrad, Trackpad): positiv schiebt die Ansicht nach rechts.
    pub fn scroll_by(&mut self, dx: f32) {
        if self.mode == LayoutMode::Scroll {
            self.scroll_x += dx;
        }
    }

    /// Beschriftung der Spaltentaste: Grid „AUTO SP“ oder „N SP“, Scrollen „N SP“; sonst keine.
    pub fn cols_label(&self) -> Option<String> {
        let count = |c: usize| if c == 0 { trd!("AUTO SP") } else { trd!("%lld SP", c as i64) };
        match self.mode {
            LayoutMode::Grid => Some(count(self.cols)),
            LayoutMode::Scroll => Some(count(self.scroll_cols)),
            _ => None,
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
            this.persist_ratio(&line.key);
        } else {
            this.layout.dragging = Some(line.clone());
        }
        cx.stop_propagation();
        cx.notify();
    }))
    .into_any_element()
}

/// Was unter dem Zeiger einer Kachel bei einem Klick liegt: ein Symbol der Kopfzeile oder einer Stack-Zeile (der Klick
/// gehört dann dem Symbol und fokussiert die Kachel nicht), sonst die Kachel selbst.
#[derive(Clone, Copy, PartialEq)]
enum Spot {
    Tile,
    /// Körper einer Stack-Kachel: kein Kopf, keine Symbole.
    Body,
    Row,
}

/// Liegt der Klick auf einem Symbol? Punkte ab linker oberer Ecke der Kachel, Maße in logischen Punkten.
fn on_icon(spot: Spot, local: (f32, f32), width: f32, header: bool) -> bool {
    match spot {
        Spot::Tile => header && tile_look::hit_icon(local, width).is_some(),
        Spot::Body => false,
        Spot::Row => tile_look::row_zone(width - local.0) != tile_look::RowZone::Row,
    }
}

fn placed(r: Rect, g: usize, s: usize, key: &str, spot: Spot, tile: AnyElement, cx: &mut Context<Shell>) -> gpui::Stateful<gpui::Div> {
    let t = theme(cx);
    let a = crate::appearance::current(cx);
    let host = format!("tile-{}", g * 100 + s);
    let (target, hover_key) = (key.to_string(), key.to_string());
    let menu_key = key.to_string();
    let (fg, corner) = (t.fg, tile_look::corner_radius((r.x, r.y), a.gap, a.scale));
    // Rahmen in Vordergrundfarbe, 2 px, solange eine Kachel darübergezogen wird.
    let hot = div()
        .group(format!("{host}-hot"))
        .absolute()
        .size_full()
        .rounded_tl(px(corner))
        .border_2()
        .border_color(gpui::transparent_black())
        .group_drag_over::<DragTile>(host.clone(), move |s| s.border_color(fg));
    let (width, scale, header) = (r.w / a.scale, a.scale, true);
    div()
        .id(("tile", g * 100 + s))
        .group(host)
        .on_drop(cx.listener(move |this, d: &DragTile, window, cx| this.drop_tile(d, &target, window, cx)))
        .on_hover(cx.listener(move |this, hovering: &bool, _, cx| {
            let was = this.layout.hover.clone();
            if *hovering {
                this.layout.hover = Some(hover_key.clone());
            } else if this.layout.hover.as_deref() == Some(hover_key.as_str()) {
                this.layout.hover = None;
            }
            if was != this.layout.hover {
                cx.notify();
            }
        }))
        .absolute()
        .left(px(r.x))
        .top(px(r.y))
        .w(px(r.w))
        .h(px(r.h))
        .capture_any_mouse_down(cx.listener(move |this, ev: &MouseDownEvent, window, cx| {
            let (ox, oy, _, _) = this.layout.measured.get().unwrap_or((0., 0., 0., 0.));
            let local = ((ev.position.x.as_f32() - ox - r.x) / scale, (ev.position.y.as_f32() - oy - r.y) / scale);
            if !on_icon(spot, local, width, header) {
                this.focus_tile(g, s, window, cx);
            }
        }))
        .on_mouse_down(MouseButton::Right, cx.listener(move |this, ev: &MouseDownEvent, _, cx| this.tile_context_menu(&menu_key, ev.position, cx)))
        .child(tile)
        .child(hot)
}

/// Sitzt die Fläche anders als zuletzt gemessen, neu zeichnen, damit die Rahmen stimmen. Fängt außerdem das
/// seitliche Scrollen im Layout Scrollen ab, noch bevor das Terminal darunter es bekommt.
fn measure_scrolling(measured: Measured, entity: gpui::Entity<Shell>) -> impl IntoElement {
    let id = entity.entity_id();
    measure_with(measured, id, Some(entity))
}

/// Wie `measure_scrolling` ohne Scrollen: für weitere Fenster, die keine `Shell` sind.
pub(crate) fn measure(measured: Measured, entity: gpui::EntityId) -> impl IntoElement {
    measure_with(measured, entity, None)
}

fn measure_with(measured: Measured, entity: gpui::EntityId, scroller: Option<gpui::Entity<Shell>>) -> impl IntoElement {
    canvas(
        move |b, _, cx| {
            let now = (b.origin.x.as_f32(), b.origin.y.as_f32(), b.size.width.as_f32(), b.size.height.as_f32());
            let old = measured.get();
            if old.is_none_or(|o| (o.0 - now.0).abs() + (o.1 - now.1).abs() + (o.2 - now.2).abs() + (o.3 - now.3).abs() > 0.5) {
                measured.set(Some(now));
                cx.notify(entity);
            }
        },
        move |bounds: Bounds<Pixels>, _, window, _| {
            let Some(scroller) = scroller.clone() else { return };
            window.on_mouse_event(move |ev: &ScrollEvent, phase, _, cx| {
                if phase != gpui::DispatchPhase::Capture || !bounds.contains(&ev.position) {
                    return;
                }
                if scroller.update(cx, |shell, cx| shell.wheel_scroll(ev, cx)) {
                    cx.stop_propagation();
                }
            });
        },
    )
    .absolute()
    .size_full()
}

/// Klick auf ein Symbol einer Kachel oder Stack-Zeile: die Shell führt es aus (`tile_actions`).
fn icon_click(cx: &mut Context<Shell>, key: &str) -> tile::IconClick {
    let (entity, key) = (cx.entity(), key.to_string());
    Rc::new(move |icon, ev, window, app| {
        let force = ev.modifiers().alt;
        entity.update(app, |this, cx| this.tile_icon(&key, icon, force, window, cx));
    })
}

/// Kopfzeile ohne Zeiger aktiviert (Bedienhilfe „Drücken“): die Kachel bekommt den Fokus.
fn head_click(cx: &mut Context<Shell>, g: usize, s: usize) -> tile::HeadClick {
    let entity = cx.entity();
    Rc::new(move |window, app| {
        entity.update(app, |this, cx| this.focus_tile(g, s, window, cx));
    })
}

/// Eine Kachel mit ihrer Lage; neue blenden in 0,18 s ein.
fn one_tile(shell: &Shell, i: usize, frame: Rect, stacked: bool, cx: &mut Context<Shell>) -> AnyElement {
    let visible = shell.visible();
    let (g, s) = visible[i];
    let (group, session): (&Group, _) = (&shell.groups[g], &shell.groups[g].sessions[s]);
    let marks = shell.tile_marks((g, s), visible.len());
    let a = crate::appearance::current(cx);
    let geo = tile::Geometry {
        width: frame.w,
        height: frame.h,
        corner: tile_look::corner_radius((frame.x, frame.y), a.gap, a.scale),
        hovered: shell.layout.hover.as_deref() == Some(session.key.as_str()),
        header_hidden: stacked,
        row_below: stacked && i + 1 != visible.len(),
    };
    let elsewhere = shell.owners.elsewhere(0, &session.key);
    let on_head = head_click(cx, g, s);
    let el = tile::cell(&tile::Cell { group, session, marks, geo, body: true, elsewhere, on_icon: Some(icon_click(cx, &session.key)), on_head: Some(on_head) }, cx);
    let spot = if stacked { Spot::Body } else { Spot::Tile };
    fade_in(shell, &session.key, placed(frame, g, s, &session.key, spot, el, cx))
}

/// Neue Kacheln blenden in 0,18 s ein (nicht beim ersten Laden).
fn fade_in(shell: &Shell, key: &str, el: gpui::Stateful<gpui::Div>) -> AnyElement {
    if !shell.layout.is_fading(key) {
        return el.into_any_element();
    }
    el.with_animation(gpui::ElementId::Name(format!("tile-fade-{key}").into()), Animation::new(Duration::from_millis(180)), |el, delta| el.opacity(delta)).into_any_element()
}

/// Geisterebene einer entfernten Kachel: Fläche mit dem letzten Rahmen und dunkler Fuge, blendet in 0,15 s aus.
fn ghost(g: &tile_edge::Ghost, cx: &mut Context<Shell>) -> AnyElement {
    let t = theme(cx);
    let a = crate::appearance::current(cx);
    let r = g.frame;
    let corner = tile_look::corner_radius((r.x, r.y), a.gap, a.scale);
    div()
        .absolute()
        .left(px(r.x))
        .top(px(r.y))
        .w(px(r.w))
        .h(px(r.h))
        .rounded_tl(px(tile_edge::clamp_corner(corner, r.w, r.h)))
        .bg(a.panel_fill(t.bg))
        .border_1()
        .border_color(super::chrome::seam_dark(&t))
        .with_animation(
            gpui::ElementId::Name(format!("tile-ghost-{}", g.key).into()),
            Animation::new(tile_edge::FADE_OUT),
            |el, delta| el.opacity(1. - delta),
        )
        .into_any_element()
}

/// Eine Stack-Zeile mit ihrer Lage.
fn one_row(shell: &Shell, i: usize, frame: Rect, first: bool, body_above: bool, body_below: bool, cx: &mut Context<Shell>) -> AnyElement {
    let visible = shell.visible();
    let (g, s) = visible[i];
    let (group, session): (&Group, _) = (&shell.groups[g], &shell.groups[g].sessions[s]);
    let a = crate::appearance::current(cx);
    let on = shell.tile_marks((g, s), visible.len()).active || shell.ws.sync;
    let row = stack_row::RowCell {
        group,
        session,
        width: frame.w,
        height: frame.h,
        on,
        hovered: shell.layout.hover.as_deref() == Some(session.key.as_str()),
        first,
        body_above,
        body_below,
        corner: tile_look::corner_radius((frame.x, frame.y), a.gap, a.scale),
        show_path: shell.state.settings.stack_show_path(),
        on_icon: Some(icon_click(cx, &session.key)),
    };
    let el = stack_row::row(&row, cx);
    fade_in(shell, &session.key, placed(frame, g, s, &session.key, Spot::Row, el, cx))
}

/// Stack: Zeilen aller Kacheln und der Körper der aktiven. Die Zeilen liegen auf ihrem Platz, der Körper direkt unter seiner.
fn stack_tiles(shell: &Shell, rows: &[Rect], body: Rect, active: usize, cx: &mut Context<Shell>) -> Vec<AnyElement> {
    let n = rows.len();
    let mut out = Vec::new();
    for (i, row) in rows.iter().enumerate() {
        out.push(one_row(shell, i, *row, i == 0, i == active + 1, i == active, cx));
    }
    if n > 0 {
        out.push(one_tile(shell, active, body, true, cx));
    }
    out
}

fn tiles(shell: &Shell, frames: &[Rect], cx: &mut Context<Shell>) -> Vec<AnyElement> {
    let visible = shell.visible();
    let area = shell.layout.area();
    let mut out = Vec::new();
    for (i, frame) in frames.iter().enumerate().take(visible.len()) {
        // Beim Scrollen liegen Spalten ganz außerhalb: nicht zeichnen, Nachbarn per Pfeil finden sie trotzdem.
        if shell.layout.mode == LayoutMode::Scroll && visible.len() > 1 && !frame.intersects(area) {
            continue;
        }
        out.push(one_tile(shell, i, *frame, false, cx));
    }
    out
}

fn on_move(this: &mut Shell, ev: &MouseMoveEvent, cx: &mut Context<Shell>) {
    if this.layout.dragging.is_none() {
        return;
    }
    if ev.pressed_button != Some(MouseButton::Left) {
        this.end_drag();
        return;
    }
    let (ox, oy, _, _) = this.layout.measured.get().unwrap_or((0., 0., 0., 0.));
    this.layout.drag_to((ev.position.x.as_f32() - ox, ev.position.y.as_f32() - oy));
    cx.notify();
}

/// Arbeitsfläche: alle sichtbaren Kacheln samt ziehbaren Grenzen.
pub fn workspace(shell: &mut Shell, cx: &mut Context<Shell>) -> Stateful<gpui::Div> {
    let t = theme(cx);
    let a = crate::appearance::current(cx);
    shell.layout.gap = a.gap;
    shell.layout.ui_scale = a.scale;
    let visible = shell.visible();
    let keys: Vec<String> = visible.iter().map(|&(g, s)| shell.groups[g].sessions[s].key.clone()).collect();
    shell.layout.note_visible(&keys);
    let active = shell.selected.and_then(|sel| visible.iter().position(|id| *id == sel)).unwrap_or(0);
    // Stack gilt auch für eine einzelne Kachel, nicht aber bei Zoom und Vorschau (dann füllt eine Kachel die Fläche).
    let stacked = shell.layout.mode == LayoutMode::Stack && !shell.state.zoom && shell.preview_id().is_none();
    let (children, dividers) = if stacked {
        let (rows, body) = shell.layout.plan_stack(visible.len(), active);
        (stack_tiles(shell, &rows, body, active, cx), Vec::new())
    } else {
        let (frames, dividers) = shell.layout.plan(visible.len(), active);
        (tiles(shell, &frames, cx), dividers)
    };
    let shown: Vec<(String, Rect)> = keys.iter().cloned().zip(shell.layout.frames.iter().copied()).collect();
    let state = &shell.state;
    shell.layout.ghosts.update(std::time::Instant::now(), &shown, &|k| state.session(k).is_some());
    let ghosts: Vec<AnyElement> = shell.layout.ghosts.live().to_vec().iter().map(|g| ghost(g, cx)).collect();
    let covering = shell.layout.under_corner(a.scale);
    let empty = visible.is_empty().then(|| crate::misc::empty::view(shell, &t, cx));
    // Die Kacheln liegen in einer beschnittenen Fläche, die Ecke darüber ragt 2 px über den Rand hinaus (auf die Fugen).
    let mut clipped = div().absolute().size_full().overflow_hidden().child(measure_scrolling(shell.layout.measured.clone(), cx.entity())).children(children).children(ghosts).children(empty);
    for (i, d) in dividers.iter().enumerate() {
        clipped = clipped.child(grip(d, i, cx));
    }
    let area = div().id("workspace").relative().flex_1().min_w_0().min_h_0().bg(a.panel_fill(t.bg)).child(clipped).child(module_corner::view(&t, &a, covering));
    area.on_mouse_move(cx.listener(|this, ev: &MouseMoveEvent, _, cx| on_move(this, ev, cx)))
        .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, _| this.end_drag()))
        .on_mouse_up_out(MouseButton::Left, cx.listener(|this, _, _, _| this.end_drag()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const B: Rect = Rect { x: 0., y: 0., w: 1000., h: 600. };

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
        let mut st = LayoutState::new(LayoutMode::Stack);
        st.measured.set(Some((0., 0., 1000., 600.)));
        let (f, d) = st.plan(3, 1);
        assert!(d.is_empty());
        assert_eq!(f[0].h, ROW_HEIGHT);
        assert!(f[1].h > ROW_HEIGHT);
        assert_eq!(st.plan(1, 0).0, vec![B.inset(GAP)], "der Außenrand gilt auch für eine einzige Kachel");
    }

    #[test]
    fn stack_gives_rows_and_the_active_body_below_its_row_even_for_one_tile() {
        let mut st = LayoutState::new(LayoutMode::Stack);
        st.measured.set(Some((0., 0., 1000., 600.)));
        let (rows, body) = st.plan_stack(3, 1);
        assert_eq!(rows.len(), 3);
        assert_eq!((rows[0].x, rows[0].y, rows[0].h), (GAP, GAP, 26.));
        assert_eq!(body.y, rows[1].max_y(), "der Körper liegt direkt unter seiner Zeile");
        assert_eq!(rows[2].y, body.max_y());
        assert_eq!(body.h, 600. - 2. * GAP - 3. * 26.);
        let (rows, body) = st.plan_stack(1, 0);
        assert_eq!((rows.len(), body.y), (1, GAP + 26.));
        st.ui_scale = 1.3;
        assert_eq!(st.plan_stack(2, 0).0[0].h, 34., "26 mal UI-Größe, gerundet");
    }

    #[test]
    fn new_tiles_fade_in_but_not_at_the_first_load() {
        let mut st = LayoutState::new(LayoutMode::Grid);
        st.note_visible(&["a".into(), "b".into()]);
        assert!(!st.is_fading("a"), "beim ersten Laden erscheint alles sofort");
        st.note_visible(&["a".into(), "b".into(), "c".into()]);
        assert!(st.is_fading("c") && !st.is_fading("a"));
        st.note_visible(&["a".into()]);
        assert!(!st.is_fading("c"), "wer weg ist, blendet nicht mehr ein");
        assert!(st.cols_auto() && { st.cols = 3; !st.cols_auto() });
    }

    fn measured(mode: LayoutMode) -> LayoutState {
        let st = LayoutState::new(mode);
        st.measured.set(Some((0., 0., 1000., 600.)));
        st
    }

    #[test]
    fn scroll_plan_reveals_the_focus_tile_once_and_keeps_the_offset() {
        let mut st = measured(LayoutMode::Scroll);
        let (f, _) = st.plan(4, 3);
        assert!(f[3].max_x() <= 1000. - GAP + 0.5, "Fokus-Spalte ganz im Bild: {:?}", f[3]);
        assert!(st.scroll_x > 0.);
        assert!(!st.reveal);
        let x = st.scroll_x;
        st.plan(4, 0);
        assert_eq!(st.scroll_x, x, "ohne neuen Anlass bleibt der Versatz");
        st.reveal = true;
        st.plan(4, 0);
        assert_eq!(st.scroll_x, 0.);
        st.scroll_by(40.);
        st.plan(4, 0);
        assert_eq!(st.scroll_x, 40.);
        let mut other = measured(LayoutMode::Grid);
        other.scroll_by(40.);
        assert_eq!(other.scroll_x, 0., "nur Scrollen scrollt");
    }

    #[test]
    fn a_new_width_reveals_the_focus_tile_again() {
        let mut st = measured(LayoutMode::Scroll);
        st.plan(4, 3);
        st.reveal = false;
        st.measured.set(Some((0., 0., 700., 600.)));
        st.plan(4, 3);
        assert!(st.scroll_x > 0.);
        assert!(!st.reveal, "das Nachführen verbraucht den Anlass");
    }

    #[test]
    fn scroll_width_steps_through_thirds_and_halves() {
        let mut st = measured(LayoutMode::Scroll);
        assert!(st.step_scroll_width(1, Direction::Right));
        assert_eq!(st.ratios[SCROLL_KEY], vec![0.5, 2. / 3.]);
        assert!(!st.step_scroll_width(1, Direction::Right), "breiter als ⅔ gibt es nicht");
        assert!(st.step_scroll_width(1, Direction::Left));
        assert!(st.step_scroll_width(1, Direction::Left));
        assert!(!st.step_scroll_width(1, Direction::Left));
        assert!(!st.step_scroll_width(0, Direction::Up));
        assert_eq!(st.ratios[SCROLL_KEY], vec![0.5, 1. / 3.]);
        // Eine gespeicherte Liste mit anderer Länge als die Kachelzahl gilt weiter.
        assert_eq!(st.ratios_for(SCROLL_KEY, 7), Some(vec![0.5, 1. / 3.]));
    }

    #[test]
    fn nudge_changes_the_ratio_of_the_line_next_to_the_focus_tile() {
        let mut st = measured(LayoutMode::Main);
        let (frames, _) = st.plan(3, 0);
        assert_eq!(st.nudge(frames[0], Direction::Right).as_deref(), Some("main.cols"));
        assert!((st.ratios["main.cols"][0] - 0.55).abs() < 0.02);
        assert_eq!(st.nudge(frames[0], Direction::Down), None);
        st.reset("main.cols");
        assert!(st.ratios.is_empty());
    }

    #[test]
    fn settings_restore_columns_splits_and_ratios() {
        let mut s = Settings::default();
        s.set(GRID_COLUMNS_KEY, serde_json::json!(3));
        s.set(SCROLL_COLUMNS_KEY, serde_json::json!(0));
        s.set(SPLITS_KEY, serde_json::json!("dr"));
        s.set("layout.main.cols", serde_json::json!([0.7, 0.3]));
        s.set("layout.broken", serde_json::json!("x"));
        let st = LayoutState::from_settings(LayoutMode::Custom, &s);
        assert_eq!((st.cols, st.scroll_cols, st.splits.as_str()), (3, 1, "dr"));
        assert_eq!(st.ratios.get("main.cols"), Some(&vec![0.7, 0.3]));
        assert!(!st.ratios.contains_key("broken"));
        assert!(!st.ratios.contains_key("grid.columns"));
    }

    #[test]
    fn column_label_follows_the_mode() {
        let mut st = LayoutState::new(LayoutMode::Grid);
        assert_eq!(st.cols_label().as_deref(), Some("AUTO SP"));
        st.cols = 3;
        assert_eq!(st.cols_label().as_deref(), Some("3 SP"));
        st.mode = LayoutMode::Scroll;
        assert_eq!(st.cols_label().as_deref(), Some("2 SP"));
        st.mode = LayoutMode::Main;
        assert_eq!(st.cols_label(), None);
    }
}
