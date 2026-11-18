//! Bewegung im Baum (`SidebarView.flashes`, `appeared`): ein Statuswechsel lässt den Punkt aufblitzen (fertig: Weiß
//! blendet aus, wartet: pulsiert zweimal, 0,8 s), eine neu aufgetauchte Session gleitet in 0,25 s von links ein. Beim
//! ersten Laden gleitet nichts (`knownIds == nil`). Die Rechnung steht in `tree_rules`, hier nur der Zustand.

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use kadrell_claude::attention::Transitions;

use super::model::Group;
use super::tree_rules::{self, APPEAR_SECS, FLASH_SECS};
use crate::appearance::breathing::reduce_motion;

/// Was eine Zeile in diesem Bild an Bewegung trägt.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RowAnim {
    /// Stärke des Weiß im Punkt, 0 bis 1.
    pub flash: Option<f32>,
    /// Versatz in Punkten und Deckkraft einer einfahrenden Zeile.
    pub slide: Option<(f32, f32)>,
}

/// Bewegung aller Zeilen zu einem Zeitpunkt, in `SidebarCtx` an den Baum gereicht.
#[derive(Clone, Debug, Default)]
pub struct AnimSnapshot {
    rows: HashMap<String, RowAnim>,
}

impl AnimSnapshot {
    pub fn of(&self, key: &str) -> RowAnim {
        self.rows.get(key).copied().unwrap_or_default()
    }

    pub fn active(&self) -> bool {
        !self.rows.is_empty()
    }
}

#[derive(Default)]
pub struct TreeAnim {
    /// Statuswechsel: Start und ob die Session wartet (sonst fertig).
    flashes: HashMap<String, (Instant, bool)>,
    appeared: HashMap<String, Instant>,
    /// `None` bis zum ersten Laden: beim Start gleitet nichts ein.
    known: Option<HashSet<String>>,
    /// ⌘ (Mac) bzw. Strg (Windows) gehalten: „Neue Session“ der Gruppen zeigt das Terminal-Symbol.
    pub cmd_down: bool,
}

impl TreeAnim {
    /// Gleicht mit den Sessions des Baums ab: neue Ids fahren ein, `transitions` lassen aufblitzen. Abgelaufenes fällt
    /// heraus (`pruneAnimations`).
    pub fn sync(&mut self, groups: &[Group], transitions: &Transitions, now: Instant) {
        self.sync_with(groups, transitions, now, reduce_motion());
    }

    fn sync_with(&mut self, groups: &[Group], transitions: &Transitions, now: Instant, reduce: bool) {
        let ids: HashSet<String> = groups.iter().flat_map(|g| g.sessions.iter().map(|s| s.key.clone())).collect();
        if let Some(known) = &self.known {
            for id in ids.difference(known) {
                self.appeared.insert(id.clone(), now);
            }
        }
        self.known = Some(ids);
        for id in &transitions.done {
            self.flashes.insert(id.clone(), (now, false));
        }
        for id in &transitions.waiting {
            self.flashes.insert(id.clone(), (now, true));
        }
        self.flashes.retain(|_, (start, _)| Self::flash_progress(start, now, reduce).is_some());
        self.appeared.retain(|_, start| Self::appear_progress(start, now, reduce).is_some());
    }

    fn flash_progress(start: &Instant, now: Instant, reduce: bool) -> Option<f32> {
        tree_rules::progress(now.saturating_duration_since(*start).as_secs_f32(), FLASH_SECS, reduce)
    }

    fn appear_progress(start: &Instant, now: Instant, reduce: bool) -> Option<f32> {
        tree_rules::progress(now.saturating_duration_since(*start).as_secs_f32(), APPEAR_SECS, reduce)
    }

    /// Bewegung zum Zeitpunkt `now`.
    pub fn snapshot(&self, now: Instant) -> AnimSnapshot {
        self.snapshot_with(now, reduce_motion())
    }

