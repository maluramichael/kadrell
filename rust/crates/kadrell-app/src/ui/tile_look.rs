//! Regeln für das Aussehen einer Kachel ohne GPUI, Port von `CellView.swift` und `WorkspaceView.swift`: was der
//! Körper zeigt, welche Farbe und Breite der Rand hat, wie sich die Kopfzeile bei wenig Platz aufteilt, wo die
//! Symbole sitzen und wie rund die Ecke in der Arbeitsfläche ist. Alle Maße in logischen Punkten (vor UI-Größe).

use std::collections::HashSet;

use super::model::Status;

/// Schlüssel der Sessions, die die offene Suche als Treffer zeigt (`None`: keine Hervorhebung). Als Global, damit
/// Kacheln und Baum es beim Zeichnen lesen, ohne die Shell zu lesen; die Shell setzt es mit der Suche.
#[derive(Default)]
pub struct PaletteHits(pub Option<HashSet<String>>);

impl gpui::Global for PaletteHits {}

/// Treffer der Suche für diese Session: `None` ohne offene Hervorhebung, sonst ob sie Treffer ist.
pub fn palette_hit(cx: &gpui::App, key: &str) -> Option<bool> {
    cx.try_global::<PaletteHits>()?.0.as_ref().map(|keys| keys.contains(key))
}

/// Nicht getroffene Kacheln und Zeilen treten zurück, solange die Suche Treffer zeigt.
pub const DIMMED_OPACITY: f32 = 0.45;

/// Höhe von Kopfzeile und Stack-Zeile in logischen Punkten.
pub const ROW: f32 = 26.;
/// Radius der runden Ecke oben links an der Arbeitsfläche (`ModuleCornerView.radius`).
pub const MODULE_RADIUS: f32 = 14.;
/// Breite der Fuge (dunkel plus hell) am Rand der Modul-Ecke.
pub const MODULE_SEAM: f32 = 2.;

/// Höhe der Kopfzeile in Pixeln: `26 * Skalierung`, gerundet.
pub fn header_height(scale: f32) -> f32 {
    (ROW * scale).round()
}

/// Radius der Modul-Ecke in Pixeln, einzige Quelle für Ecke und Kachel darunter.
pub fn module_radius(scale: f32) -> f32 {
    (MODULE_RADIUS * scale).round()
}

/// Radius der Kachel oder Stack-Zeile genau in der Ecke oben links: konzentrisch zum Bogen der Modul-Ecke, die 2 pt weiter
/// außen auf den Fugen sitzt. Alle anderen bleiben eckig.
pub fn corner_radius(origin: (f32, f32), gap: f32, scale: f32) -> f32 {
    let at_corner = (origin.0 - gap).abs() < 0.5 && (origin.1 - gap).abs() < 0.5;
    if at_corner { (module_radius(scale) - gap).max(0.) } else { 0. }
}

/// Was der Körper einer Kachel zeigt, in der Rangfolge von `CellView.drawBody`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BodyKind {
    /// Der Ordner der Session fehlt.
    MissingFolder,
    /// Startfehler: Prozess kurz nach dem Start mit diesem Code gestorben.
    StartFailed(i32),
    /// Beendet oder gestoppt: Schraffur, letzte Zeilen, „Klick setzt fort“.
    Ended,
    /// Vorschau (⌥J/⌥K) einer nie gestarteten Session.
    Preview,
    /// Der Prozess startet gerade.
    Starting,
    /// Das Terminal hängt in einem anderen Fenster.
    Elsewhere,
    /// Prozess läuft, aber kein Terminal in dieser Kachel: letzte Zeilen.
    Lines,
    /// Das Terminal füllt den Körper.
    Terminal,
}

/// Was die Kachel über ihren Prozess weiß.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Run {
    pub missing_folder: bool,
    pub exit_code: Option<i32>,
    pub ended: bool,
    /// Ein Prozess hängt an der Session (Terminal existiert, in welchem Fenster auch immer).
    pub attached: bool,
    pub previewing: bool,
    pub elsewhere: bool,
}

pub fn body_kind(r: Run) -> BodyKind {
    if r.missing_folder {
        BodyKind::MissingFolder
    } else if let Some(code) = r.exit_code {
        BodyKind::StartFailed(code)
    } else if r.ended {
        BodyKind::Ended
    } else if !r.attached && r.previewing {
        BodyKind::Preview
    } else if !r.attached {
        BodyKind::Starting
    } else if r.elsewhere {
        BodyKind::Elsewhere
    } else {
        BodyKind::Terminal
    }
}

