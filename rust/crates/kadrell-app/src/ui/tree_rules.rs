//! Regeln des Baums ohne GPUI, damit sie sich wie die Swift-Tests prüfen lassen: Statusfarbe mit Shell-Farbe
//! (`Theme.statusColor`, `Theme.activityColor`), Form des Statuspunkts bei „Unterscheiden ohne Farbe“
//! (`Icons.statusDot`), Aufblitzen und Einfahren (`Feedback.progress`, `SidebarView.dotColor`), Kürzen des Gruppenpfads
//! (`Theme.fitPath`) und das Gruppen-Label (`SidebarRenderer.groupLabel`).

use gpui::Hsla;

use super::layout::{Id, Selection};
use super::model::Status;
use crate::theme::Theme;

// MARK: Farben

/// Statuspunkt (`Theme.statusColor`): ohne laufenden Prozess grau, die Shell-Farbe nur bei fertiger Session mit noch
/// laufendem Kommando aus dem Bash-Tool; Wartet und Fehler behalten ihre eigene Farbe.
pub fn status_color(status: Status, shell_running: bool, t: &Theme) -> Hsla {
    if status == Status::Idle && shell_running { t.shell } else { status.color(t) }
}

/// Farbe des atmenden Punkts (`Theme.activityColor`), `None` = ruhiger Statuspunkt: Claude arbeitet, oder die Session
/// ist fertig und im Hintergrund läuft noch ein Kommando. Einzige Quelle dieser Regel für Baum, Kachel und Stack.
pub fn activity_color(status: Status, shell_running: bool, t: &Theme) -> Option<Hsla> {
    let works = status == Status::Running || (shell_running && status == Status::Idle);
    works.then(|| status_color(status, shell_running, t))
}

// MARK: Form des Punkts

/// Form des Statuspunkts. Mit „Unterscheiden ohne Farbe“ trägt sie den Status: Ring = nicht gestartet, zwei Balken =
/// wartet, Ausrufezeichen = Fehler. Fertig und arbeitet bleiben ein gefüllter Punkt.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DotShape {
    Disc,
    Ring,
    Pause,
    Bang,
}

pub fn dot_shape(status: Status, differentiate: bool) -> DotShape {
    match (differentiate, status) {
        (false, _) => DotShape::Disc,
        (true, Status::Detached) => DotShape::Ring,
        (true, Status::Waiting) => DotShape::Pause,
        (true, Status::Error) => DotShape::Bang,
        (true, _) => DotShape::Disc,
    }
}

/// Strichstärke und Einzug des Rings (`Icons.statusDot`).
pub const RING_WIDTH: f32 = 1.5;
pub const RING_INSET: f32 = 1.;

/// Rechteck als Anteile der Punktfläche: (x, y, Breite, Höhe).
pub type Frac = (f32, f32, f32, f32);

/// Zeichen auf dem Punkt in Anteilen der Fläche, von oben links: zwei Balken (Wartet) bzw. Strich und Punkt (Fehler).
pub fn marks(shape: DotShape) -> Vec<Frac> {
    match shape {
        DotShape::Pause => vec![(0.30, 0.22, 0.16, 0.56), (0.54, 0.22, 0.16, 0.56)],
        DotShape::Bang => vec![(0.42, 0.20, 0.16, 0.42), (0.42, 0.68, 0.16, 0.16)],
        DotShape::Disc | DotShape::Ring => Vec::new(),
    }
}

// MARK: Aufblitzen und Einfahren

/// Dauer des Aufblitzens bei einem Statuswechsel und des Einfahrens einer neuen Session (Sekunden).
pub const FLASH_SECS: f32 = 0.8;
pub const APPEAR_SECS: f32 = 0.25;
/// Weg, den eine neue Zeile von links einfährt, in Punkten.
pub const SLIDE_FROM: f32 = 24.;

/// Fortschritt einer Animation, 0 bis 1, zum Ende hin abbremsend (`Feedback.progress`). `None` = vorbei, bei „Bewegung
/// reduzieren“ sofort: der Aufrufer zeigt dann den Endzustand.
pub fn progress(elapsed: f32, duration: f32, reduce_motion: bool) -> Option<f32> {
    if reduce_motion {
        return None;
    }
    let p = elapsed / duration;
    (0. ..1.).contains(&p).then(|| 1. - (1. - p).powi(3))
}

