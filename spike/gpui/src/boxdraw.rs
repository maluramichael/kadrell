//! Eingebautes Zeichnen von Box-Drawing, Blockelementen und Powerline-Trennern.
//!
//! Die Schrift-Glyphen dieser Zeichen füllen die Zelle nicht, weil die Zeilenhöhe 1,25 x Schriftgröße beträgt:
//! zwischen den Zeilen und Zellen bleiben Lücken. Deshalb zeichnet der Renderer sie als Geometrie, die die
//! Zelle exakt ausfüllt (wie der eingebaute Box-Drawing-Modus von Alacritty und Ghostty).
//!
//! `shapes` ist rein (Zeichen und Zellgröße rein, Formen in Zellkoordinaten raus) und ohne GPUI testbar,
//! `paint` übersetzt die Formen auf das Fenster.

use gpui::{Bounds, Hsla, PathBuilder, Pixels, Point, Window, fill, point, px, size};

/// Kontrollpunkt-Faktor für einen Viertelkreis aus einer kubischen Bézierkurve.
const KAPPA: f32 = 0.552_284_8;

/// Ein Pfadbefehl in Zellkoordinaten (Pixel, Ursprung oben links in der Zelle).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Cmd {
    Move(f32, f32),
    Line(f32, f32),
    Cubic { c1: (f32, f32), c2: (f32, f32), to: (f32, f32) },
    Close,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    /// Gefülltes Rechteck, `alpha` skaliert die Vordergrundfarbe (Schattierungen).
    Rect { x0: f32, y0: f32, x1: f32, y1: f32, alpha: f32 },
    /// Gefüllter (`stroke: None`) oder gestrichelter Pfad mit der Linienbreite `stroke`.
    Path { cmds: Vec<Cmd>, stroke: Option<f32> },
}

/// Gehört das Zeichen zu den selbst gezeichneten?
pub fn is_builtin(c: char) -> bool {
    matches!(c, '\u{2500}'..='\u{259F}' | '\u{E0B0}'..='\u{E0B7}')
}

/// Formen für `c` in einer Zelle der Größe `w` x `h`. Leer, wenn das Zeichen nicht selbst gezeichnet wird.
pub fn shapes(c: char, w: f32, h: f32) -> Vec<Shape> {
    match c {
        '\u{2500}'..='\u{257F}' => lines(c as u32 - 0x2500, w, h),
        '\u{2580}'..='\u{259F}' => blocks(c as u32 - 0x2580, w, h),
        '\u{E0B0}'..='\u{E0B7}' => powerline(c as u32 - 0xE0B0, w, h),
        _ => Vec::new(),
    }
}

fn rect(x0: f32, y0: f32, x1: f32, y1: f32) -> Shape {
    Shape::Rect { x0, y0, x1, y1, alpha: 1. }
}

// ---------- Blockelemente ----------

fn blocks(i: u32, w: f32, h: f32) -> Vec<Shape> {
    let shade = |alpha: f32| vec![Shape::Rect { x0: 0., y0: 0., x1: w, y1: h, alpha }];
    match i {
        0x00 => vec![rect(0., 0., w, h / 2.)],
        0x01..=0x07 => vec![rect(0., h - h * i as f32 / 8., w, h)],
        0x08 => vec![rect(0., 0., w, h)],
        0x09..=0x0F => vec![rect(0., 0., w * (0x10 - i) as f32 / 8., h)],
        0x10 => vec![rect(w / 2., 0., w, h)],
        0x11 => shade(0.25),
        0x12 => shade(0.5),
        0x13 => shade(0.75),
        0x14 => vec![rect(0., 0., w, h / 8.)],
        0x15 => vec![rect(w - w / 8., 0., w, h)],
        _ => quadrants(i - 0x16, w, h),
    }
}

/// Viertelblöcke U+2596..U+259F; Bits: 1 = links oben, 2 = rechts oben, 4 = links unten, 8 = rechts unten.
fn quadrants(i: u32, w: f32, h: f32) -> Vec<Shape> {
    const MASKS: [u8; 10] = [4, 8, 1, 1 | 4 | 8, 1 | 8, 1 | 2 | 4, 1 | 2 | 8, 2, 2 | 4, 2 | 4 | 8];
    let mask = MASKS[i as usize];
    let (mx, my) = (w / 2., h / 2.);
    let cells = [(0., 0., mx, my), (mx, 0., w, my), (0., my, mx, h), (mx, my, w, h)];
    (0..4).filter(|b| mask & (1 << b) != 0).map(|b| {
        let (x0, y0, x1, y1) = cells[b];
        rect(x0, y0, x1, y1)
    }).collect()
}

