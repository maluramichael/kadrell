//! Platzverteilung der Statusleiste unten (`FooterBarView.swift`: `parts`, `width`, `fit`). Textbreiten kommen als
//! Zahlen von außen, damit die Rechnung ohne Schrift testbar bleibt.

use kadrell_ext::footer::{Align, FooterItem};

/// Ein Eintrag der Leiste mit der Extension, die ihn liefert.
pub type FooterEntry = (String, FooterItem);

/// Abstände in px (`FooterBarView`).
pub const HEIGHT: f32 = 22.;
pub const PAD: f32 = 10.;
pub const PART_GAP: f32 = 5.;
pub const SIDE_GAP: f32 = 16.;
pub const BAR_WIDTH: f32 = 3.;
pub const BAR_GAP: f32 = 1.;
pub const BAR_HEIGHT: f32 = 10.;
pub const GAUGE_WIDTH: f32 = 36.;
pub const GAUGE_HEIGHT: f32 = 6.;
/// Fuge zwischen zwei Modulen: dunkle und helle Haarlinie.
pub const SEAM: f32 = 2.;

/// Ein Teil eines Moduls, in der Reihenfolge, in der er gezeichnet wird.
#[derive(Debug, Clone, PartialEq)]
pub enum Part {
    /// Gedämpftes Label vorn.
    Label(String),
    Bars(Vec<f64>),
    Gauge(f64),
    /// Text in der Farbe des Eintrags.
    Text(String),
}

/// Label, Balken, Pegel, Text. Leere Texte zählen nicht.
pub fn parts(item: &FooterItem) -> Vec<Part> {
    let mut parts = Vec::new();
    if let Some(label) = item.label.as_ref().filter(|s| !s.is_empty()) {
        parts.push(Part::Label(label.clone()));
    }
    if !item.bars.is_empty() {
        parts.push(Part::Bars(item.bars.clone()));
    }
    if let Some(g) = item.gauge {
        parts.push(Part::Gauge(g));
    }
    if let Some(text) = item.text.as_ref().filter(|s| !s.is_empty()) {
        parts.push(Part::Text(text.clone()));
    }
    parts
}

/// Breite eines Teils; `text` misst einen Text.
pub fn part_width(part: &Part, text: &dyn Fn(&str) -> f32) -> f32 {
    match part {
        Part::Label(s) | Part::Text(s) => text(s).ceil(),
        Part::Bars(values) => values.len() as f32 * (BAR_WIDTH + BAR_GAP) - BAR_GAP,
        Part::Gauge(_) => GAUGE_WIDTH,
    }
}

/// Breite eines Moduls ohne Fuge: `PAD` je Seite, `PART_GAP` zwischen den Teilen.
pub fn module_width(item: &FooterItem, text: &dyn Fn(&str) -> f32) -> f32 {
    let parts = parts(item);
    let inner: f32 = parts.iter().map(|p| part_width(p, text)).sum();
    2. * PAD + inner + parts.len().saturating_sub(1) as f32 * PART_GAP
}

/// Wie viele Einträge Platz haben: rechts zählt vor links, links fallen die hinteren weg, rechts die vorderen.
/// Zwischen beiden Seiten bleiben mindestens `SIDE_GAP` frei. Liefert die Anzahl links (vorn) und rechts (hinten).
pub fn fit(left: &[f32], right: &[f32], width: f32) -> (usize, usize) {
    let tail = |n: usize| right[right.len() - n..].iter().sum::<f32>();
    let mut right_count = right.len();
    while right_count > 0 && tail(right_count) > width {
        right_count -= 1;
    }
    let room = width - tail(right_count) - if right_count > 0 { SIDE_GAP } else { 0. };
    let (mut left_count, mut used) = (0, 0.);
    while left_count < left.len() && used + left[left_count] <= room {
        used += left[left_count];
        left_count += 1;
    }
    (left_count, right_count)
}