/// Stärke des Weiß im Punkt: fertig blendet aus, wartet pulsiert zweimal (`|sin|`).
pub fn flash_amount(waiting: bool, p: f32) -> f32 {
    if waiting { (p * std::f32::consts::TAU).sin().abs() } else { 1. - p }
}

/// `NSColor.white.mixed(0.75 * amount, into: base)`.
pub fn flash_color(base: Hsla, amount: f32) -> Hsla {
    let (b, f) = (gpui::Rgba::from(base), 0.75 * amount);
    let mix = |c: f32| 1. * f + c * (1. - f);
    gpui::Rgba { r: mix(b.r), g: mix(b.g), b: mix(b.b), a: 1. }.into()
}

/// Versatz nach links und Deckkraft einer einfahrenden Zeile (`drawSession`: `(1 - slide) * -24`, Alpha = Fortschritt).
pub fn slide(p: f32) -> (f32, f32) {
    ((1. - p) * -SLIDE_FROM, p)
}

// MARK: Pfad und Label

/// `shortPath`, bei Platzmangel von vorn Ordner auf den Anfangsbuchstaben gekürzt (`~/d/p/projekt`, `.claude` wird
/// `.c`). Der letzte Ordner bleibt ganz. `path` hat schon `~` statt Home; `measure` liefert die Breite eines Texts.
/// Ein Laufwerk (`C:`) bleibt stehen, wie `~` (Swift kennt keine Laufwerke).
pub fn fit_path(path: &str, width: f32, measure: impl Fn(&str) -> f32) -> String {
    let mut parts: Vec<String> = path.split('/').map(String::from).collect();
    let mut s = parts.join("/");
    let last = parts.len().saturating_sub(1);
    for i in 0..last {
        if measure(&s) <= width {
            break;
        }
        if parts[i].chars().count() > 1 && !parts[i].ends_with(':') {
            let keep = if parts[i].starts_with('.') { 2 } else { 1 };
            parts[i] = parts[i].chars().take(keep).collect();
            s = parts.join("/");
        }
    }
    s
}

/// Gruppen-Label in Versalien: Name und ` · Zahl` getrennt, damit nur der Name gekürzt wird und die Zahl ganz bleibt.
pub fn group_label_parts(name: &str, count: usize) -> (String, String) {
    (name.to_uppercase(), format!(" · {count}"))
}

/// Abstände rechts neben den Teilen der Gruppen-Kopfzeile (unskaliert), von rechts nach links gelegt wie in
/// `drawGroup`: Herz, Warte-Marke, Punkte (kompakt, eingeklappt), Label.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HeadGaps {
    pub badge: f32,
    pub dots: f32,
    pub label: f32,
}

pub fn head_gaps(heart: bool, badge: bool, dots: bool, compact: bool) -> HeadGaps {
    let after_heart = if heart { 6. } else { 0. };
    let past_badge = if badge { 8. } else { after_heart };
    let label = if dots {
        8.
    } else if compact {
        past_badge + 5.
    } else {
        past_badge
    };
    HeadGaps { badge: after_heart, dots: past_badge, label }
}

/// Name auf höchstens `max` Zeichen kürzen, mit `…` am Ende (`truncatesLastVisibleLine`).
pub fn truncate_tail(text: &str, max: usize) -> String {
    let n = text.chars().count();
    if n <= max {
        return text.to_string();
    }
    let keep = max.saturating_sub(1);
    format!("{}…", text.chars().take(keep).collect::<String>())
}

/// Die letzten `max` Zeichen mit `…` davor (`byTruncatingHead`, die Nachrichtenzeile zeigt das Ende der Antwort).
pub fn truncate_head(text: &str, max: usize) -> String {
    let n = text.chars().count();
    if n <= max {
        return text.to_string();
    }
    let keep = max.saturating_sub(1);
    format!("…{}", text.chars().skip(n - keep).collect::<String>())
}