/// `Lines` statt `Terminal`, wenn kein Terminal in der Kachel hängt, obwohl ein Prozess läuft.
pub fn body_kind_mounted(r: Run, mounted: bool) -> BodyKind {
    match body_kind(r) {
        BodyKind::Terminal if !mounted => BodyKind::Lines,
        kind => kind,
    }
}

/// Farbe des Rands in der Rangfolge von `CellView.borderColor`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    /// Ziel beim Ziehen einer anderen Kachel: Vordergrundfarbe.
    Drop,
    /// Fehlender Ordner oder Startfehler: Fehlerfarbe.
    Error,
    /// Fokus: Vordergrundfarbe.
    Focus,
    /// Wartet und hängt an einem Prozess: Wartefarbe.
    Waiting,
    /// Status Fehler: Fehlerfarbe.
    StatusError,
    /// Treffer der offenen Suche, ohne Fokus und Zustand: Akzentfarbe.
    Hit,
    /// Sonst die dunkle Hälfte der Fuge.
    Seam,
}

/// Treffer der Suche färben nur die ruhige Fuge: Fokus, Fehler und Warten behalten ihre Farbe.
pub fn with_hit(edge: Edge, hit: Option<bool>) -> Edge {
    if edge == Edge::Seam && hit == Some(true) { Edge::Hit } else { edge }
}

pub fn edge(drop_target: bool, r: Run, focused: bool, status: Status) -> Edge {
    if drop_target {
        Edge::Drop
    } else if r.missing_folder || r.exit_code.is_some() {
        Edge::Error
    } else if focused {
        Edge::Focus
    } else if status == Status::Waiting && r.attached {
        Edge::Waiting
    } else if status == Status::Error {
        Edge::StatusError
    } else {
        Edge::Seam
    }
}

/// Breite des Rands in Pixeln: 2 bei Fokus oder als Ablageziel, sonst 1.
pub fn edge_width(focused: bool, drop_target: bool) -> f32 {
    if focused || drop_target { 2. } else { 1. }
}

/// Breite mit Treffer: die Hervorhebung der Suche ist 2 breit wie der Fokus.
pub fn edge_width_for(edge: Edge, focused: bool) -> f32 {
    if edge == Edge::Hit { 2. } else { edge_width(focused, false) }
}

/// Breiten der Teile in der Kopfzeile (Text gemessen, ohne Abstände).
#[derive(Clone, Copy, Debug, Default)]
pub struct HeaderIn {
    pub width: f32,
    pub title: f32,
    /// 0 ohne Zweig.
    pub branch: f32,
    /// Breite des Gruppen-Labels in Versalien, 0 ohne Gruppenname.
    pub label: f32,
    pub meta: f32,
    pub hovered: bool,
    pub zoomed: bool,
}

/// Wo was in der Kopfzeile steht (x in Punkten ab linker Kante).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HeaderOut {
    pub title_w: f32,
    pub branch_x: Option<f32>,
    /// Akzentquadrat und Label der Gruppe: (x des Akzents, x des Labels, Breite des Labels).
    pub group: Option<(f32, f32, f32)>,
    pub meta_x: f32,
    pub zoom_x: Option<f32>,
}

/// Der Titel hat Vorrang: Branch und Gruppe erscheinen nur, solange daneben noch Platz ist.
pub fn header_layout(i: HeaderIn) -> HeaderOut {
    let reserved = if i.hovered { 84. } else { 0. };
    let mut icons = reserved;
    let zoom_x = i.zoomed.then(|| {
        let x = i.width - icons - 9. - i.meta - 8. - 16.;
        icons += 24.;
        x
    });
    let avail = (i.width - 24. - i.meta - icons - 16.).max(0.);
    let title_w = i.title.min(avail);
    let mut x = 24. + title_w + 8.;
    let mut rest = avail - title_w - 8.;
    let mut branch_x = None;
    let branch_w = if i.branch == 0. { 0. } else { i.branch + 8. };
    if branch_w > 0. && rest >= branch_w {
        branch_x = Some(x);
        x += branch_w;
        rest -= branch_w;
    }
    let group = (i.label > 0. && rest >= (i.label + 11.).min(51.)).then_some((x + 3., x + 11., rest - 11.));
    HeaderOut { title_w, branch_x, group, meta_x: i.width - reserved - 9. - i.meta, zoom_x }
}

/// Symbole der Kopfzeile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Close,
    Rename,
    Deselect,
    Zoom,
}