// ---------- Powerline ----------

fn powerline(i: u32, w: f32, h: f32) -> Vec<Shape> {
    let thin = i % 2 == 1;
    let right = i < 2 || (4..6).contains(&i);
    let round = i >= 4;
    let (x_base, x_tip) = if right { (0., w) } else { (w, 0.) };
    let cmds = if round {
        half_circle(x_base, x_tip, h)
    } else {
        vec![Cmd::Move(x_base, 0.), Cmd::Line(x_tip, h / 2.), Cmd::Line(x_base, h)]
    };
    if thin {
        vec![Shape::Path { cmds, stroke: Some(light(w)) }]
    } else {
        let mut cmds = cmds;
        cmds.push(Cmd::Close);
        vec![Shape::Path { cmds, stroke: None }]
    }
}

/// Halbkreis (Ellipse) von der Basis oben über die Spitze in der Mitte bis zur Basis unten.
fn half_circle(x_base: f32, x_tip: f32, h: f32) -> Vec<Cmd> {
    let (my, dx) = (h / 2., x_tip - x_base);
    vec![
        Cmd::Move(x_base, 0.),
        Cmd::Cubic { c1: (x_base + dx * KAPPA, 0.), c2: (x_tip, my - my * KAPPA), to: (x_tip, my) },
        Cmd::Cubic { c1: (x_tip, my + my * KAPPA), c2: (x_base + dx * KAPPA, h), to: (x_base, h) },
    ]
}

// ---------- Linien ----------

/// Dicke einer leichten Linie in Pixeln, wächst mit der Zellbreite.
fn light(w: f32) -> f32 {
    (w / 7.).round().max(1.)
}

/// Pixel unterhalb/oberhalb der Mitte, damit gerade und ungerade Dicken zeilenscharf liegen.
fn lo(t: f32) -> f32 {
    (t / 2.).floor()
}

/// Tabelle der Arme (links, rechts, oben, unten) für U+2500..U+257F.
/// 0 = keiner, 1 = leicht, 2 = fett, 3 = doppelt. Striche, Bögen und Diagonalen stehen als "0000" drin.
#[rustfmt::skip]
const ARMS: [&str; 0x80] = [
    "1100", "2200", "0011", "0022", "0000", "0000", "0000", "0000", "0000", "0000", "0000", "0000", "0101", "0201", "0102", "0202",
    "1001", "2001", "1002", "2002", "0110", "0210", "0120", "0220", "1010", "2010", "1020", "2020", "0111", "0211", "0121", "0112",
    "0122", "0221", "0212", "0222", "1011", "2011", "1021", "1012", "1022", "2021", "2012", "2022", "1101", "2101", "1201", "2201",
    "1102", "2102", "1202", "2202", "1110", "2110", "1210", "2210", "1120", "2120", "1220", "2220", "1111", "2111", "1211", "2211",
    "1121", "1112", "1122", "2121", "1221", "2112", "1212", "2221", "2212", "2122", "1222", "2222", "0000", "0000", "0000", "0000",
    "3300", "0033", "0301", "0103", "0303", "3001", "1003", "3003", "0310", "0130", "0330", "3010", "1030", "3030", "0311", "0133",
    "0333", "3011", "1033", "3033", "3301", "1103", "3303", "3310", "1130", "3330", "3311", "1133", "3333", "0000", "0000", "0000",
    "0000", "0000", "0000", "0000", "1000", "0010", "0100", "0001", "2000", "0020", "0200", "0002", "1200", "0012", "2100", "0021",
];

fn lines(i: u32, w: f32, h: f32) -> Vec<Shape> {
    let lt = light(w);
    match i {
        0x04..=0x0B | 0x4C..=0x4F => dashes(i, w, h, lt),
        0x6D..=0x70 => arc(i, w, h, lt),
        0x71..=0x73 => diagonals(i, w, h, lt),
        // Die Tabelle hat 0x80 Einträge, die Auswahl oben ist ein Teilbereich davon.
        0x00..=0x7F => arms(ARMS[i as usize], w, h, lt),
        _ => Vec::new(),
    }
}