    fn snapshot_with(&self, now: Instant, reduce: bool) -> AnimSnapshot {
        let mut rows: HashMap<String, RowAnim> = HashMap::new();
        for (id, (start, waiting)) in &self.flashes {
            if let Some(p) = Self::flash_progress(start, now, reduce) {
                rows.entry(id.clone()).or_default().flash = Some(tree_rules::flash_amount(*waiting, p));
            }
        }
        for (id, start) in &self.appeared {
            if let Some(p) = Self::appear_progress(start, now, reduce) {
                rows.entry(id.clone()).or_default().slide = Some(tree_rules::slide(p));
            }
        }
        AnimSnapshot { rows }
    }

    /// Läuft noch etwas? Dann bittet die Shell um das nächste Bild.
    pub fn active(&self, now: Instant) -> bool {
        self.snapshot(now).active()
    }

    /// ⌘/Strg gedrückt oder losgelassen; `true`, wenn sich etwas geändert hat (dann neu zeichnen).
    pub fn set_cmd(&mut self, down: bool) -> bool {
        let changed = self.cmd_down != down;
        self.cmd_down = down;
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::model::test_session;
    use std::time::Duration;

    fn groups(keys: &[&str]) -> Vec<Group> {
        let sessions = keys.iter().map(|k| test_session(k)).collect();
        vec![Group { id: "g".into(), name: "g".into(), color: 0, path: String::new(), cwd: String::new(), favorite: false, collapsed: false, host: None, sessions }]
    }

    fn done(id: &str) -> Transitions {
        Transitions { waiting: HashSet::new(), done: [id.to_string()].into() }
    }

    /// Beim ersten Laden gleitet nichts ein, später aufgetauchte Sessions schon.
    #[test]
    fn nothing_slides_on_the_first_load() {
        let (mut a, t0) = (TreeAnim::default(), Instant::now());
        a.sync_with(&groups(&["a", "b"]), &Transitions::default(), t0, false);
        assert!(!a.snapshot_with(t0, false).active());
        a.sync_with(&groups(&["a", "b", "c"]), &Transitions::default(), t0, false);
        let snap = a.snapshot_with(t0, false);
        assert_eq!(snap.of("c").slide, Some((-24., 0.)));
        assert_eq!(snap.of("a").slide, None);
    }

    #[test]
    fn slide_ends_after_a_quarter_second() {
        let (mut a, t0) = (TreeAnim::default(), Instant::now());
        a.sync_with(&groups(&["a"]), &Transitions::default(), t0, false);
        a.sync_with(&groups(&["a", "b"]), &Transitions::default(), t0, false);
        assert!(a.snapshot_with(t0 + Duration::from_millis(100), false).of("b").slide.is_some());
        assert!(!a.snapshot_with(t0 + Duration::from_millis(260), false).active());
    }

    #[test]
    fn status_change_flashes_for_0_8_seconds() {
        let (mut a, t0) = (TreeAnim::default(), Instant::now());
        a.sync_with(&groups(&["a"]), &done("a"), t0, false);
        assert_eq!(a.snapshot_with(t0, false).of("a").flash, Some(1.));
        assert!(a.snapshot_with(t0 + Duration::from_millis(700), false).of("a").flash.is_some());
        assert!(!a.snapshot_with(t0 + Duration::from_millis(810), false).active());
    }

    #[test]
    fn waiting_pulses_instead_of_fading() {
        let (mut a, t0) = (TreeAnim::default(), Instant::now());
        let t = Transitions { waiting: ["a".to_string()].into(), done: HashSet::new() };
        a.sync_with(&groups(&["a"]), &t, t0, false);
        assert_eq!(a.snapshot_with(t0, false).of("a").flash, Some(0.), "Start: sin(0)");
    }

    #[test]
    fn cmd_state_reports_changes_only() {
        let mut a = TreeAnim::default();
        assert!(a.set_cmd(true));
        assert!(!a.set_cmd(true));
        assert!(a.set_cmd(false));
    }

    #[test]
    fn reduce_motion_shows_the_end_state_at_once() {
        let (mut a, t0) = (TreeAnim::default(), Instant::now());
        a.sync_with(&groups(&["a"]), &Transitions::default(), t0, false);
        a.sync_with(&groups(&["a", "b"]), &done("a"), t0, false);
        assert!(!a.snapshot_with(t0, true).active());
    }
}
