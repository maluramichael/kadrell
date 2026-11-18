//! Zeichnen der Charts mit GPUI-Pfaden (Ersatz für Swift Charts): Linien mit gestrichelten Brücken für die Limits,
//! gestapelte Balken für die Prompts, Raster und Tageslinien. Achsenbeschriftung und Tooltip sind normale Elemente
//! darüber (`sections`). Die Maus meldet ihre Lage über `on_hover`, der Tooltip entsteht beim nächsten Zeichnen.

use std::sync::Arc;

use gpui::{Bounds, Hsla, MouseMoveEvent, PathBuilder, Pixels, Point, Window, canvas, fill, point, px, rgb, size};
use kadrell_accounts::{LimitKind, LimitPoint, StatsSnapshot};

use super::format::count_ticks;
use crate::theme::Theme;

/// Rechts bleibt Platz, sonst schneidet der Rand die Beschriftung kurz vor „jetzt“ ab.
pub const END_PAD: f32 = 24.;
/// Innenabstand oben und unten, damit dicke Linien an 0 % und 100 % nicht abgeschnitten werden.
const PAD_Y: f32 = 3.;

const LIGHT: [u32; 7] = [0x2a78d6, 0xeb6834, 0x1baf7a, 0xeda100, 0xe87ba4, 0x008300, 0x6250d6];
const DARK: [u32; 7] = [0x3987e5, 0xd95926, 0x199e70, 0xc98500, 0xd55181, 0x008300, 0x9085e9];

/// Kategoriale Farben der Charts (aus der Swift-App, in beiden Modi geprüft). Ab dem achten Projekt gilt „Andere“ in Grau.
pub fn project_color(slot: Option<usize>, t: &Theme) -> Hsla {
    let Some(slot) = slot else { return t.muted };
    let colors = if t.dark { DARK } else { LIGHT };
    rgb(colors[slot % colors.len()]).into()
}

pub fn limit_color(kind: LimitKind, t: &Theme) -> Hsla {
    project_color(Some(kind.raw() as usize), t)
}

/// Farben und Raster, die ein Chart braucht.
#[derive(Clone)]
pub struct Style {
    pub grid: Hsla,
    pub day: Hsla,
    pub day_minor: Hsla,
    pub rule: Hsla,
    pub theme: Theme,
}

impl Style {
    pub fn new(t: &Theme) -> Style {
        Style { grid: t.muted.opacity(0.2), day: t.muted.opacity(0.55), day_minor: t.muted.opacity(0.25), rule: t.muted, theme: *t }
    }
}

/// Wohin die Maus zeigt: Chart, Abstand vom linken Rand und Breite der Fläche in Pixeln.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hover {
    pub chart: usize,
    pub x: f32,
    pub width: f32,
}

impl Hover {
    /// Zeitpunkt unter der Maus. `None` im freien Platz rechts der Daten.
    pub fn time(&self, from: f64, now: f64) -> Option<f64> {
        let plot = (self.width - END_PAD).max(1.);
        (self.x <= plot).then(|| from + f64::from(self.x / plot) * (now - from))
    }
}

pub type OnHover = std::rc::Rc<dyn Fn(Option<Hover>, &mut gpui::App)>;

fn x_of(ts: f64, snap: &StatsSnapshot, w: f32) -> f32 {
    let span = (snap.now - snap.from).max(1.);
    (((ts - snap.from) / span) as f32).clamp(0., 1.) * (w - END_PAD).max(1.)
}

fn pt(bounds: &Bounds<Pixels>, x: f32, y: f32) -> Point<Pixels> {
    point(bounds.origin.x + px(x), bounds.origin.y + px(y))
}

fn rect(bounds: &Bounds<Pixels>, x0: f32, y0: f32, x1: f32, y1: f32) -> Bounds<Pixels> {
    Bounds::new(pt(bounds, x0, y0), size(px((x1 - x0).max(0.)), px((y1 - y0).max(0.))))
}

fn hline(window: &mut Window, b: &Bounds<Pixels>, y: f32, color: Hsla) {
    window.paint_quad(fill(rect(b, 0., y - 0.5, b.size.width.as_f32(), y + 0.5), color));
}