/// Gestrichelte Linien: n Striche pro Zelle, an den Zellrändern je ein halber Zwischenraum.
fn dashes(i: u32, w: f32, h: f32, lt: f32) -> Vec<Shape> {
    let (n, heavy, vertical) = match i {
        0x04..=0x0B => (if i < 0x08 { 3. } else { 4. }, i % 2 == 1, (i / 2) % 2 == 1),
        _ => (2., i % 2 == 1, i >= 0x4E),
    };
    let t = if heavy { lt * 2. } else { lt };
    let len = if vertical { h } else { w };
    let seg = len / n;
    let dash = (seg * 0.6).max(1.);
    let (cx, cy) = ((w / 2.).round(), (h / 2.).round());
    (0..n as usize).map(|k| {
        let a = seg * k as f32 + (seg - dash) / 2.;
        if vertical {
            rect(cx - lo(t), a, cx - lo(t) + t, a + dash)
        } else {
            rect(a, cy - lo(t), a + dash, cy - lo(t) + t)
        }
    }).collect()
}

/// Abgerundete Ecken ╭ ╮ ╯ ╰: ein Viertelkreis zwischen Zellmitte-Kante und Zellmitte-Kante.
fn arc(i: u32, w: f32, h: f32, lt: f32) -> Vec<Shape> {
    let (cx, cy) = ((w / 2.).round(), (h / 2.).round());
    let r = cx.min(cy);
    // (rechts?, unten?) je Zeichen: ╭ rechts+unten, ╮ links+unten, ╯ links+oben, ╰ rechts+oben
    let (right, down) = match i {
        0x6D => (true, true),
        0x6E => (false, true),
        0x6F => (false, false),
        _ => (true, false),
    };
    let sx = if right { 1. } else { -1. };
    let sy = if down { 1. } else { -1. };
    let edge_x = if right { w } else { 0. };
    let edge_y = if down { h } else { 0. };
    // Start auf der waagerechten Kante, Ende auf der senkrechten, Radius r um den Mittelpunkt (cx + sx r, cy + sy r).
    let start = (cx + sx * r, cy);
    let end = (cx, cy + sy * r);
    let cmds = vec![
        Cmd::Move(edge_x, cy),
        Cmd::Line(start.0, start.1),
        Cmd::Cubic {
            c1: (start.0 - sx * r * KAPPA, start.1),
            c2: (end.0, end.1 - sy * r * KAPPA),
            to: end,
        },
        Cmd::Line(cx, edge_y),
    ];
    vec![Shape::Path { cmds, stroke: Some(lt) }]
}

fn diagonals(i: u32, w: f32, h: f32, lt: f32) -> Vec<Shape> {
    let up = vec![Cmd::Move(w, 0.), Cmd::Line(0., h)];
    let down = vec![Cmd::Move(0., 0.), Cmd::Line(w, h)];
    let mut out = Vec::new();
    if i != 0x72 {
        out.push(Shape::Path { cmds: up, stroke: Some(lt) });
    }
    if i != 0x71 {
        out.push(Shape::Path { cmds: down, stroke: Some(lt) });
    }
    out
}

#[derive(Clone, Copy)]
struct Arms {
    l: u8,
    r: u8,
    u: u8,
    d: u8,
}

impl Arms {
    fn parse(s: &str) -> Self {
        let b: Vec<u8> = s.bytes().map(|c| c - b'0').collect();
        Self { l: b[0], r: b[1], u: b[2], d: b[3] }
    }
}

fn arms(spec: &str, w: f32, h: f32, lt: f32) -> Vec<Shape> {
    let a = Arms::parse(spec);
    let g = Grid::new(w, h, lt);
    let mut out = Vec::new();
    g.horizontal(&a, &mut out);
    g.vertical(&a, &mut out);
    merge_collinear(out)
}

type R = (f32, f32, f32, f32);

/// Vereinigt `b` in `a`, wenn beide dieselbe Spanne haben und sich in der anderen Achse berühren oder überlappen.
fn union(a: &mut R, b: R) -> bool {
    if a.1 == b.1 && a.3 == b.3 && b.0 <= a.2 && b.2 >= a.0 {
        (a.0, a.2) = (a.0.min(b.0), a.2.max(b.2));
        true
    } else if a.0 == b.0 && a.2 == b.2 && b.1 <= a.3 && b.3 >= a.1 {
        (a.1, a.3) = (a.1.min(b.1), a.3.max(b.3));
        true
    } else {
        false
    }
}

