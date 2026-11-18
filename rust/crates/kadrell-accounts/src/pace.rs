//! Warnung vor zu schnellem 7-Tage-Verbrauch (Logik aus `AppDelegate.notifyPaceIfNeeded`).

use crate::usage::Usage;

/// Wann das 7-Tage-Limit bei diesem Tempo voll ist und wann der Reset kommt (Sekunden seit Epoche). Die
/// Oberfläche formuliert daraus: „Bei diesem Tempo ist das 7-Tage-Limit am <lockout> voll. Reset ist aber erst am
/// <resets>, bis dahin wärst du gesperrt.“
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PaceWarning {
    pub lockout: f64,
    pub resets: f64,
}

/// Latch, damit die Warnung nur einmal pro Überschreitung kommt statt bei jedem Poll.
#[derive(Debug, Default)]
pub struct PaceWatch {
    warned: bool,
}

impl PaceWatch {
    /// Nach jeder Messung aufrufen. `enabled` ist die Einstellung `autoswitch.paceWarning` (Default an). Liegt das
    /// 7-Tage-Limit über dem gleichmäßigen Wochen-Plan und erreicht bei diesem Tempo vor dem Reset 100 %, droht ein
    /// früher Lockout: dann einmal `Some`. Fällt die Bedingung weg, wird der Latch zurückgesetzt.
    pub fn check(&mut self, usage: &Usage, enabled: bool, now: f64) -> Option<PaceWarning> {
        let warning = enabled
            .then(|| usage.weekly_lockout(now).zip(usage.weekly_resets))
            .flatten();
        let Some((lockout, resets)) = warning else {
            self.warned = false;
            return None;
        };
        if self.warned {
            return None;
        }
        self.warned = true;
        Some(PaceWarning { lockout, resets })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fast() -> Usage {
        Usage {
            weekly: Some(80),
            weekly_resets: Some(1000.0 + 84.0 * 3600.0),
            ..Usage::default()
        }
    }

    #[test]
    fn warns_once_per_crossing_and_rearms() {
        let mut w = PaceWatch::default();
        assert!(w.check(&fast(), true, 1000.0).is_some());
        assert!(
            w.check(&fast(), true, 1060.0).is_none(),
            "nicht bei jedem Poll"
        );
        let slow = Usage {
            weekly: Some(10),
            ..fast()
        };
        assert!(w.check(&slow, true, 1120.0).is_none());
        assert!(
            w.check(&fast(), true, 1180.0).is_some(),
            "neue Überschreitung warnt wieder"
        );
    }

    #[test]
    fn disabled_setting_never_warns() {
        let mut w = PaceWatch::default();
        assert!(w.check(&fast(), false, 1000.0).is_none());
    }
}
