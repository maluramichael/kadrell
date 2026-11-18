//! Hochzählen der Nutzungswerte in der Leiste (Swift: `StatusBarView.usage` mit `usageFrom`/`usageAt`,
//! `Feedback.progress`): ändert sich ein Wert, zählt die Anzeige in 0,5 s vom alten zum neuen Stand, ease-out.
//! Reine Logik, die Leiste liest `Fade` und zeichnet den Zwischenstand.

use std::time::{Duration, Instant};

use kadrell_accounts::Usage;

/// So lange zählt ein neuer Wert hoch.
pub const DURATION: Duration = Duration::from_millis(500);

/// Zwischenstand einer laufenden Animation: der Wert davor und der Fortschritt (0 bis 1, schon abgebremst).
#[derive(Clone, Debug, PartialEq)]
pub struct Fade {
    pub from: Usage,
    pub progress: f32,
}

/// Merkt sich den letzten Stand und seit wann der aktuelle gilt.
#[derive(Default)]
pub struct CountUp {
    current: Usage,
    from: Usage,
    since: Option<Instant>,
}

impl CountUp {
    /// Neuer Stand aus dem Hub: bei einer Änderung beginnt eine Animation vom alten Stand aus.
    pub fn observe(&mut self, usage: &Usage, now: Instant) {
        if *usage != self.current {
            self.from = std::mem::replace(&mut self.current, usage.clone());
            self.since = Some(now);
        }
    }

    /// Der Zwischenstand, solange die Animation läuft. Bei „Bewegung reduzieren“ nie (`Feedback.progress`).
    pub fn fade(&self, now: Instant, reduce_motion: bool) -> Option<Fade> {
        if reduce_motion {
            return None;
        }
        let elapsed = now.checked_duration_since(self.since?)?;
        progress(elapsed).map(|progress| Fade { from: self.from.clone(), progress })
    }
}

/// Fortschritt nach `elapsed`: ease-out kubisch, `None` vor dem Start und nach dem Ende (`Feedback.progress`).
pub fn progress(elapsed: Duration) -> Option<f32> {
    let p = elapsed.as_secs_f32() / DURATION.as_secs_f32();
    (0.0..1.0).contains(&p).then(|| 1. - (1. - p).powi(3))
}

/// Was die Leiste zeigt: ohne Animation oder ohne Ausgangswert das Ziel, sonst der Zwischenstand.
pub fn shown(target: i32, from: Option<i32>, fade: Option<f32>) -> i32 {
    match (from, fade) {
        (Some(from), Some(p)) => from + ((target - from) as f32 * p).round() as i32,
        _ => target,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn usage(session: Option<i32>) -> Usage {
        Usage { session, ..Usage::default() }
    }

    #[test]
    fn progress_eases_out_and_ends_after_half_a_second() {
        assert_eq!(progress(Duration::ZERO), Some(0.));
        let half = progress(Duration::from_millis(250)).expect("läuft");
        assert!(half > 0.5 && half < 1., "ease-out liegt über der Mitte: {half}");
        assert_eq!(progress(DURATION), None);
    }

    #[test]
    fn shown_interpolates_between_the_old_and_the_new_value() {
        assert_eq!(shown(40, Some(20), Some(0.5)), 30);
        assert_eq!(shown(40, Some(20), None), 40);
        assert_eq!(shown(40, None, Some(0.5)), 40, "ohne Ausgangswert springt der Wert");
        assert_eq!(shown(10, Some(30), Some(0.5)), 20, "es zählt auch herunter");
    }

    #[test]
    fn a_change_starts_a_fade_from_the_old_state() {
        let t0 = Instant::now();
        let mut c = CountUp::default();
        c.observe(&usage(Some(10)), t0);
        c.observe(&usage(Some(10)), t0 + Duration::from_millis(100));
        let fade = c.fade(t0 + Duration::from_millis(100), false).expect("läuft");
        assert_eq!(fade.from, Usage::default());
        c.observe(&usage(Some(50)), t0 + Duration::from_secs(5));
        let fade = c.fade(t0 + Duration::from_secs(5), false).expect("läuft");
        assert_eq!(fade.from.session, Some(10));
        assert!(c.fade(t0 + Duration::from_secs(6), false).is_none(), "nach 0,5 s steht der Wert");
    }

    #[test]
    fn reduce_motion_skips_the_animation() {
        let t0 = Instant::now();
        let mut c = CountUp::default();
        c.observe(&usage(Some(10)), t0);
        assert!(c.fade(t0, true).is_none());
    }
}