/// Ein Eintrag mit Platz: Index in der Eintragsliste und Rahmen (x, Breite) in px, ohne die Fuge.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placed {
    pub index: usize,
    pub x: f32,
    pub width: f32,
}

/// Wo die Module bei `width` stehen: links von links in Listenreihenfolge, rechts bündig rechts, ebenfalls in
/// Listenreihenfolge. Wird es eng, fallen links die hinteren und rechts die vorderen weg.
pub fn layout(entries: &[FooterEntry], width: f32, text: &dyn Fn(&str) -> f32) -> (Vec<Placed>, Vec<Placed>) {
    let side = |align: Align| -> Vec<(usize, f32)> {
        entries.iter().enumerate().filter(|(_, e)| e.1.align == align).map(|(i, e)| (i, module_width(&e.1, text))).collect()
    };
    let (left, right) = (side(Align::Left), side(Align::Right));
    let with_seam = |v: &[(usize, f32)]| v.iter().map(|(_, w)| w + SEAM).collect::<Vec<f32>>();
    let (nl, nr) = fit(&with_seam(&left), &with_seam(&right), width);
    let mut x = 0.;
    let placed_left = left
        .iter()
        .take(nl)
        .map(|&(index, w)| {
            let p = Placed { index, x, width: w };
            x += w + SEAM;
            p
        })
        .collect();
    let mut rx = width;
    let mut placed_right: Vec<Placed> = right
        .iter()
        .skip(right.len() - nr)
        .rev()
        .map(|&(index, w)| {
            let p = Placed { index, x: rx - SEAM - w, width: w };
            rx = p.x;
            p
        })
        .collect();
    placed_right.reverse();
    (placed_left, placed_right)
}