/// Fasst die Arme einer durchgehenden Linie zusammen (linker plus rechter Arm), damit keine Naht entsteht.
fn merge_collinear(shapes: Vec<Shape>) -> Vec<Shape> {
    let mut rects: Vec<R> = Vec::new();
    for s in shapes {
        if let Shape::Rect { x0, y0, x1, y1, .. } = s {
            let b = (x0, y0, x1, y1);
            if !rects.iter_mut().any(|r| union(r, b)) {
                rects.push(b);
            }
        }
    }
    rects.into_iter().map(|(x0, y0, x1, y1)| rect(x0, y0, x1, y1)).collect()
}

/// Gemeinsame Maße einer Zelle für Linien mit Armen.
struct Grid {
    w: f32,
    h: f32,
    lt: f32,
    cx: f32,
    cy: f32,
}

impl Grid {
    fn new(w: f32, h: f32, lt: f32) -> Self {
        Self { w, h, lt, cx: (w / 2.).round(), cy: (h / 2.).round() }
    }

    fn thickness(&self, style: u8) -> f32 {
        if style == 2 { self.lt * 2. } else { self.lt }
    }

    /// Mitte der Linie j (0 = erste, 1 = zweite) eines Doppelstrichs um `c`: [a0, a1], [b0, b1].
    fn double(&self, c: f32) -> ((f32, f32), (f32, f32)) {
        let a0 = c - self.lt - lo(self.lt);
        ((a0, a0 + self.lt), (a0 + 2. * self.lt, a0 + 3. * self.lt))
    }

    /// Streckenende hin zur Mitte für einfache Arme (Stil 1 oder 2) einer Achse.
    /// `perp`: Stile der beiden Gegenarme, `through`: der Gegenarm in Gegenrichtung existiert.
    fn single_reach(&self, perp: (u8, u8), through: bool, toward_low: bool, c: f32) -> f32 {
        let (p0, p1) = perp;
        if p0 == 3 || p1 == 3 {
            // Doppelte Gegenarme: bis an die innere Kante der nächsten Linie, bei Durchgang bis zur Mitte.
            if through {
                return c;
            }
            let (a, b) = self.double(c);
            return if toward_low { a.1 } else { b.0 };
        }
        let t = self.thickness(p0).max(self.thickness(p1)) * ((p0 != 0 || p1 != 0) as u8 as f32);
        if toward_low { c + (t - lo(t)) } else { c - lo(t) }
    }

    fn horizontal(&self, a: &Arms, out: &mut Vec<Shape>) {
        for (style, left) in [(a.l, true), (a.r, false)] {
            match style {
                0 => {}
                3 => self.double_h(a, left, out),
                s => self.single_h(a, s, left, out),
            }
        }
    }

    fn single_h(&self, a: &Arms, style: u8, left: bool, out: &mut Vec<Shape>) {
        let t = self.thickness(style);
        let through = if left { a.r != 0 } else { a.l != 0 };
        let reach = self.single_reach((a.u, a.d), through, left, self.cx);
        let (x0, x1) = if left { (0., reach) } else { (reach, self.w) };
        out.push(rect(x0, self.cy - lo(t), x1, self.cy - lo(t) + t));
    }

    fn double_h(&self, a: &Arms, left: bool, out: &mut Vec<Shape>) {
        let (va, vb) = self.double(self.cx);
        let (ha, hb) = self.double(self.cy);
        for (j, (y0, y1)) in [ha, hb].into_iter().enumerate() {
            let facing = if j == 0 { a.u } else { a.d } == 3;
            let opposite = if j == 0 { a.d } else { a.u } == 3;
            let reach = match (facing, opposite, left) {
                (true, _, true) => va.1,
                (true, _, false) => vb.0,
                (false, true, true) => vb.1,
                (false, true, false) => va.0,
                _ => self.cx,
            };
            let (x0, x1) = if left { (0., reach) } else { (reach, self.w) };
            out.push(rect(x0, y0, x1, y1));
        }
    }

    fn vertical(&self, a: &Arms, out: &mut Vec<Shape>) {
        for (style, up) in [(a.u, true), (a.d, false)] {
            match style {
                0 => {}
                3 => self.double_v(a, up, out),
                s => self.single_v(a, s, up, out),
            }
        }
    }

