//! Reine Kachel-Mathematik der Arbeitsfläche, Port von `app/Kadrell/Workspace/Tiling.swift`: die sieben Layouts,
//! ziehbare Grenzen, Nachbarn, Stack und das seitliche Scrollen. Alles in Bildschirm-Pixeln, ohne GPUI.

/// Abstand zwischen den Kacheln und zum Rand (`Settings.tileGap` der Mac-App).
pub const GAP: f32 = 6.;
/// Titelzeile einer Stack-Zeile.
pub const ROW_HEIGHT: f32 = 26.;
/// Kleinster Anteil eines Feldes beim Ziehen.
const MIN_RATIO: f64 = 0.05;
const MAX_COLS: usize = 9;
/// Spaltenbreiten beim Scrollen, ⌃⌥←/→ schaltet eine Stufe weiter.
pub const SCROLL_WIDTHS: [f64; 3] = [1. / 3., 0.5, 2. / 3.];
/// Schlüssel der Spaltenbreiten im Layout Scrollen.
pub const SCROLL_KEY: &str = "scroll.widths";

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
    pub fn mid_x(&self) -> f32 {
        self.x + self.w / 2.
    }
    pub fn mid_y(&self) -> f32 {
        self.y + self.h / 2.
    }
    pub fn inset(&self, d: f32) -> Rect {
        Rect::new(self.x + d, self.y + d, (self.w - 2. * d).max(0.), (self.h - 2. * d).max(0.))
    }
    pub fn offset_x(&self, dx: f32) -> Rect {
        Rect::new(self.x + dx, self.y, self.w, self.h)
    }
    /// Überschneiden sich die Flächen (Berührung an der Kante zählt nicht)?
    pub fn intersects(&self, o: Rect) -> bool {
        self.x < o.max_x() && o.x < self.max_x() && self.y < o.max_y() && o.y < self.max_y()
    }
}

/// Die sieben Layouts in der Reihenfolge von ⌘L (`LayoutMode.allCases` der Mac-App).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LayoutMode {
    Grid,
    Main,
    Spiral,
    /// Frei (i3): jede Kachel teilt das Feld der vorigen rechts oder unten.
    Custom,
    /// Scrollen (niri): Spalten fester Breite, was nicht passt, ragt rechts hinaus.
    Scroll,
    Row,
    Stack,
}

impl LayoutMode {
    pub const ALL: [LayoutMode; 7] =
        [LayoutMode::Grid, LayoutMode::Main, LayoutMode::Spiral, LayoutMode::Custom, LayoutMode::Scroll, LayoutMode::Row, LayoutMode::Stack];

    /// Reihum wie tmux `next-layout`.
    pub fn next(self) -> LayoutMode {
        let i = Self::ALL.iter().position(|m| *m == self).unwrap_or(0);
        Self::ALL[(i + 1) % Self::ALL.len()]
    }

    pub fn from_name(name: &str) -> Option<LayoutMode> {
        Self::ALL.into_iter().find(|m| m.key() == name)
    }

    pub fn key(self) -> &'static str {
        match self {
            LayoutMode::Grid => "grid",
            LayoutMode::Main => "main",
            LayoutMode::Spiral => "spiral",
            LayoutMode::Custom => "custom",
            LayoutMode::Scroll => "scroll",
            LayoutMode::Row => "row",
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

impl Direction {
    pub fn horizontal(self) -> bool {
        matches!(self, Direction::Left | Direction::Right)
    }
    /// Nach rechts oder unten.
    pub fn forward(self) -> bool {
        matches!(self, Direction::Right | Direction::Down)
    }
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

/// Einstellungen der Vorlagen, die nicht zu den Verhältnissen gehören.
#[derive(Clone, Copy, Debug)]
pub struct Opts<'a> {
    /// Grid: feste Spaltenzahl, 0 = ⌈√n⌉.
    pub cols: usize,
    /// Scrollen: sichtbare Spalten ohne eigene Breite.
    pub scroll_cols: usize,
    /// Frei: Zeichen i teilt Feld i, `r` rechts, `d` unten, `a` entlang der längeren Seite.
    pub splits: &'a str,
}

impl Default for Opts<'_> {
    fn default() -> Self {
        Opts { cols: 0, scroll_cols: 2, splits: "" }
    }
}

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

