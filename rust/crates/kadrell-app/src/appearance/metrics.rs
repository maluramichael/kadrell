//! Maße der Darstellung ohne GPUI: UI-Größe (`Theme.scale` der Mac-App), die Zeilenmaße der beiden Baum-Designs
//! (`ClassicSidebarRenderer`, `CompactSidebarRenderer`, gemeinsame Bausteine aus `SidebarRenderer`) und die Aufteilung
//! des Baums in Zeilen. Alles rein rechnerisch, damit es sich wie die Swift-Tests ohne Fenster prüfen lässt.

use kadrell_core::settings::SidebarStyle;

use crate::ui::layout::Rect;

/// Erlaubter Bereich der UI-Größe als Faktor (`Settings.uiScalePercent` der Mac-App: 50 bis 200 %).
pub const SCALE_RANGE: (f32, f32) = (0.5, 2.0);

/// UI-Größe aus der Einstellung, auf den erlaubten Bereich begrenzt. Unlesbares wird zu 1.
pub fn clamp_scale(v: f64) -> f32 {
    if v.is_finite() { (v as f32).clamp(SCALE_RANGE.0, SCALE_RANGE.1) } else { 1. }
}

/// Logische Punkte mal UI-Größe (`CGRect.scaled`, `.ui`).
pub fn scaled(v: f32, scale: f32) -> f32 {
    v * scale
}

/// Höhe der Leiste: 30 pt, auf ganze Punkte gerundet (`Theme.barHeight`).
pub fn bar_height(scale: f32) -> f32 {
    (30. * scale).round()
}

/// Radius der runden Ecke oben links an der Arbeitsfläche, auf ganze Punkte gerundet (`ModuleCornerView.scaledRadius`):
/// sonst hat die Fläche Bruchteile von Pixeln und zeichnet sich als Viereck ab.
pub fn corner_radius(scale: f32) -> f32 {
    (14. * scale).round()
}

/// Breite der Seitenleiste in Punkten bei dieser UI-Größe.
pub fn sidebar_width(scale: f32) -> f32 {
    (260. * scale).round()
}

/// Zeilenmaße eines Designs (`groupRow`, `sessionRow`, `groupGap`, `topInset` der Renderer), unskaliert.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RowMetrics {
    pub group_row: f32,
    /// Kopfzeile einer Session, die Nachrichtenzeile kommt bei Bedarf dazu.
    pub session_row: f32,
    /// Abstand vor jeder Gruppe außer der ersten.
    pub group_gap: f32,
    /// Abstand über der ersten Gruppe.
    pub top_inset: f32,
    /// Klassisch: Pfad unter dem Gruppennamen. Kompakt: kein Pfad, eingeklappt ein Punkt je Session im Kopf, Fuge
    /// zwischen den Gruppen.
    pub path: bool,
}

impl RowMetrics {
    pub const CLASSIC: RowMetrics = RowMetrics { group_row: 38., session_row: 24., group_gap: 6., top_inset: 6., path: true };
    pub const COMPACT: RowMetrics = RowMetrics { group_row: 24., session_row: 22., group_gap: 0., top_inset: 4., path: false };

    pub fn of(style: SidebarStyle) -> RowMetrics {
        match style {
            SidebarStyle::Classic => Self::CLASSIC,
            SidebarStyle::Compact => Self::COMPACT,
        }
    }

    /// Kopfhöhe der Gruppenzeile: bei Klassisch ohne die Pfadzeile darunter (`headRect`).
    pub fn group_head(&self) -> f32 {
        if self.path { self.session_row } else { self.group_row }
    }

    /// Höhe einer Session-Zeile, mit Nachrichtenzeile (Einstellung „letzte Nachricht zeigen“) um `MESSAGE_LINE` höher.
    pub fn session_height(&self, message: bool) -> f32 {
        self.session_row + if message { MESSAGE_LINE } else { 0. }
    }
}

/// Höhe der Nachrichtenzeile unter der Kopfzeile einer Session. 2 pt mehr als die Schrift braucht: mit Tastaturring
/// rückt der Umriss unten 2 pt ein und bliebe sonst an den Unterlängen.
pub const MESSAGE_LINE: f32 = 16.;
/// Rand der Zeilenfläche in der Zeile (`Instrument.rowInset`).
pub const ROW_INSET: (f32, f32) = (6., 1.);
/// Statuspunkt: Mitte 16 pt vom linken Rand, Sessions sind nicht eingerückt (`dotRect`).
pub const DOT_X: f32 = 12.;
pub const DOT_SIZE: f32 = 8.;
/// Linke Kante von Titel und Nachrichtenzeile einer Session ohne Typ-Symbol.
pub const SESSION_TEXT_X: f32 = 26.;
/// Linke Kante des Gruppen-Labels und der Pfadzeile.
pub const GROUP_LABEL_X: f32 = 37.;
pub const GROUP_PATH_X: f32 = 42.;
/// Rand rechts in Kopfzeilen (Laufzeit, Favorit).
pub const RIGHT_INSET: f32 = 10.;