    fn single_v(&self, a: &Arms, style: u8, up: bool, out: &mut Vec<Shape>) {
        let t = self.thickness(style);
        let through = if up { a.d != 0 } else { a.u != 0 };
        let reach = self.single_reach((a.l, a.r), through, up, self.cy);
        let (y0, y1) = if up { (0., reach) } else { (reach, self.h) };
        out.push(rect(self.cx - lo(t), y0, self.cx - lo(t) + t, y1));
    }

    fn double_v(&self, a: &Arms, up: bool, out: &mut Vec<Shape>) {
        let (va, vb) = self.double(self.cx);
        let (ha, hb) = self.double(self.cy);
        for (i, (x0, x1)) in [va, vb].into_iter().enumerate() {
            let facing = if i == 0 { a.l } else { a.r } == 3;
            let opposite = if i == 0 { a.r } else { a.l } == 3;
            let reach = match (facing, opposite, up) {
                (true, _, true) => ha.1,
                (true, _, false) => hb.0,
                (false, true, true) => hb.1,
                (false, true, false) => ha.0,
                _ => self.cy,
            };
            let (y0, y1) = if up { (0., reach) } else { (reach, self.h) };
            out.push(rect(x0, y0, x1, y1));
        }
    }
}

// ---------- Zeichnen ----------

fn pt(origin: Point<Pixels>, x: f32, y: f32) -> Point<Pixels> {
    point(origin.x + px(x), origin.y + px(y))
}

/// Zeichnet die Formen eines Zeichens in die Zelle mit der linken oberen Ecke `origin`.
pub fn paint(window: &mut Window, shapes: &[Shape], origin: Point<Pixels>, color: Hsla) {
    for shape in shapes {
        match shape {
            Shape::Rect { x0, y0, x1, y1, alpha } => {
                let b = Bounds::new(pt(origin, *x0, *y0), size(px(x1 - x0), px(y1 - y0)));
                window.paint_quad(fill(b, color.opacity(*alpha)));
            }
            Shape::Path { cmds, stroke } => paint_path(window, cmds, *stroke, origin, color),
        }
    }
}