    /// Kette: jede Kachel teilt den Rest. Spirale (bspwm) abwechselnd senkrecht und waagerecht. Frei (i3, bspwm-Insert):
    /// Kachel i+1 teilt Feld i rechts oder unten, ohne Vorgabe entlang der längeren Seite. Geteilt wird immer der Rest
    /// (Kette, kein Baum): 2×2 geht so nicht, dafür gibt es das Grid.
    fn chain(&mut self, n: usize, b: Rect, mode: LayoutMode, dirs: &[char]) -> Vec<Rect> {
        let mut area = b;
        let mut frames = Vec::new();
        for i in 0..n.saturating_sub(1) {
            let vertical = match mode {
                LayoutMode::Spiral => i % 2 == 0,
                _ => match dirs.get(i) {
                    Some(&c) if c != 'a' => c == 'r',
                    _ => area.w >= area.h,
                },
            };
            let parts = self.split(area, vertical, &format!("{}.{i}", mode.key()), 2);
            frames.push(parts[0]);
            area = parts[1];
        }
        frames.push(area);
        frames
    }
}

/// niri: Spalten fester Breite (Anteil der Fläche) nebeneinander, was nicht passt, ragt rechts hinaus. Ohne eigene
/// Breite füllt eine Spalte 1/`columns` der Fläche, so passen genau `columns` Spalten ins Sichtfeld.
fn scroll_frames(n: usize, b: Rect, gap: f32, columns: usize, widths: &[f64]) -> Vec<Rect> {
    let default = 1.0 / columns.max(1) as f64;
    let mut x = b.x;
    let mut frames = Vec::new();
    for i in 0..n {
        let share = widths.get(i).copied().unwrap_or(default).clamp(0.1, 1.) as f32;
        let w = ((b.w + gap) * share - gap).round();
        frames.push(Rect::new(x, b.y, w, b.h));
        x += w + gap;
    }
    frames
}

/// Die Vorlage des Layouts für `n` Kacheln: Felder in Reihenfolge und ihre ziehbaren Grenzen. Stack hat keine
/// Vorlage (siehe `stack`) und füllt wie das Grid.
pub fn layout(mode: LayoutMode, n: usize, b: Rect, gap: f32, opts: Opts, ratios: Ratios) -> (Vec<Rect>, Vec<SplitLine>) {
    if n == 0 {
        return (Vec::new(), Vec::new());
    }
    let mut s = Splitter { gap, ratios, dividers: Vec::new() };
    let frames = match mode {
        LayoutMode::Grid | LayoutMode::Stack => s.grid(n, b, opts.cols),
        LayoutMode::Row => s.grid(n, b, n),
        LayoutMode::Main => s.main(n, b),
        LayoutMode::Spiral | LayoutMode::Custom => s.chain(n, b, mode, &opts.splits.chars().collect::<Vec<_>>()),
        LayoutMode::Scroll => scroll_frames(n, b, gap, opts.scroll_cols, &ratios(SCROLL_KEY, n).unwrap_or_default()),
    };
    (frames, s.dividers)
}

/// Neuer Versatz der scrollenden Fläche: `reveal` (Fokus-Feld ohne Versatz) wird ganz sichtbar, der Rand bleibt im
/// Inhalt.
pub fn scroll_offset(current: f32, reveal: Option<Rect>, content_max_x: f32, b: Rect) -> f32 {
    let mut x = current;
    if let Some(r) = reveal {
        if r.max_x() - x > b.max_x() {
            x = r.max_x() - b.max_x();
        }
        if r.x - x < b.x {
            x = r.x - b.x;
        }
    }
    x.clamp(0., (content_max_x - b.max_x()).max(0.))
}

