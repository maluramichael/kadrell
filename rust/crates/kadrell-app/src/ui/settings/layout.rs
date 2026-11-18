//! Rechnung hinter den Spalten der Einstellungen (`ColumnBalance` in `SettingsLayout.swift`), ohne Oberfläche:
//! Spaltenzahl, Teilung der Abschnitte und Breite der Steuerelemente. Die Höhe jedes Abschnitts misst der Frame davor
//! (`measure` in `controls`), erst im ersten Frame ohne Messung gilt eine Schätzung aus der Zeilenzahl.

/// Ab dieser Spaltenbreite stehen zwei Spalten nebeneinander, darunter eine.
pub const MIN_COLUMN_WIDTH: f32 = 600.;
/// Abstand zwischen zwei Abschnitten einer Spalte.
pub const SECTION_SPACING: f32 = 4.;
/// Höchstbreite der Steuerelemente, logische Punkte.
pub const MAX_CONTROL_WIDTH: f32 = 300.;

/// Eine oder zwei Spalten, je nach Breite.
pub fn count(width: f32) -> usize {
    if width >= 2. * MIN_COLUMN_WIDTH { 2 } else { 1 }
}

/// Anzahl der Abschnitte in der linken Spalte, bei der die höhere Spalte am niedrigsten ist. Bei Gleichstand
/// bekommt die linke Spalte mehr, damit ein einzelner Abschnitt links steht und nicht rechts.
pub fn split(heights: &[f32], spacing: f32) -> usize {
    let column = |part: &[f32]| part.iter().sum::<f32>() + spacing * part.len().saturating_sub(1) as f32;
    let longer = |k: usize| column(&heights[..k]).max(column(&heights[k..]));
    let mut best = 0;
    for k in 1..=heights.len() {
        let (a, b) = (longer(k), longer(best));
        if a < b || (a == b && k > best) {
            best = k;
        }
    }
    best
}

/// Anzahl der Abschnitte in der linken Spalte: bei einer Spalte alle, bei zwei die ausgewogene Teilung. Die Spalten
/// sind zusammenhängende Teile der Reihenfolge, Tab läuft also erst die linke hinunter, dann die rechte.
pub fn left_count(heights: &[f32], columns: usize) -> usize {
    if columns == 2 { split(heights, SECTION_SPACING) } else { heights.len() }
}

/// Rund 45 % der Spalte nach Abzug der Ränder, höchstens 300, mindestens 120. Ohne gemessene Breite die volle.
pub fn control_width(column: f32) -> f32 {
    if column <= 0. {
        return MAX_CONTROL_WIDTH;
    }
    (((column - 64.) * 0.45).round()).clamp(120., MAX_CONTROL_WIDTH)
}

/// Höhen für die Teilung: je Abschnitt die gemessene, solange noch keine vorliegt die Schätzung (Swift nimmt dann 0,
/// was im ersten Frame kurz falsch teilt).
pub fn heights(measured: &[Option<f32>], estimated: &[f32]) -> Vec<f32> {
    estimated.iter().enumerate().map(|(i, e)| measured.get(i).copied().flatten().unwrap_or(*e)).collect()
}

/// Hat sich die Grundlage der Teilung (Breite, Höhen) gegenüber dem letzten Frame geändert? Dann braucht es noch einen
/// Frame, weil die Messung dieses Frames erst im nächsten gilt. Unter einem halben Punkt gilt als gleich.
pub fn basis_changed(old: &(f32, Vec<f32>), new: &(f32, Vec<f32>)) -> bool {
    let apart = |a: f32, b: f32| (a - b).abs() >= 0.5;
    apart(old.0, new.0) || old.1.len() != new.1.len() || old.1.iter().zip(&new.1).any(|(a, b)| apart(*a, *b))
}