/// Fläche einer hervorgehobenen Zeile: eingerückt (`rowShape`).
pub fn row_shape(row: Rect) -> Rect {
    inset(row, ROW_INSET.0, ROW_INSET.1)
}

/// Außenkante des Tastaturrings: um die Zeilenfläche, innerhalb der Zeile (`keyFocusRing`).
pub fn key_focus_ring(row: Rect) -> Rect {
    inset(row_shape(row), -2.5, -0.5)
}

/// Fläche einer Session. Mit Tastaturring rückt sie oben und unten 2 pt ein: in der Zeile ist darüber nur 1 pt frei,
/// so liegen Ring, Luft und Umriss ringsum gleich breit und der Ring bleibt in der Zeile (`sessionShape`).
pub fn session_shape(focused: bool, key_focus: bool, row: Rect) -> Rect {
    if focused && key_focus { inset(key_focus_ring(row), 2.5, 2.5) } else { row_shape(row) }
}

/// Kopfzeile einer Zeile (`headRect`): volle Breite, Höhe `head`.
pub fn head_rect(row: Rect, head: f32) -> Rect {
    Rect::new(row.x, row.y, row.w, head)
}

/// Stelle des Statuspunkts (und des atmenden Punkts) in der Kopfzeile (`dotRect`).
pub fn dot_rect(row: Rect, head: f32) -> Rect {
    let h = head_rect(row, head);
    Rect::new(DOT_X, h.y + h.h / 2. - DOT_SIZE / 2., DOT_SIZE, DOT_SIZE)
}

fn inset(r: Rect, dx: f32, dy: f32) -> Rect {
    Rect::new(r.x + dx, r.y + dy, (r.w - 2. * dx).max(0.), (r.h - 2. * dy).max(0.))
}

/// Eine Zeile des Baums.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RowKind {
    Group(usize),
    Session(usize, usize),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Row {
    pub kind: RowKind,
    pub y: f32,
    pub h: f32,
}

/// Form einer Gruppe für die Zeilenaufteilung: Zahl der Sessions und ob sie aufgeklappt ist.
#[derive(Clone, Copy, Debug)]
pub struct GroupShape {
    pub sessions: usize,
    pub open: bool,
}