fn paint_path(window: &mut Window, cmds: &[Cmd], stroke: Option<f32>, origin: Point<Pixels>, color: Hsla) {
    let mut builder = match stroke {
        Some(width) => PathBuilder::stroke(px(width)),
        None => PathBuilder::fill(),
    };
    for cmd in cmds {
        match *cmd {
            Cmd::Move(x, y) => builder.move_to(pt(origin, x, y)),
            Cmd::Line(x, y) => builder.line_to(pt(origin, x, y)),
            Cmd::Cubic { c1, c2, to } => {
                builder.cubic_bezier_to(pt(origin, to.0, to.1), pt(origin, c1.0, c1.1), pt(origin, c2.0, c2.1))
            }
            Cmd::Close => builder.close(),
        }
    }
    if let Ok(path) = builder.build() {
        window.paint_path(path, color);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: f32 = 8.;
    const H: f32 = 16.;

    fn rects(c: char) -> Vec<(f32, f32, f32, f32)> {
        shapes(c, W, H)
            .into_iter()
            .filter_map(|s| match s {
                Shape::Rect { x0, y0, x1, y1, .. } => Some((x0, y0, x1, y1)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn voller_block_fuellt_die_zelle() {
        assert_eq!(rects('█'), vec![(0., 0., W, H)]);
    }

    #[test]
    fn halbbloecke_und_achtel() {
        assert_eq!(rects('▀'), vec![(0., 0., W, H / 2.)]);
        assert_eq!(rects('▄'), vec![(0., H / 2., W, H)]);
        assert_eq!(rects('▌'), vec![(0., 0., W / 2., H)]);
        assert_eq!(rects('▐'), vec![(W / 2., 0., W, H)]);
        assert_eq!(rects('▁'), vec![(0., H - H / 8., W, H)]);
        assert_eq!(rects('▉'), vec![(0., 0., W * 7. / 8., H)]);
    }

    #[test]
    fn schattierungen_sind_alpha() {
        let alpha = |c| match shapes(c, W, H)[0] {
            Shape::Rect { alpha, x0, y1, .. } => {
                assert_eq!((x0, y1), (0., H));
                alpha
            }
            _ => panic!("kein Rechteck"),
        };
        assert_eq!((alpha('░'), alpha('▒'), alpha('▓')), (0.25, 0.5, 0.75));
    }

    #[test]
    fn viertelbloecke() {
        assert_eq!(rects('▘'), vec![(0., 0., W / 2., H / 2.)]);
        assert_eq!(rects('▟').len(), 3);
        assert_eq!(rects('▚').len(), 2);
    }

    #[test]
    fn waagerechte_linie_ist_mittig_und_ueber_die_ganze_breite() {
        let r = rects('─');
        assert_eq!(r.len(), 1);
        let (x0, y0, x1, y1) = r[0];
        assert_eq!((x0, x1), (0., W));
        assert!(((y0 + y1) / 2. - H / 2.).abs() <= 1., "Linie liegt mittig");
        assert!(y1 - y0 >= 1.);
    }

    #[test]
    fn senkrechte_linie_geht_von_oben_nach_unten() {
        let (x0, y0, x1, y1) = rects('│')[0];
        assert_eq!((y0, y1), (0., H));
        assert!((x0 + x1) / 2. - W / 2. <= 1.);
    }

    #[test]
    fn ecke_verbindet_rechts_und_unten() {
        // ┌: waagerecht von der Mitte bis zum rechten Rand, senkrecht von der Mitte bis zum unteren Rand.
        let r = rects('┌');
        assert_eq!(r.len(), 2);
        let horizontal = r.iter().find(|s| s.3 - s.1 < s.2 - s.0).expect("waagerechter Arm");
        let vertical = r.iter().find(|s| s.2 - s.0 < s.3 - s.1).expect("senkrechter Arm");
        assert_eq!(horizontal.2, W);
        assert_eq!(vertical.3, H);
        assert!(horizontal.0 <= W / 2. && horizontal.0 > 0.);
        assert!(vertical.1 <= H / 2. && vertical.1 > 0.);
    }

    #[test]
    fn rundung_beginnt_am_rechten_und_endet_am_unteren_rand() {
        let s = shapes('╭', W, H);
        let Shape::Path { cmds, stroke } = &s[0] else { panic!("kein Pfad") };
        assert!(stroke.is_some());
        assert_eq!(cmds.first(), Some(&Cmd::Move(W, (H / 2.).round())));
        assert_eq!(cmds.last(), Some(&Cmd::Line((W / 2.).round(), H)));
    }

    #[test]
    fn doppelstrich_hat_zwei_linien_mit_luecke() {
        let r = rects('═');
        assert_eq!(r.len(), 2);
        assert_eq!((r[0].0, r[0].2), (0., W));
        assert!(r[1].1 > r[0].3, "zwischen den Linien bleibt eine Lücke");
    }

    #[test]
    fn doppelte_ecke_hat_aussen_und_innenlinie() {
        // ╔: vier Rechtecke, zwei waagerecht nach rechts, zwei senkrecht nach unten.
        assert_eq!(rects('╔').len(), 4);
        assert_eq!(rects('╬').len(), 8);
    }

    #[test]
    fn powerline_pfeil_fuellt_die_hoehe() {
        let s = shapes('\u{E0B0}', W, H);
        let Shape::Path { cmds, stroke } = &s[0] else { panic!("kein Pfad") };
        assert!(stroke.is_none());
        assert_eq!(cmds[0], Cmd::Move(0., 0.));
        assert_eq!(cmds[1], Cmd::Line(W, H / 2.));
        assert_eq!(cmds[2], Cmd::Line(0., H));
    }

    #[test]
    fn powerline_duenn_ist_ein_strich() {
        let s = shapes('\u{E0B1}', W, H);
        assert!(matches!(&s[0], Shape::Path { stroke: Some(_), .. }));
        let s = shapes('\u{E0B6}', W, H);
        assert!(matches!(&s[0], Shape::Path { stroke: None, .. }));
    }

    #[test]
    fn jedes_zeichen_im_bereich_liefert_formen() {
        for c in ('\u{2500}'..='\u{259F}').chain('\u{E0B0}'..='\u{E0B7}') {
            assert!(is_builtin(c));
            assert!(!shapes(c, W, H).is_empty(), "{c:?} liefert nichts");
        }
        assert!(!is_builtin('a'));
        assert!(shapes('a', W, H).is_empty());
    }
}