/// Die Stufe von `SCROLL_WIDTHS`, die `width` am nächsten liegt.
pub fn nearest_scroll_width(width: f64) -> usize {
    let dist = |i: usize| (SCROLL_WIDTHS[i] - width).abs();
    (0..SCROLL_WIDTHS.len()).min_by(|a, b| dist(*a).total_cmp(&dist(*b))).unwrap_or(1)
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

/// Die Grenze, die ⌃⌥-Pfeil `d` an der Kachel `frame` verschiebt: bevorzugt die Linie auf der Seite des Pfeils, sonst
/// die gegenüberliegende (`resizeDivider` der Mac-App). `reach` ist, wie weit die Linie von der Kante liegen darf.
pub fn divider_for(dividers: &[SplitLine], frame: Rect, d: Direction, reach: f32) -> Option<&SplitLine> {
    let vertical = d.horizontal();
    let forward = d.forward();
    let (lo, hi) = if vertical { (frame.x, frame.max_x()) } else { (frame.y, frame.max_y()) };
    let near = |line: &&SplitLine| {
        line.vertical == vertical
            && if vertical { line.rect.y < frame.max_y() && line.rect.max_y() > frame.y } else { line.rect.x < frame.max_x() && line.rect.max_x() > frame.x }
    };
    let mid = |line: &SplitLine| if vertical { line.rect.mid_x() } else { line.rect.mid_y() };
    let at = |edge: f32| dividers.iter().filter(near).find(|l| (mid(l) - edge).abs() <= reach);
    at(if forward { hi } else { lo }).or_else(|| at(if forward { lo } else { hi }))
}

/// Zielposition für einen Schritt von 5 % der Fläche der Linie `d` in Richtung `dir`.
pub fn nudge_target(d: &SplitLine, dir: Direction) -> (f32, f32) {
    let sign = if dir.forward() { 1. } else { -1. };
    if d.vertical {
        (d.rect.mid_x() + d.span.w * 0.05 * sign, 0.)
    } else {
        (0., d.rect.mid_y() + d.span.h * 0.05 * sign)
    }
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

/// Nachbar-Index nach Reihenfolge, für Stack und Zoom: ohne Geometrie. Grid-Raster sonst. `None` am Rand.
pub fn neighbor_index(i: usize, n: usize, mode: LayoutMode, d: Direction) -> Option<usize> {
    if i >= n {
        return None;
    }
    let (i, cols) = (i as isize, columns(n) as isize);
    let j = if mode == LayoutMode::Stack {
        if d.forward() { i + 1 } else { i - 1 }
    } else {
        match d {
            Direction::Left if i % cols == 0 => -1,
            Direction::Left => i - 1,
            Direction::Right if i % cols == cols - 1 => -1,
            Direction::Right => i + 1,
            Direction::Up => i - cols,
            Direction::Down => i + cols,
        }
    };
    (0..n as isize).contains(&j).then_some(j as usize)
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

/// Sichtbare Spalten beim Scrollen nach ‹ ›: mindestens 1, höchstens 12.
pub fn step_scroll_cols(current: usize, delta: isize) -> usize {
    (current as isize + delta).clamp(1, 12) as usize
}

/// Die Teilungsrichtung an Position `i` setzen: die Liste wird bei Bedarf mit `a` aufgefüllt.
pub fn set_split(splits: &str, i: usize, dir: char) -> String {
    let mut s: Vec<char> = splits.chars().collect();
    while s.len() <= i {
        s.push('a');
    }
    s[i] = dir;
    s.into_iter().collect()
}

/// Die Teilungsrichtung an Position `i`, `a` ohne Vorgabe.
pub fn split_at(splits: &str, i: usize) -> char {
    splits.chars().nth(i).unwrap_or('a')
}

/// Nächste Teilungsrichtung beim Klick auf die Taste: auto, rechts, unten, auto.
pub fn next_split(current: char) -> char {
    match current {
        'a' => 'r',
        'r' => 'd',
        _ => 'a',
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const B: Rect = Rect { x: 0., y: 0., w: 1000., h: 600. };

    fn none(_: &str, _: usize) -> Option<Vec<f64>> {
        None
    }

    fn lay(mode: LayoutMode, n: usize, gap: f32) -> Vec<Rect> {
        layout(mode, n, B, gap, Opts::default(), &none).0
    }

    #[test]
    fn grid_columns_and_fill() {
        assert!(lay(LayoutMode::Grid, 0, 6.).is_empty());
        assert_eq!(lay(LayoutMode::Grid, 1, 6.), vec![B]);
        let five = lay(LayoutMode::Grid, 5, 6.);
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
        let wide = lay(LayoutMode::Grid, 2, 40.);
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
    fn neighbor_by_count_for_grid_and_stack() {
        // 5 Kacheln im Grid: 3 Spalten, Indizes 0 1 2 / 3 4
        let g = |i, d| neighbor_index(i, 5, LayoutMode::Grid, d);
        assert_eq!(g(0, Direction::Right), Some(1));
        assert_eq!(g(2, Direction::Right), None);
        assert_eq!(g(3, Direction::Left), None);
        assert_eq!(g(1, Direction::Down), Some(4));
        assert_eq!(g(2, Direction::Down), None);
        assert_eq!(g(4, Direction::Up), Some(1));
        assert_eq!(neighbor_index(1, 3, LayoutMode::Stack, Direction::Down), Some(2));
        assert_eq!(neighbor_index(0, 3, LayoutMode::Stack, Direction::Up), None);
        assert_eq!(neighbor_index(7, 3, LayoutMode::Grid, Direction::Up), None);
    }

    #[test]
    fn templates_fill_in_order() {
        let r = |k: &str, _: usize| (k == "main.cols").then(|| vec![0.7, 0.3]);
        let (f, d) = layout(LayoutMode::Main, 3, B, 0., Opts::default(), &r);
        assert_eq!(f[0], Rect::new(0., 0., 700., 600.));
        assert_eq!(f[1], Rect::new(700., 0., 300., 300.));
        assert_eq!(f[2].max_y(), 600.);
        assert_eq!(d.iter().map(|d| d.key.as_str()).collect::<Vec<_>>(), ["main.cols", "main.rows.2"]);
        let sp = lay(LayoutMode::Spiral, 3, 0.);
        assert_eq!(sp[0], Rect::new(0., 0., 500., 600.));
        assert_eq!(sp[1], Rect::new(500., 0., 500., 300.));
        assert_eq!(sp[2], Rect::new(500., 300., 500., 300.));
        let r3 = |k: &str, _: usize| (k == "grid.cols.3").then(|| vec![0.5, 0.25, 0.25]);
        let g = layout(LayoutMode::Grid, 3, B, 0., Opts { cols: 3, ..Opts::default() }, &r3).0;
        assert_eq!(g.iter().map(|r| r.w).collect::<Vec<_>>(), [500., 250., 250.]);
        let bad = |_: &str, _: usize| Some(vec![1.]);
        let g = layout(LayoutMode::Grid, 2, B, 0., Opts::default(), &bad).0;
        assert_eq!(g.iter().map(|r| r.w).collect::<Vec<_>>(), [500., 500.]);
    }

    #[test]
    fn custom_splits_per_position() {
        // Ohne Vorgabe entlang der längeren Seite: 1000×600 erst rechts, der Rest 500×600 dann unten.
        let auto = lay(LayoutMode::Custom, 3, 0.);
        assert_eq!(auto, vec![Rect::new(0., 0., 500., 600.), Rect::new(500., 0., 500., 300.), Rect::new(500., 300., 500., 300.)]);
        // `d` an Position 0: Kachel 1 unter Kachel 0; Position 1 `r`; zu lange oder kurze Listen stören nicht.
        let with = |n, splits| layout(LayoutMode::Custom, n, B, 0., Opts { splits, ..Opts::default() }, &none).0;
        let f = with(3, "dr");
        assert_eq!(f[0], Rect::new(0., 0., 1000., 300.));
        assert_eq!(f[1], Rect::new(0., 300., 500., 300.));
        assert_eq!(f[2], Rect::new(500., 300., 500., 300.));
        assert_eq!(with(1, "dr"), vec![B]);
        assert_eq!(with(4, "d").len(), 4);
        let r = |k: &str, _: usize| (k == "custom.0").then(|| vec![0.3, 0.7]);
        let (f, d) = layout(LayoutMode::Custom, 2, B, 0., Opts { splits: "r", ..Opts::default() }, &r);
        assert_eq!(f[0].w, 300.);
        assert_eq!(d.iter().map(|d| d.key.as_str()).collect::<Vec<_>>(), ["custom.0"]);
    }

    #[test]
    fn scroll_columns_and_offset() {
        // ½ ist Standard: zwei Spalten füllen die Fläche, die dritte liegt rechts außerhalb.
        let gap = 10.;
        let r = |k: &str, _: usize| (k == SCROLL_KEY).then(|| vec![0.5, 0.5, 2. / 3.]);
        let (f, d) = layout(LayoutMode::Scroll, 3, B, gap, Opts::default(), &r);
        assert_eq!(f[0], Rect::new(0., 0., 495., 600.));
        assert_eq!(f[1].x, 505.);
        assert_eq!(f[1].max_x(), 1000.);
        assert_eq!(f[2].x, 1010.);
        assert_eq!(f[2].w, (1010.0f32 * 2. / 3. - 10.).round());
        assert!(d.is_empty());
        let content = f[2].max_x();
        // Fokus rechts außerhalb: so weit schieben, dass er ganz rechts sitzt; zurück nach links wieder bis 0.
        let x = scroll_offset(0., Some(f[2]), content, B);
        assert_eq!(x, content - 1000.);
        assert_eq!(scroll_offset(x, Some(f[0]), content, B), 0.);
        // Ohne Fokus nur begrenzen: nicht über den Inhalt hinaus, nicht unter 0.
        assert_eq!(scroll_offset(5000., None, content, B), content - 1000.);
        assert_eq!(scroll_offset(-50., None, content, B), 0.);
        assert_eq!(scroll_offset(300., None, 800., B), 0.);
    }

    #[test]
    fn row_fills_one_row_and_scroll_column_count() {
        // Reihe: alle Kacheln nebeneinander, gleich breit, volle Höhe, keine überstehende.
        let row = lay(LayoutMode::Row, 3, 0.);
        assert_eq!(row.iter().map(|r| r.y).collect::<Vec<_>>(), [0., 0., 0.]);
        assert_eq!(row.iter().map(|r| r.h).collect::<Vec<_>>(), [600., 600., 600.]);
        assert_eq!(row.iter().map(|r| r.w).sum::<f32>(), 1000.);
        assert_eq!(row.last().map(Rect::max_x), Some(1000.));
        // Scrollen: N sichtbare Spalten teilen die Fläche, ohne gezogene Breite.
        let four = layout(LayoutMode::Scroll, 4, B, 0., Opts { scroll_cols: 4, ..Opts::default() }, &none).0;
        assert_eq!(four.iter().map(|r| r.w).collect::<Vec<_>>(), [250., 250., 250., 250.]);
        assert_eq!(four.last().map(Rect::max_x), Some(1000.));
    }

    #[test]
    fn drag_moves_boundary_and_clamps() {
        let d = layout(LayoutMode::Main, 2, B, 10., Opts::default(), &none).1[0].clone();
        assert!(d.vertical);
        let moved = drag(&d, (305., 0.), 10., &[]);
        assert!((moved[0] - 300. / 990.).abs() < 0.001);
        assert!((moved.iter().sum::<f64>() - 1.).abs() < 0.0001);
        assert!((drag(&d, (-500., 0.), 10., &[])[0] - 0.05).abs() < 0.0001);
        assert!((drag(&d, (5000., 0.), 10., &[])[1] - 0.05).abs() < 0.0001);
    }

    #[test]
    fn drag_keeps_other_fields() {
        let d = layout(LayoutMode::Grid, 3, B, 0., Opts { cols: 3, ..Opts::default() }, &none).1[1].clone();
        let v = drag(&d, (600., 0.), 0., &[0.2, 0.4, 0.4]);
        assert!((v[0] - 0.2).abs() < 1e-6);
        assert!((v[1] - 0.4).abs() < 1e-6);
        assert!((v[2] - 0.4).abs() < 1e-6);
        let v = drag(&d, (800., 0.), 0., &[0.2, 0.4, 0.4]);
        assert!((v[1] - 0.6).abs() < 1e-6 && (v[2] - 0.2).abs() < 1e-6);
    }

    #[test]
    fn neighbor_by_frames() {
        let f = lay(LayoutMode::Main, 3, 0.);
        assert_eq!(neighbor(0, &f, Direction::Right), Some(1));
        assert_eq!(neighbor(2, &f, Direction::Left), Some(0));
        assert_eq!(neighbor(1, &f, Direction::Down), Some(2));
        assert_eq!(neighbor(0, &f, Direction::Left), None);
        assert_eq!(neighbor(9, &f, Direction::Left), None);
    }

    #[test]
    fn mode_cycles_in_swift_order_and_cols_step() {
        let mut order = vec![LayoutMode::Grid];
        while order.len() < 7 {
            order.push(order[order.len() - 1].next());
        }
        let keys: Vec<_> = order.iter().map(|m| m.key()).collect();
        assert_eq!(keys, ["grid", "main", "spiral", "custom", "scroll", "row", "stack"]);
        assert_eq!(LayoutMode::Stack.next(), LayoutMode::Grid);
        assert_eq!(LayoutMode::from_name("scroll"), Some(LayoutMode::Scroll));
        assert_eq!(LayoutMode::from_name("kachel"), None);
        assert_eq!(columns(5), 3);
        assert_eq!(step_cols(0, 4, 1), 3);
        assert_eq!(step_cols(0, 4, -1), 1);
        assert_eq!(step_cols(1, 4, -1), 0);
        assert_eq!(step_cols(9, 4, 1), 9);
        assert_eq!(step_scroll_cols(1, -1), 1);
        assert_eq!(step_scroll_cols(2, 1), 3);
    }

    #[test]
    fn nudge_moves_the_divider_next_to_the_focus_tile_by_five_percent() {
        let (frames, dividers) = layout(LayoutMode::Main, 3, B, 0., Opts::default(), &none);
        // Rechtspfeil an der linken Kachel: die senkrechte Linie an ihrer rechten Kante.
        let line = divider_for(&dividers, frames[0], Direction::Right, 16.).unwrap();
        assert_eq!(line.key, "main.cols");
        let (x, _) = nudge_target(line, Direction::Right);
        let v = drag(line, (x, 0.), 0., &[]);
        assert!((v[0] - 0.55).abs() < 1e-6, "{v:?}");
        // Linkspfeil an der rechten Kachel: dieselbe Linie wandert nach links.
        let line = divider_for(&dividers, frames[1], Direction::Left, 16.).unwrap();
        let (x, _) = nudge_target(line, Direction::Left);
        assert!((drag(line, (x, 0.), 0., &[])[0] - 0.45).abs() < 1e-6);
        // Pfeil ohne Linie auf dieser Achse und Seite: nichts. Die linke Kachel hat unten keine Linie.
        assert!(divider_for(&dividers, frames[0], Direction::Down, 16.).is_none());
        // Zwischen den rechten Kacheln liegt eine waagerechte Linie.
        let line = divider_for(&dividers, frames[1], Direction::Down, 16.).unwrap();
        assert_eq!(line.key, "main.rows.2");
    }

    #[test]
    fn scroll_widths_snap_to_the_nearest_step() {
        assert_eq!(nearest_scroll_width(0.5), 1);
        assert_eq!(nearest_scroll_width(0.3), 0);
        assert_eq!(nearest_scroll_width(0.9), 2);
    }

    #[test]
    fn split_list_helpers() {
        assert_eq!(set_split("", 2, 'r'), "aar");
        assert_eq!(set_split("dr", 0, 'a'), "ar");
        assert_eq!(split_at("dr", 1), 'r');
        assert_eq!(split_at("dr", 5), 'a');
        assert_eq!(next_split('a'), 'r');
        assert_eq!(next_split('r'), 'd');
        assert_eq!(next_split('d'), 'a');
    }

    #[test]
    fn rect_intersection_ignores_touching_edges() {
        let r = Rect::new(0., 0., 100., 50.);
        assert!(r.intersects(Rect::new(99., 10., 50., 10.)));
        assert!(!r.intersects(Rect::new(100., 10., 50., 10.)));
    }
}