/// Rechteck (x, y, Breite, Höhe) des Symbols in Punkten für eine Kachel der Breite `width`.
pub fn icon_rect(icon: Icon, width: f32) -> (f32, f32, f32, f32) {
    let close = width - 24.;
    let x = match icon {
        Icon::Close => close,
        Icon::Rename => close - 20.,
        Icon::Deselect => close - 40.,
        Icon::Zoom => close - 60.,
    };
    (x, 5., 16., 16.)
}

/// Welches Symbol liegt unter `p` (Punkte ab linker oberer Ecke der Kachel)? Trefferfläche: das Schließen-Kreuz 4 Punkte
/// größer rundum, die anderen 2 seitlich und 4 oben und unten.
pub fn hit_icon(p: (f32, f32), width: f32) -> Option<Icon> {
    [Icon::Close, Icon::Rename, Icon::Deselect, Icon::Zoom].into_iter().find(|icon| {
        let (x, y, w, h) = icon_rect(*icon, width);
        let dx = if *icon == Icon::Close { 4. } else { 2. };
        p.0 >= x - dx && p.0 <= x + w + dx && p.1 >= y - 4. && p.1 <= y + h + 4.
    })
}

/// Was ein Klick auf eine Stack-Zeile trifft.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowZone {
    Close,
    Rename,
    Row,
}

/// Abstand von der rechten Kante in Punkten: unter 30 schließt, unter 50 benennt um, sonst die Zeile.
pub fn row_zone(from_right: f32) -> RowZone {
    if from_right < 30. {
        RowZone::Close
    } else if from_right < 50. {
        RowZone::Rename
    } else {
        RowZone::Row
    }
}

/// Zeilen eines beendeten Prozesses, die in den Körper passen: die letzten, die mit Zeilenhöhe `12 * 1.45` hineinpassen.
pub const LINE_FONT: f32 = 12.;
pub const LINE_HEIGHT: f32 = LINE_FONT * 1.45;

pub fn visible_lines(lines: &[String], height: f32) -> &[String] {
    if height <= 10. || lines.is_empty() {
        return &[];
    }
    let fit = (height / LINE_HEIGHT).floor() as usize;
    &lines[lines.len().saturating_sub(fit)..]
}

/// Farbklasse einer Schnappschuss-Zeile: `>` hell und fett, `⏺`/`●` blasser, sonst gedämpft.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineTone {
    Prompt,
    Marker,
    Plain,
}

pub fn line_tone(line: &str) -> LineTone {
    if line.starts_with('>') {
        LineTone::Prompt
    } else if line.starts_with('⏺') || line.starts_with('●') {
        LineTone::Marker
    } else {
        LineTone::Plain
    }
}

/// Schraffur: schwarze Diagonalen (Breite 6, Abstand `12 * √2` waagerecht) über dem Körper `w` mal `h`; je Linie
/// (x oben, x unten) in Körperkoordinaten, von unten links nach oben rechts.
pub fn hatch_lines(w: f32, h: f32) -> Vec<((f32, f32), (f32, f32))> {
    let step = 12. * std::f32::consts::SQRT_2;
    let mut out = Vec::new();
    let mut x = -h;
    while x < w + h {
        out.push(((x, h), (x + h, 0.)));
        x += step;
    }
    out
}