fn vline(window: &mut Window, b: &Bounds<Pixels>, x: f32, color: Hsla) {
    window.paint_quad(fill(rect(b, x - 0.5, 0., x + 0.5, b.size.height.as_f32()), color));
}

/// Raster waagrecht (`y_fracs` von oben, 0 bis 1) und Tageslinien.
fn background(window: &mut Window, b: &Bounds<Pixels>, snap: &StatsSnapshot, y_fracs: &[f32], style: &Style) {
    let (w, h) = (b.size.width.as_f32(), b.size.height.as_f32());
    for f in y_fracs {
        hline(window, b, PAD_Y + f * (h - 2. * PAD_Y), style.grid);
    }
    for day in &snap.days {
        let color = if snap.range == kadrell_accounts::StatsRange::Month && !day.labeled { style.day_minor } else { style.day };
        vline(window, b, x_of(day.date, snap, w), color);
    }
}

/// Registriert die Mausbewegung für eine Fläche: innerhalb `Some`, sonst `None` (nur, wenn dieser Chart hovert).
fn track_hover(window: &mut Window, b: Bounds<Pixels>, chart: usize, active: bool, on_hover: OnHover) {
    window.on_mouse_event(move |ev: &MouseMoveEvent, phase, _, cx| {
        if !phase.bubble() {
            return;
        }
        if b.contains(&ev.position) {
            on_hover(Some(Hover { chart, x: (ev.position.x - b.origin.x).as_f32(), width: b.size.width.as_f32() }), cx);
        } else if active {
            on_hover(None, cx);
        }
    });
}

fn rule(window: &mut Window, b: &Bounds<Pixels>, hover: Option<Hover>, chart: usize, style: &Style) {
    if let Some(h) = hover.filter(|h| h.chart == chart && h.x <= h.width - END_PAD) {
        vline(window, b, h.x, style.rule);
    }
}

// MARK: Limits

fn series(points: &[LimitPoint], bridge: bool) -> Vec<Vec<&LimitPoint>> {
    let mut by: std::collections::BTreeMap<String, Vec<&LimitPoint>> = std::collections::BTreeMap::new();
    for p in points.iter().filter(|p| p.bridge == bridge) {
        by.entry(p.series()).or_default().push(p);
    }
    by.into_values().collect()
}

fn y_limit(v: i32, h: f32) -> f32 {
    PAD_Y + (1. - v.clamp(0, 100) as f32 / 100.) * (h - 2. * PAD_Y)
}

fn stroke_line(window: &mut Window, b: &Bounds<Pixels>, pts: &[(f32, f32)], width: f32, color: Hsla) {
    let mut path = PathBuilder::stroke(px(width));
    let points: Vec<Point<Pixels>> = pts.iter().map(|(x, y)| pt(b, *x, *y)).collect();
    path.add_polygon(&points, false);
    if let Ok(p) = path.build() {
        window.paint_path(p, color);
    }
}

/// Gestrichelt: kurze Teilstücke nacheinander.
fn stroke_dashed(window: &mut Window, b: &Bounds<Pixels>, a: (f32, f32), z: (f32, f32), color: Hsla) {
    let (dx, dy) = (z.0 - a.0, z.1 - a.1);
    let len = (dx * dx + dy * dy).sqrt();
    let (dash, gap) = (4., 3.);
    let mut at = 0.;
    while at < len {
        let end = (at + dash).min(len);
        let (f0, f1) = (at / len, end / len);
        stroke_line(window, b, &[(a.0 + dx * f0, a.1 + dy * f0), (a.0 + dx * f1, a.1 + dy * f1)], 1.5, color);
        at += dash + gap;
    }
}