/// Zeilen des Baums von oben nach unten (unskaliert): `top_inset`, dann je Gruppe `group_gap` (außer vor der ersten),
/// Kopfzeile und, aufgeklappt, die Sessions. Gibt die Zeilen und die Gesamthöhe zurück. `message` sagt je Gruppe und
/// Session, ob eine Nachrichtenzeile dazukommt.
pub fn tree_rows(metrics: RowMetrics, groups: &[GroupShape], message: &dyn Fn(usize, usize) -> bool) -> (Vec<Row>, f32) {
    let mut rows = Vec::new();
    let mut y = metrics.top_inset;
    for (g, group) in groups.iter().enumerate() {
        if g > 0 {
            y += metrics.group_gap;
        }
        rows.push(Row { kind: RowKind::Group(g), y, h: metrics.group_row });
        y += metrics.group_row;
        if !group.open {
            continue;
        }
        for s in 0..group.sessions {
            let h = metrics.session_height(message(g, s));
            rows.push(Row { kind: RowKind::Session(g, s), y, h });
            y += h;
        }
    }
    (rows, y)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROW: Rect = Rect { x: 0., y: 0., w: 200., h: 24. };

    #[test]
    fn scale_is_clamped_and_unreadable_values_fall_back() {
        assert_eq!(clamp_scale(1.3), 1.3);
        assert_eq!(clamp_scale(0.1), 0.5);
        assert_eq!(clamp_scale(9.), 2.);
        assert_eq!(clamp_scale(f64::NAN), 1.);
    }

    #[test]
    fn scaled_sizes_are_whole_points_where_the_mac_app_rounds() {
        assert_eq!(bar_height(1.), 30.);
        assert_eq!(bar_height(1.3), 39.);
        assert_eq!(bar_height(0.9), 27.);
        assert_eq!(corner_radius(1.), 14.);
        assert_eq!(corner_radius(1.3), 18.);
        assert_eq!(corner_radius(0.9), 13.);
        assert_eq!(sidebar_width(1.3), 338.);
        assert_eq!(scaled(8., 1.3), 8. * 1.3);
    }

    #[test]
    fn designs_have_the_swift_metrics() {
        let c = RowMetrics::of(SidebarStyle::Classic);
        assert_eq!((c.group_row, c.session_row, c.group_gap, c.top_inset, c.path), (38., 24., 6., 6., true));
        let k = RowMetrics::of(SidebarStyle::Compact);
        assert_eq!((k.group_row, k.session_row, k.group_gap, k.top_inset, k.path), (24., 22., 0., 4., false));
    }

    /// `testClassicKeepsPathLineBelowHead`: die Kopfzeile bleibt so hoch wie eine Session, der Pfad steht darunter.
    #[test]
    fn classic_keeps_the_path_line_below_the_head() {
        let c = RowMetrics::CLASSIC;
        assert_eq!(c.group_head(), 24.);
        assert!(c.group_row > c.group_head());
        assert_eq!(RowMetrics::COMPACT.group_head(), 24.);
    }

    /// `testSessionRowIsNotIndented`: Punkt unter dem Chevron, Mitte bei x = 16.
    #[test]
    fn session_row_is_not_indented() {
        let dot = dot_rect(Rect::new(0., 0., 200., 22.), RowMetrics::COMPACT.session_row);
        assert_eq!(dot.x + dot.w / 2., 16.);
        assert_eq!(dot.y + dot.h / 2., 11.);
        assert!(dot.x + dot.w < SESSION_TEXT_X);
    }

    /// `testKeyFocusRingOutsideOutline`: Ring, Luft und Umriss liegen ringsum gleich breit, der Ring bleibt in der Zeile.
    #[test]
    fn key_focus_ring_stays_inside_the_row_and_outside_the_outline() {
        let row = Rect::new(0., 10., 200., 24.);
        let ring = key_focus_ring(row);
        assert!(ring.y >= row.y && ring.max_y() <= row.max_y(), "Ring oben und unten in der Zeile");
        assert!(ring.x >= row.x && ring.max_x() <= row.max_x());
        let shape = session_shape(true, true, row);
        assert_eq!((shape.x - ring.x, shape.y - ring.y), (2.5, 2.5), "Luft und Umriss gleich breit");
        assert_eq!((ring.max_x() - shape.max_x(), ring.max_y() - shape.max_y()), (2.5, 2.5));
    }

    #[test]
    fn focus_without_key_focus_has_no_ring() {
        assert_eq!(session_shape(true, false, ROW), row_shape(ROW));
        assert_eq!(session_shape(false, true, ROW), row_shape(ROW));
        assert_eq!(row_shape(ROW), Rect::new(6., 1., 188., 22.));
    }

    /// `testUnreadPillStaysInsideKeyFocusOutline`: bei 22 pt (kompakt) bleibt für die Marke weniger als 16 pt Höhe.
    #[test]
    fn compact_row_with_key_focus_leaves_a_flat_pill() {
        let row = Rect::new(0., 10., 200., RowMetrics::COMPACT.session_row);
        let shape = session_shape(true, true, row);
        assert_eq!((shape.y, shape.max_y()), (13., 29.));
        let mid = row.y + row.h / 2.;
        let pill_h = 16f32.min(2. * (mid - shape.y - 2.));
        assert!(pill_h < 16. && pill_h > 8.);
    }

    /// `testMessageDescendersClearKeyFocusOutline`: auch mit Nachricht endet der Umriss 3 pt über der Zeilenkante.
    #[test]
    fn message_line_leaves_room_below_the_outline() {
        for m in [RowMetrics::CLASSIC, RowMetrics::COMPACT] {
            let h = m.session_height(true);
            assert_eq!(h, m.session_row + MESSAGE_LINE);
            let shape = session_shape(true, true, Rect::new(0., 0., 200., h));
            assert_eq!(h - shape.max_y(), 3.);
        }
    }

    #[test]
    fn tree_rows_stack_with_insets_and_gaps() {
        let groups = [GroupShape { sessions: 2, open: true }, GroupShape { sessions: 3, open: false }, GroupShape { sessions: 1, open: true }];
        let (rows, total) = tree_rows(RowMetrics::CLASSIC, &groups, &|_, _| false);
        let kinds: Vec<RowKind> = rows.iter().map(|r| r.kind).collect();
        assert_eq!(
            kinds,
            [RowKind::Group(0), RowKind::Session(0, 0), RowKind::Session(0, 1), RowKind::Group(1), RowKind::Group(2), RowKind::Session(2, 0)]
        );
        assert_eq!(rows[0].y, 6., "topInset");
        assert_eq!(rows[1].y, 6. + 38.);
        assert_eq!(rows[3].y, 6. + 38. + 48. + 6., "groupGap vor der zweiten Gruppe");
        assert_eq!(total, rows[5].y + 24.);
        let (compact, total) = tree_rows(RowMetrics::COMPACT, &groups, &|g, s| g == 0 && s == 1);
        assert_eq!(compact[0].y, 4.);
        assert_eq!(compact[2].h, 22. + MESSAGE_LINE);
        assert_eq!(compact[3].y, compact[2].y + compact[2].h, "kompakt ohne Abstand");
        assert_eq!(total, compact[5].y + 22.);
    }
}