/// Abstand, ab dem der Pfad in der Stack-Zeile so weit gekürzt wird, dass er passt: Ordner von vorn auf den
/// Anfangsbuchstaben, der letzte Ordner bleibt ganz (`Theme.fitPath`). `fits` misst die Breite.
pub fn fit_path(path: &str, fits: &dyn Fn(&str) -> bool) -> String {
    let mut parts: Vec<String> = path.split('/').map(String::from).collect();
    let mut s = parts.join("/");
    for i in 0..parts.len().saturating_sub(1) {
        if fits(&s) {
            break;
        }
        if parts[i].chars().count() <= 1 {
            continue;
        }
        let keep = if parts[i].starts_with('.') { 2 } else { 1 };
        parts[i] = parts[i].chars().take(keep).collect();
        s = parts.join("/");
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run() -> Run {
        Run { attached: true, ..Run::default() }
    }

    #[test]
    fn body_kind_follows_the_precedence_of_draw_body() {
        let r = Run { missing_folder: true, exit_code: Some(1), ended: true, ..run() };
        assert_eq!(body_kind(r), BodyKind::MissingFolder);
        assert_eq!(body_kind(Run { missing_folder: false, ..r }), BodyKind::StartFailed(1));
        assert_eq!(body_kind(Run { exit_code: None, missing_folder: false, ..r }), BodyKind::Ended);
        assert_eq!(body_kind(Run { attached: false, previewing: true, ..Run::default() }), BodyKind::Preview);
        assert_eq!(body_kind(Run::default()), BodyKind::Starting);
        assert_eq!(body_kind(Run { previewing: true, ..run() }), BodyKind::Terminal, "mit Prozess keine Vorschau-Meldung");
        assert_eq!(body_kind(Run { elsewhere: true, ..run() }), BodyKind::Elsewhere);
        assert_eq!(body_kind_mounted(run(), false), BodyKind::Lines);
        assert_eq!(body_kind_mounted(run(), true), BodyKind::Terminal);
        assert_eq!(body_kind_mounted(Run::default(), false), BodyKind::Starting);
    }

    #[test]
    fn search_hits_only_color_the_quiet_seam() {
        assert_eq!(with_hit(Edge::Seam, Some(true)), Edge::Hit);
        assert_eq!(with_hit(Edge::Seam, Some(false)), Edge::Seam);
        assert_eq!(with_hit(Edge::Seam, None), Edge::Seam);
        assert_eq!(with_hit(Edge::Focus, Some(true)), Edge::Focus);
        assert_eq!(with_hit(Edge::Error, Some(true)), Edge::Error);
        assert_eq!((edge_width_for(Edge::Hit, false), edge_width_for(Edge::Seam, false), edge_width_for(Edge::Focus, true)), (2., 1., 2.));
    }

    #[test]
    fn edge_color_and_width_follow_cell_view() {
        let r = run();
        assert_eq!(edge(true, Run { missing_folder: true, ..r }, true, Status::Idle), Edge::Drop);
        assert_eq!(edge(false, Run { missing_folder: true, ..r }, true, Status::Idle), Edge::Error);
        assert_eq!(edge(false, Run { exit_code: Some(2), ..r }, false, Status::Idle), Edge::Error);
        assert_eq!(edge(false, r, true, Status::Waiting), Edge::Focus);
        assert_eq!(edge(false, r, false, Status::Waiting), Edge::Waiting);
        assert_eq!(edge(false, Run::default(), false, Status::Waiting), Edge::Seam, "wartend nur mit Prozess");
        assert_eq!(edge(false, r, false, Status::Error), Edge::StatusError);
        assert_eq!(edge(false, r, false, Status::Idle), Edge::Seam);
        assert_eq!((edge_width(true, false), edge_width(false, true), edge_width(false, false)), (2., 2., 1.));
    }

    #[test]
    fn header_title_has_priority_over_branch_and_group() {
        let base = HeaderIn { width: 400., title: 80., branch: 50., label: 60., meta: 30., ..Default::default() };
        let wide = header_layout(base);
        assert_eq!(wide.title_w, 80.);
        assert_eq!(wide.branch_x, Some(24. + 80. + 8.));
        let (accent, label, w) = wide.group.unwrap();
        assert_eq!((accent, label), (24. + 80. + 8. + 58. + 3., 24. + 80. + 8. + 58. + 11.));
        assert!(w > 0.);
        assert_eq!(wide.meta_x, 400. - 9. - 30.);
        // Eng: der Branch passt nicht ganz und fällt weg, die Gruppe bekommt den Rest.
        let narrow = header_layout(HeaderIn { width: 200., ..base });
        assert_eq!(narrow.branch_x, None);
        // Sehr eng: der Titel wird gekürzt, nichts sonst.
        let tiny = header_layout(HeaderIn { width: 100., ..base });
        assert!(tiny.title_w < 80. && tiny.branch_x.is_none() && tiny.group.is_none());
        assert!(tiny.title_w >= 0.);
        // Hover reserviert 84 pt rechts.
        let hover = header_layout(HeaderIn { hovered: true, ..base });
        assert_eq!(hover.meta_x, 400. - 84. - 9. - 30.);
    }

    #[test]
    fn zoom_badge_sits_left_of_the_age_and_takes_24_points() {
        let i = HeaderIn { width: 400., title: 300., meta: 30., zoomed: true, ..Default::default() };
        let o = header_layout(i);
        assert_eq!(o.zoom_x, Some(400. - 9. - 30. - 8. - 16.));
        let plain = header_layout(HeaderIn { zoomed: false, ..i });
        assert_eq!(o.title_w, 300_f32.min(400. - 24. - 30. - 24. - 16.));
        assert_eq!(plain.title_w, 300.);
    }

    #[test]
    fn icons_sit_20_points_apart_from_the_right_and_hit_zones_are_wider() {
        assert_eq!(icon_rect(Icon::Close, 300.), (276., 5., 16., 16.));
        assert_eq!(icon_rect(Icon::Rename, 300.).0, 256.);
        assert_eq!(icon_rect(Icon::Deselect, 300.).0, 236.);
        assert_eq!(icon_rect(Icon::Zoom, 300.).0, 216.);
        assert_eq!(hit_icon((284., 13.), 300.), Some(Icon::Close));
        assert_eq!(hit_icon((272.5, 13.), 300.), Some(Icon::Close), "4 pt Rand um das Kreuz");
        assert_eq!(hit_icon((264., 13.), 300.), Some(Icon::Rename));
        assert_eq!(hit_icon((244., 1.5), 300.), Some(Icon::Deselect), "4 pt oben");
        assert_eq!(hit_icon((224., 13.), 300.), Some(Icon::Zoom));
        assert_eq!(hit_icon((100., 13.), 300.), None);
        assert_eq!(hit_icon((284., 40.), 300.), None);
    }

    #[test]
    fn stack_row_zones_by_distance_from_the_right_edge() {
        assert_eq!(row_zone(0.), RowZone::Close);
        assert_eq!(row_zone(29.9), RowZone::Close);
        assert_eq!(row_zone(30.), RowZone::Rename);
        assert_eq!(row_zone(49.9), RowZone::Rename);
        assert_eq!(row_zone(50.), RowZone::Row);
    }

    #[test]
    fn only_the_corner_tile_is_rounded_concentric_to_the_module_corner() {
        assert_eq!(module_radius(1.), 14.);
        assert_eq!(module_radius(1.3), 18.);
        assert_eq!(corner_radius((6., 6.), 6., 1.), 8., "14 minus Abstand");
        assert_eq!(corner_radius((6., 6.), 20., 1.), 0., "Abstand größer als der Radius");
        assert_eq!(corner_radius((6., 40.), 6., 1.), 0.);
        assert_eq!(corner_radius((300., 6.), 6., 1.), 0.);
        assert_eq!(header_height(1.), 26.);
        assert_eq!(header_height(1.3), 34.);
    }

    #[test]
    fn snapshot_lines_show_the_last_ones_that_fit() {
        let lines: Vec<String> = (0..10).map(|i| format!("z{i}")).collect();
        assert_eq!(visible_lines(&lines, 3. * LINE_HEIGHT + 1.).len(), 3);
        assert_eq!(visible_lines(&lines, 3. * LINE_HEIGHT + 1.)[0], "z7");
        assert!(visible_lines(&lines, 8.).is_empty(), "unter 10 pt nichts");
        assert!(visible_lines(&lines, 12.).is_empty(), "keine ganze Zeile passt");
        assert_eq!(visible_lines(&lines, 1000.).len(), 10);
        assert!((LINE_HEIGHT - 17.4).abs() < 0.001);
    }

    #[test]
    fn line_tones_by_prefix() {
        assert_eq!(line_tone("> frage"), LineTone::Prompt);
        assert_eq!(line_tone("⏺ antwort"), LineTone::Marker);
        assert_eq!(line_tone("● punkt"), LineTone::Marker);
        assert_eq!(line_tone("text"), LineTone::Plain);
    }

    #[test]
    fn hatch_covers_the_body_diagonally_with_12_root_2_spacing() {
        let lines = hatch_lines(100., 50.);
        assert_eq!(lines[0], ((-50., 50.), (0., 0.)));
        let step = lines[1].0.0 - lines[0].0.0;
        assert!((step - 12. * 2f32.sqrt()).abs() < 0.001);
        assert!(lines.last().unwrap().0.0 < 150.);
    }

    #[test]
    fn long_paths_shrink_folders_from_the_front_but_keep_the_last() {
        let width = |s: &str| s.len() <= 12;
        assert_eq!(fit_path("~/dev/projects/kadrell", &|s| width(s)), "~/d/p/kadrell");
        assert_eq!(fit_path("~/.claude/x", &|s| s.len() <= 5), "~/.c/x");
        assert_eq!(fit_path("~/a", &|_| true), "~/a");
        assert_eq!(fit_path("~/dev/projekt", &|_| false), "~/d/projekt", "der letzte Ordner bleibt ganz");
    }
}