fn paint_limit_card(window: &mut Window, b: Bounds<Pixels>, snap: &StatsSnapshot, idx: usize, hover: Option<Hover>, style: &Style) {
    let Some(account) = snap.limits.get(idx) else { return };
    let (w, h) = (b.size.width.as_f32(), b.size.height.as_f32());
    background(window, &b, snap, &[0., 0.5, 1.], style);
    let xy = |p: &LimitPoint| (x_of(p.date, snap, w), y_limit(p.value, h));
    // Nicht gemessen: gestrichelt, dünner und blasser, damit die Messwerte vorn bleiben.
    for line in series(&account.bridges, true) {
        if let [a, z] = line.as_slice() {
            stroke_dashed(window, &b, xy(a), xy(z), limit_color(a.kind, &style.theme).opacity(0.6));
        }
    }
    for line in series(&account.points, false) {
        let color = limit_color(line[0].kind, &style.theme);
        let pts: Vec<(f32, f32)> = line.iter().map(|p| xy(p)).collect();
        if pts.len() == 1 {
            window.paint_quad(fill(rect(&b, pts[0].0 - 2.5, pts[0].1 - 2.5, pts[0].0 + 2.5, pts[0].1 + 2.5), color).corner_radii(px(2.5)));
        } else {
            stroke_line(window, &b, &pts, 2., color);
        }
    }
    rule(window, &b, hover, idx, style);
}

/// Zeichenfläche eines Konto-Charts. `chart` ist der Index der Kachel (für `Hover`).
pub fn limit_chart(snap: Arc<StatsSnapshot>, idx: usize, hover: Option<Hover>, style: Style, on_hover: OnHover) -> gpui::Canvas<()> {
    canvas(
        |_, _, _| (),
        move |b, _, window, _| {
            track_hover(window, b, idx, hover.is_some_and(|h| h.chart == idx), on_hover.clone());
            paint_limit_card(window, b, &snap, idx, hover, &style);
        },
    )
}

// MARK: Prompts

/// Höchster Stapel, aufgerundet auf einen Rasterwert.
pub fn prompt_max(snap: &StatsSnapshot) -> usize {
    let max = snap.bars.iter().map(|b| b.high).max().unwrap_or(1).max(1);
    count_ticks(max).last().copied().unwrap_or(max).max(1)
}

fn paint_prompts(window: &mut Window, b: Bounds<Pixels>, snap: &StatsSnapshot, hover: Option<Hover>, chart: usize, style: &Style) {
    let (w, h) = (b.size.width.as_f32(), b.size.height.as_f32());
    let max = prompt_max(snap) as f32;
    let ticks = count_ticks(max as usize);
    let fracs: Vec<f32> = ticks.iter().map(|t| 1. - *t as f32 / max).collect();
    background(window, &b, snap, &fracs, style);
    let y = |v: usize| PAD_Y + (1. - v as f32 / max) * (h - 2. * PAD_Y);
    for bar in &snap.bars {
        let (lo, hi) = bar.visible_span(snap.from, snap.now);
        let (x0, x1) = (x_of(lo, snap, w), x_of(hi, snap, w));
        window.paint_quad(fill(rect(&b, x0, y(bar.high), x1.max(x0 + 1.), y(bar.low)), crate::ui::stats::draw::project_color(bar.slot, &style.theme)));
    }
    rule(window, &b, hover, chart, style);
}

pub fn prompt_chart(snap: Arc<StatsSnapshot>, chart: usize, hover: Option<Hover>, style: Style, on_hover: OnHover) -> gpui::Canvas<()> {
    canvas(
        |_, _, _| (),
        move |b, _, window, _| {
            track_hover(window, b, chart, hover.is_some_and(|h| h.chart == chart), on_hover.clone());
            paint_prompts(window, b, &snap, hover, chart, &style);
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hover_maps_x_to_time_and_ignores_the_padding() {
        let h = Hover { chart: 0, x: 0., width: 224. };
        assert_eq!(h.time(1000., 3000.), Some(1000.));
        let mid = Hover { chart: 0, x: 100., width: 224. };
        assert_eq!(mid.time(1000., 3000.), Some(2000.));
        assert_eq!(Hover { chart: 0, x: 210., width: 224. }.time(1000., 3000.), None);
    }

    #[test]
    fn limit_values_map_inside_the_padding() {
        assert_eq!(y_limit(0, 100.), 97.);
        assert_eq!(y_limit(100, 100.), 3.);
        assert_eq!(y_limit(150, 100.), 3.);
    }

    #[test]
    fn palette_has_two_variants() {
        let dark = crate::theme::Theme::mocha();
        let light = crate::theme::Theme::latte();
        assert_ne!(project_color(Some(0), &dark), project_color(Some(0), &light));
        assert_eq!(project_color(None, &dark), dark.muted);
    }
}