/// Befehle für ⌘P `>`: jeder Eintrag der Statusleisten (oben und unten) mit Aktion, damit er ohne Maus auslösbar
/// ist (`extensionCommands`). Zuerst die oberen, dann die unteren. Liefert (Extension, was, Aktions-id).
pub fn commands(top: &[(String, kadrell_ext::panel::StatusItem)], bottom: &[FooterEntry]) -> Vec<(String, String, String)> {
    let top = top.iter().filter_map(|(name, item)| item.action.as_ref().map(|a| (name.clone(), item.text.clone(), a.clone())));
    let bottom = bottom.iter().filter_map(|(name, item)| item.action.as_ref().map(|a| (name.clone(), item.spoken(), a.clone())));
    top.chain(bottom).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use kadrell_ext::panel::StatusItem;

    fn text_w(s: &str) -> f32 {
        s.chars().count() as f32 * 6.
    }

    fn item(id: &str, align: Align) -> FooterItem {
        FooterItem { id: id.into(), align, text: Some("abc".into()), ..FooterItem::default() }
    }

    #[test]
    fn fit_prefers_right_and_drops_from_the_middle() {
        assert_eq!(fit(&[100., 100.], &[100.], 500.).0, 2);
        let tight = fit(&[100., 100., 100.], &[100., 100.], 450.);
        assert_eq!(tight.1, 2, "rechts zählt vor links");
        assert_eq!(tight.0, 2, "links fallen die hinteren weg, zwischen den Seiten bleiben 16 px");
        let narrow = fit(&[100.], &[100., 100., 100.], 250.);
        assert_eq!(narrow.1, 2, "rechts fallen die vorderen weg");
        assert_eq!(narrow.0, 0);
    }

    #[test]
    fn fit_handles_empty_sides() {
        assert_eq!(fit(&[], &[], 100.), (0, 0));
        assert_eq!(fit(&[50., 50.], &[], 100.), (2, 0), "ohne rechte Seite kein Mindestabstand");
        assert_eq!(fit(&[50., 50., 50.], &[], 100.), (2, 0));
    }

    #[test]
    fn module_width_adds_padding_parts_and_gaps() {
        let it = FooterItem {
            id: "cpu".into(),
            label: Some("CPU".into()),
            bars: vec![0.1, 0.2, 0.3, 0.4],
            gauge: Some(0.5),
            text: Some("23%".into()),
            ..FooterItem::default()
        };
        assert_eq!(parts(&it).len(), 4);
        // Label 18, Balken 4 * 4 - 1 = 15, Pegel 36, Text 18, drei Lücken je 5, Rand 2 * 10.
        assert_eq!(module_width(&it, &text_w), 20. + 18. + 15. + 36. + 18. + 15.);
        assert_eq!(module_width(&item("a", Align::Left), &text_w), 20. + 18.);
        let empty = FooterItem { id: "e".into(), text: Some(String::new()), ..FooterItem::default() };
        assert_eq!(module_width(&empty, &text_w), 20., "leere Texte zählen nicht");
    }

    #[test]
    fn layout_places_left_from_the_left_and_right_flush_right() {
        let entries: Vec<FooterEntry> = vec![
            ("x".into(), item("l1", Align::Left)),
            ("x".into(), item("r1", Align::Right)),
            ("x".into(), item("l2", Align::Left)),
            ("x".into(), item("r2", Align::Right)),
        ];
        let w = module_width(&entries[0].1, &text_w);
        let (left, right) = layout(&entries, 600., &text_w);
        assert_eq!(left.iter().map(|p| p.index).collect::<Vec<_>>(), [0, 2]);
        assert_eq!(left[0].x, 0.);
        assert_eq!(left[1].x, w + SEAM);
        assert_eq!(right.iter().map(|p| p.index).collect::<Vec<_>>(), [1, 3], "rechts in Listenreihenfolge");
        assert_eq!(right[1].x + right[1].width + SEAM, 600., "bündig rechts");
        assert_eq!(right[0].x + right[0].width + SEAM, right[1].x);
    }

    #[test]
    fn layout_drops_tail_on_the_left_and_front_on_the_right_without_overlap() {
        let entries: Vec<FooterEntry> = (0..3)
            .map(|i| ("x".into(), item(&format!("l{i}"), Align::Left)))
            .chain((0..3).map(|i| ("x".into(), item(&format!("r{i}"), Align::Right))))
            .collect();
        let each = module_width(&entries[0].1, &text_w) + SEAM;
        let (left, right) = layout(&entries, each * 5., &text_w);
        assert_eq!(right.iter().map(|p| p.index).collect::<Vec<_>>(), [3, 4, 5]);
        assert_eq!(left.iter().map(|p| p.index).collect::<Vec<_>>(), [0], "von drei linken passt einer neben die rechten");
        for l in &left {
            for r in &right {
                assert!(l.x + l.width + SEAM <= r.x, "keine Überlappung");
            }
        }
        let narrow = layout(&entries, each * 2. + 1., &text_w);
        assert_eq!(narrow.1.iter().map(|p| p.index).collect::<Vec<_>>(), [4, 5], "rechts fallen die vorderen weg");
        assert!(narrow.0.is_empty());
    }

    #[test]
    fn commands_list_actions_of_both_bars() {
        let top = vec![
            ("a".to_string(), StatusItem { text: "t=3".into(), color: None, action: Some("refresh".into()) }),
            ("b".to_string(), StatusItem { text: "stumm".into(), color: None, action: None }),
        ];
        let bottom: Vec<FooterEntry> = vec![
            ("c".into(), FooterItem { id: "cpu".into(), label: Some("CPU".into()), text: Some("5%".into()), action: Some("open".into()), ..FooterItem::default() }),
            ("c".into(), item("kein", Align::Left)),
        ];
        let list = commands(&top, &bottom);
        assert_eq!(
            list,
            [("a".to_string(), "t=3".to_string(), "refresh".to_string()), ("c".to_string(), "CPU 5%".to_string(), "open".to_string())]
        );
    }
}