/// Geschätzte Höhe eines Abschnitts: Überschrift plus Zeilen mit Fuge (zweizeilige Beschriftungen zählen nicht).
pub fn estimated_height(rows: usize, has_heading: bool, has_subtitle: bool) -> f32 {
    let heading = if has_heading { 35. } else { 0. };
    let subtitle = if has_subtitle { 19. } else { 0. };
    heading + subtitle + 34. * rows as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_column_below_twice_the_minimum() {
        assert_eq!(count(1100.), 1);
        assert_eq!(count(1199.), 1);
        assert_eq!(count(1200.), 2);
        assert_eq!(count(1880.), 2);
    }

    #[test]
    fn split_minimises_the_taller_column() {
        assert_eq!(split(&[], 4.), 0);
        assert_eq!(split(&[100.], 4.), 1);
        assert_eq!(split(&[100., 100.], 4.), 1);
        assert_eq!(split(&[100., 100., 100., 100.], 4.), 2);
        assert_eq!(split(&[300., 100., 100.], 4.), 1);
        assert_eq!(split(&[100., 100., 300.], 4.), 2);
    }

    #[test]
    fn split_puts_a_single_section_on_the_left() {
        assert_eq!(split(&[50.], 0.), 1);
        // Gleich hoch bei k = 1 und k = 2: die linke Spalte bekommt mehr.
        assert_eq!(split(&[50., 0.], 0.), 2);
    }

    /// `SettingsLayoutTests.testSplitKeepsOrderAndBalancesColumns`
    #[test]
    fn split_keeps_order_and_balances_columns() {
        assert_eq!(split(&[100., 100., 100., 100.], 0.), 2);
        assert_eq!(split(&[300., 100., 100., 100.], 0.), 1);
        assert_eq!(split(&[100., 100., 100., 300.], 0.), 3);
        assert_eq!(split(&[50.], 0.), 1, "ein Abschnitt bleibt links");
        assert_eq!(split(&[], 0.), 0);
        // Abstände zählen mit: drei kleine Abschnitte links wiegen einen großen rechts auf.
        assert_eq!(split(&[100., 100., 100., 330.], 20.), 3);
    }

    /// `SettingsLayoutTests.testTabRunsDownLeftColumnThenRight`: die Spalten sind Teile der Reihenfolge, Tab läuft
    /// A, B (links), dann C, D (rechts), nie quer.
    #[test]
    fn tab_runs_down_the_left_column_then_the_right() {
        let sections = ["A", "B", "C", "D"];
        let left = left_count(&[100.; 4], 2);
        let (first, second) = sections.split_at(left);
        assert_eq!((first, second), (&["A", "B"][..], &["C", "D"][..]));
        assert_eq!(first.iter().chain(second).copied().collect::<Vec<_>>(), sections);
    }

    /// `SettingsLayoutTests.testSettingsUseTwoColumnsWhenWide`: breit zwei Spalten, schmal eine.
    #[test]
    fn settings_use_two_columns_when_wide() {
        assert_eq!(left_count(&[100.; 4], count(1400.)), 2);
        assert_eq!(left_count(&[100.; 4], count(900.)), 4, "eine Spalte: alle Abschnitte untereinander");
    }

    #[test]
    fn measured_heights_win_over_estimates() {
        assert_eq!(heights(&[Some(120.), None, Some(80.)], &[100., 50., 60.]), vec![120., 50., 80.]);
        assert_eq!(heights(&[], &[100., 50.]), vec![100., 50.]);
        // Mehrzeilige Beschriftungen machen einen Abschnitt höher als geschätzt: die Messung verschiebt die Teilung.
        assert_eq!(split(&heights(&[None, None, None], &[100., 100., 100.]), 0.), 2);
        assert_eq!(split(&heights(&[Some(100.), Some(100.), Some(300.)], &[100., 100., 100.]), 0.), 2);
        assert_eq!(split(&heights(&[Some(300.), Some(100.), Some(100.)], &[100., 100., 100.]), 0.), 1);
    }

    #[test]
    fn a_changed_basis_asks_for_another_frame() {
        let basis = (1000., vec![100., 200.]);
        assert!(!basis_changed(&basis, &(1000.2, vec![100.3, 199.9])));
        assert!(basis_changed(&basis, &(1010., vec![100., 200.])));
        assert!(basis_changed(&basis, &(1000., vec![100., 210.])));
        assert!(basis_changed(&basis, &(1000., vec![100.])));
        assert!(basis_changed(&(0., vec![]), &basis));
    }

    #[test]
    fn control_width_is_45_percent_of_the_column() {
        assert_eq!(control_width(0.), 300.);
        assert_eq!(control_width(940.), 300.);
        assert_eq!(control_width(600.), 241.);
        assert_eq!(control_width(100.), 120.);
    }
}
