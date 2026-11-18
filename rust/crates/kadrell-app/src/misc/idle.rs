//! Fertige Sessions automatisch trennen (Einstellung „Fertige Sessions automatisch trennen“, Swift:
//! `autoDetachIdleSessions` im AppDelegate). Was länger als die eingestellten Minuten fertig (grün) ist, verliert seinen
//! Prozess; die Kachel bleibt, ein Klick setzt die Konversation fort. Die gerade fokussierte Session bleibt verbunden.

use std::collections::HashMap;

/// Seit wann (Sekunden seit 1970) jede fertige Session fertig ist.
#[derive(Debug, Default)]
pub struct IdleTracker {
    since: HashMap<String, f64>,
}

impl IdleTracker {
    /// Ein Durchlauf (alle 30 Sekunden): `detachable` sind die Sessions, die jetzt trennbar wären. Gibt die zurück, die
    /// bei `minutes` > 0 fällig sind. Die Zeiten werden auch bei ausgeschalteter Einstellung (0) gepflegt, sonst
    /// trennt das Einschalten sofort alles.
    pub fn due(&mut self, detachable: &[String], focused: Option<&str>, now: f64, minutes: i64) -> Vec<String> {
        self.since.retain(|key, _| detachable.contains(key));
        for key in detachable {
            self.since.entry(key.clone()).or_insert(now);
        }
        if minutes <= 0 {
            return Vec::new();
        }
        let limit = minutes as f64 * 60.;
        detachable.iter().filter(|k| Some(k.as_str()) != focused && now - self.since.get(*k).copied().unwrap_or(now) >= limit).cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn nothing_is_due_before_the_minutes_are_over() {
        let mut t = IdleTracker::default();
        assert!(t.due(&keys(&["a"]), None, 1000., 5).is_empty());
        assert!(t.due(&keys(&["a"]), None, 1000. + 299., 5).is_empty());
        assert_eq!(t.due(&keys(&["a"]), None, 1000. + 300., 5), keys(&["a"]));
    }

    #[test]
    fn the_focused_session_stays_connected() {
        let mut t = IdleTracker::default();
        t.due(&keys(&["a", "b"]), Some("a"), 0., 1);
        assert_eq!(t.due(&keys(&["a", "b"]), Some("a"), 120., 1), keys(&["b"]));
    }

    #[test]
    fn switching_the_setting_on_does_not_detach_everything_at_once() {
        let mut t = IdleTracker::default();
        for minute in 0..10 {
            assert!(t.due(&keys(&["a"]), None, f64::from(minute) * 60., 0).is_empty(), "aus: nie fällig");
        }
        // Seit 9 Minuten fertig, Einstellung 5 Minuten: sofort fällig (die Uhr lief die ganze Zeit mit).
        assert_eq!(t.due(&keys(&["a"]), None, 600., 5), keys(&["a"]));
        // Eine neu fertig gewordene Session fängt dagegen bei null an.
        assert!(t.due(&keys(&["a", "n"]), None, 610., 5).iter().all(|k| k != "n"));
    }

    #[test]
    fn a_session_that_works_again_restarts_its_clock() {
        let mut t = IdleTracker::default();
        t.due(&keys(&["a"]), None, 0., 5);
        t.due(&[], None, 200., 5);
        assert!(t.due(&keys(&["a"]), None, 400., 5).is_empty(), "Uhr begann neu bei 400");
        assert_eq!(t.due(&keys(&["a"]), None, 700., 5), keys(&["a"]));
    }
}