/// Nachricht der Zeile: die letzten 300 Zeichen (`m.suffix(300)`), einzeilig.
pub fn message_tail(message: &str) -> String {
    let n = message.chars().count();
    let tail: String = message.chars().skip(n.saturating_sub(300)).collect();
    tail.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Zeichen, die in `width` Punkten passen, bei Schrift mit fester Zeichenbreite `advance`.
pub fn fits(width: f32, advance: f32) -> usize {
    if advance <= 0. { 0 } else { (width / advance).floor().max(0.) as usize }
}

/// Zeichenbreite von JetBrains Mono im Verhältnis zur Schriftgröße.
pub const MONO_ADVANCE: f32 = 0.6;

// MARK: Klick auf eine Session

/// Auswahl nach einem Klick auf die Session `id` (`mouseUp` des Baums): Umschalt markiert den Bereich seit dem letzten
/// Klick, Cmd/Strg nimmt sie dazu oder weg, ein gewöhnlicher Klick zeigt nur diese, auch wenn sie schon ausgewählt war
/// (seit 1.56.3 isoliert er sie, statt sie aus der Auswahl zu nehmen).
pub fn click_selection(sel: &Selection, id: Id, shift: bool, secondary: bool, flat: &[Id]) -> Selection {
    let mut next = sel.clone();
    if shift {
        next.range(id, flat);
    } else if secondary {
        next.toggle(id, flat);
    } else {
        next = Selection::single(id);
    }
    next
}

// MARK: Alle Gruppen ein- und ausklappen

/// ⌘⌥ „alle Gruppen“ (`toggleAllGroups`): ist irgendeine Gruppe offen, gehen alle zu, sonst alle auf. Liefert, ob nach
/// dem Umschalten alle zugeklappt sein sollen.
pub fn collapse_all(collapsed: &[bool]) -> bool {
    collapsed.iter().any(|c| !c)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::Rgba;

    fn rgba(c: Hsla) -> u32 {
        let r: Rgba = c.into();
        let b = |v: f32| (v * 255.).round() as u32;
        (b(r.r) << 16) | (b(r.g) << 8) | b(r.b)
    }

    /// Instrument Dark, das Schema der Swift-Tests (`Theme.shell` = `#a3a19a`, wartet `#ff5e14`).
    fn instrument() -> Theme {
        Theme::by_id("instrument-dark").unwrap()
    }

    /// `testIdleWithShellPaintsShellColor`.
    #[test]
    fn idle_with_shell_paints_shell_color() {
        let t = instrument();
        assert_eq!(status_color(Status::Idle, true, &t), t.shell);
        assert_eq!(rgba(t.shell), 0xa3a19a);
    }

    /// `testWaitingWithShellKeepsWaitingColor`.
    #[test]
    fn waiting_with_shell_keeps_waiting_color() {
        let t = instrument();
        assert_eq!(rgba(status_color(Status::Waiting, true, &t)), 0xff5e14);
    }

    /// `testErrorWithShellKeepsErrorColor`.
    #[test]
    fn error_with_shell_keeps_error_color() {
        let t = instrument();
        assert_eq!(status_color(Status::Error, true, &t), t.error);
    }

    /// `testRunningShellPaintsShellColor`: fertig mit laufender Shell, angehängt.
    #[test]
    fn running_shell_paints_shell_color() {
        let t = instrument();
        assert_eq!(status_color(Status::Idle, true, &t), t.shell);
        assert_eq!(status_color(Status::Idle, false, &t), t.idle, "ohne Shell bleibt fertig grün");
    }

    /// `testWorkingSessionKeepsRunningColor`: arbeitet Claude, gilt die Arbeitsfarbe, auch mit Shell.
    #[test]
    fn working_session_keeps_running_color() {
        let t = instrument();
        assert_eq!(status_color(Status::Running, true, &t), t.running);
    }

    /// `testDetachedStaysGrey`: ohne laufenden Prozess grau, egal ob eine Shell als laufend gemeldet wird.
    #[test]
    fn detached_stays_grey() {
        let t = instrument();
        assert_eq!(status_color(Status::Detached, true, &t), t.detached);
    }

    /// `testShellRunningUsesShellColor`: der atmende Punkt einer fertigen Session mit Shell hat die Shell-Farbe.
    #[test]
    fn shell_running_breathes_in_shell_color() {
        let t = instrument();
        assert_eq!(activity_color(Status::Idle, true, &t), Some(t.shell));
        assert_eq!(rgba(activity_color(Status::Idle, true, &t).unwrap()), 0xa3a19a);
    }

    #[test]
    fn only_working_sessions_breathe() {
        let t = instrument();
        assert_eq!(activity_color(Status::Running, false, &t), Some(t.running));
        assert_eq!(activity_color(Status::Running, true, &t), Some(t.running));
        for (s, shell) in [(Status::Idle, false), (Status::Waiting, true), (Status::Error, true), (Status::Detached, true)] {
            assert_eq!(activity_color(s, shell, &t), None, "{s:?} mit Shell {shell}");
        }
    }

    #[test]
    fn every_scheme_has_a_shell_color() {
        for t in Theme::all() {
            if t.id.starts_with("instrument") {
                assert!(rgba(t.shell) == 0xa3a19a || rgba(t.shell) == 0x5c5a54);
            }
        }
        assert_eq!(rgba(Theme::by_id("instrument-light").unwrap().shell), 0x5c5a54);
        assert_eq!(rgba(Theme::mocha().shell), 0x89b4fa, "sonst das Blau der Terminalfarben");
    }

    #[test]
    fn differentiate_encodes_status_in_the_shape() {
        use Status::*;
        assert_eq!(dot_shape(Detached, true), DotShape::Ring);
        assert_eq!(dot_shape(Waiting, true), DotShape::Pause);
        assert_eq!(dot_shape(Error, true), DotShape::Bang);
        assert_eq!(dot_shape(Idle, true), DotShape::Disc);
        assert_eq!(dot_shape(Running, true), DotShape::Disc);
        for s in [Detached, Waiting, Error, Idle, Running] {
            assert_eq!(dot_shape(s, false), DotShape::Disc, "ohne den Schalter immer ein Punkt");
        }
        assert_eq!(marks(DotShape::Pause).len(), 2);
        assert_eq!(marks(DotShape::Bang).len(), 2);
        assert!(marks(DotShape::Disc).is_empty() && marks(DotShape::Ring).is_empty());
    }

    #[test]
    fn progress_eases_out_and_ends() {
        assert_eq!(progress(0., 0.8, false), Some(0.));
        let mid = progress(0.4, 0.8, false).unwrap();
        assert!((mid - 0.875).abs() < 1e-5);
        assert_eq!(progress(0.8, 0.8, false), None);
        assert_eq!(progress(-0.1, 0.8, false), None);
        assert_eq!(progress(0.1, 0.8, true), None, "Bewegung reduzieren: sofort der Endzustand");
    }

    #[test]
    fn flash_mixes_white_into_the_dot() {
        assert_eq!(flash_amount(false, 0.), 1.);
        assert_eq!(flash_amount(false, 1.), 0.);
        assert!((flash_amount(true, 0.25) - 1.).abs() < 1e-5, "wartet: pulsiert zweimal, Spitze bei 1/4");
        assert!(flash_amount(true, 0.5) < 1e-5);
        let black = gpui::rgb(0x000000).into();
        assert_eq!(rgba(flash_color(black, 0.)), 0x000000);
        let full = rgba(flash_color(black, 1.));
        assert_eq!(full, 0xbfbfbf, "0,75 Weiß in Schwarz");
        let white = gpui::rgb(0xffffff).into();
        assert_eq!(rgba(flash_color(white, 1.)), 0xffffff);
    }

    #[test]
    fn new_rows_slide_in_from_24_points() {
        assert_eq!(slide(0.), (-24., 0.));
        assert_eq!(slide(1.), (0., 1.));
        assert_eq!(slide(0.5), (-12., 0.5));
    }

    /// `testFitPath`-Verhalten: von links auf den Anfangsbuchstaben, der letzte Ordner bleibt.
    #[test]
    fn fit_path_shortens_leading_folders_left_to_right() {
        let chars = |s: &str| s.chars().count() as f32;
        assert_eq!(fit_path("~/development/projects/kadrell", 100., chars), "~/development/projects/kadrell");
        assert_eq!(fit_path("~/development/projects/kadrell", 24., chars), "~/d/projects/kadrell");
        assert_eq!(fit_path("~/development/projects/kadrell", 16., chars), "~/d/p/kadrell");
        assert_eq!(fit_path("~/development/projects/kadrell", 1., chars), "~/d/p/kadrell", "der letzte Ordner bleibt ganz");
    }

    #[test]
    fn fit_path_keeps_two_characters_for_dot_folders_and_the_drive() {
        let chars = |s: &str| s.chars().count() as f32;
        assert_eq!(fit_path("~/.claude/projekt", 1., chars), "~/.c/projekt");
        assert_eq!(fit_path("C:/development/kadrell", 1., chars), "C:/d/kadrell");
        assert_eq!(fit_path("/usr/local/bin/x", 1., chars), "/u/l/b/x", "führendes Leerstück bleibt");
    }

    #[test]
    fn fit_path_stops_as_soon_as_it_fits() {
        let chars = |s: &str| s.chars().count() as f32;
        let one = fit_path("~/development/projects/kadrell", 20., chars);
        assert_eq!(one, "~/d/projects/kadrell");
        assert!(chars(&one) <= 20.);
    }

    /// `testGroupLabelIsUppercaseWithCount`: `KADRELL · 2`.
    #[test]
    fn group_label_is_uppercase_with_count() {
        let (name, count) = group_label_parts("Kadrell", 2);
        assert_eq!(format!("{name}{count}"), "KADRELL · 2");
        assert_eq!(group_label_parts("straße", 1).0, "STRASSE");
    }

    #[test]
    fn name_is_cut_the_count_stays() {
        assert_eq!(truncate_tail("KADRELL-SPIKE", 8), "KADRELL…");
        assert_eq!(truncate_tail("KADRELL", 8), "KADRELL");
        assert_eq!(truncate_tail("ABC", 0), "…");
    }

    #[test]
    fn message_shows_the_end_of_the_answer() {
        assert_eq!(truncate_head("Soll ich weitermachen?", 10), "…ermachen?");
        assert_eq!(truncate_head("kurz", 10), "kurz");
        let long = "a".repeat(400) + "ENDE";
        let tail = message_tail(&long);
        assert_eq!(tail.chars().count(), 300);
        assert!(tail.ends_with("ENDE"));
        assert_eq!(message_tail("eins\n\nzwei   drei"), "eins zwei drei", "einzeilig");
    }

    /// Abstände der Kopfzeile wie `drawFavorite`, `drawWaitingBadge` und die Punkte des kompakten Designs.
    #[test]
    fn head_gaps_follow_the_swift_layout() {
        assert_eq!(head_gaps(false, false, false, false), HeadGaps { badge: 0., dots: 0., label: 0. });
        assert_eq!(head_gaps(true, false, false, false).label, 6.);
        assert_eq!(head_gaps(true, true, false, false), HeadGaps { badge: 6., dots: 8., label: 8. });
        assert_eq!(head_gaps(false, true, false, false).badge, 0.);
        assert_eq!(head_gaps(false, false, false, true).label, 5., "kompakt: 5 vor dem Label");
        assert_eq!(head_gaps(false, true, true, true).label, 8., "mit Punkten: 3 + 5 hinter dem letzten Punkt");
        assert_eq!(head_gaps(true, false, true, true).dots, 6.);
    }

    #[test]
    fn fits_counts_whole_characters() {
        assert_eq!(fits(63., 6.3), 10);
        assert_eq!(fits(-5., 6.3), 0);
        assert_eq!(fits(10., 0.), 0);
    }

    /// `toggleAllGroups`: irgendeine offen -> alle zu, sonst alle auf.
    #[test]
    fn any_open_group_collapses_all_else_opens_all() {
        assert!(collapse_all(&[true, false, true]));
        assert!(collapse_all(&[false, false]));
        assert!(!collapse_all(&[true, true]));
        assert!(!collapse_all(&[]), "ohne Gruppen gibt es nichts zuzuklappen");
    }
}
