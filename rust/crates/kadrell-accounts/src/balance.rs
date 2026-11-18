//! Zielwahl des Auto-Wechsels (rein, testbar). Port der statischen Funktionen des `AccountService`.
//!
//! Verteilt wird nur nach Wochentempo: mit `on_pace` immer auf das Konto, das spürbar mehr Vorrat zu seinem
//! Wochen-Soll hat. Die Füllstand-Schwelle ist nur das Netz kurz vor der Wand. Ein 5-Stunden-Wert zählt nur bei
//! Konten, die das Limit noch haben.

use crate::account::Account;
use crate::usage::Usage;

/// Marge in Prozentpunkten, um die ein Kandidat den aktiven Account unterbieten muss, damit gewechselt wird.
/// Verhindert das Hin-und-Her, wenn zwei Konten ähnlich voll sind.
pub const HYSTERESIS: i32 = 10;

/// Regeln der Zielwahl: Füllstand-Schwelle, Verteilung nach Wochentempo, Hysterese-Marge.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SwitchPolicy {
    pub threshold: i32,
    pub on_pace: bool,
    pub hysteresis: i32,
}

/// Ziel des Auto-Wechsels, `None` heißt kein Wechsel.
pub fn switch_target(
    usage: &Usage,
    active: Option<&Account>,
    among: &[Account],
    active_id: &str,
    policy: SwitchPolicy,
    now: f64,
) -> Option<String> {
    let SwitchPolicy {
        threshold,
        on_pace,
        hysteresis,
    } = policy;
    // Ohne bekannten aktiven Account gilt der 5h-Wert wie bisher; `binding_session` gibt bei Ultra bewusst None zurück.
    let session = match active {
        Some(a) => a.binding_session(usage.session),
        None => usage.session,
    };
    let used = session.unwrap_or(0).max(usage.weekly.unwrap_or(0));
    if used >= threshold
        && let Some(t) = best_target(among, active_id, used, threshold, hysteresis, now)
    {
        return Some(t);
    }
    if !on_pace {
        return None;
    }
    let headroom = usage.weekly_plan(now).unwrap_or(100) - usage.weekly.unwrap_or(0);
    best_pace_target(among, active_id, headroom, threshold, hysteresis, now)
}

/// Unter den Konten mit noch freiem 5-Stunden-Fenster das mit dem meisten 7-Tage-Pace-Vorrat, aber nur wenn es den
/// aktiven um die Hysterese-Marge an Gesamtauslastung unterbietet. Sind alle etwa gleich voll oder alle beim
/// 5-Stunden-Limit, kommt `None` zurück (kein Toggeln, kein Leerlauf).
pub fn best_target(
    among: &[Account],
    active_id: &str,
    active_used: i32,
    threshold: i32,
    hysteresis: i32,
    now: f64,
) -> Option<String> {
    let best = best_candidate(among, active_id, threshold, now)?;
    (cached_used(best, now) <= active_used - hysteresis).then(|| best.id.clone())
}

/// Wie [`best_target`], aber für den Pace-Fall: Ziel ist das Konto mit dem meisten Wochen-Runway, gewählt nur wenn
/// es den aktiven um die Hysterese-Marge an Pace-Vorrat übertrifft. So verteilt sich die Woche auf alle Konten,
/// auch wenn keines die Schwelle erreicht.
pub fn best_pace_target(
    among: &[Account],
    active_id: &str,
    active_headroom: i32,
    threshold: i32,
    hysteresis: i32,
    now: f64,
) -> Option<String> {
    let best = best_candidate(among, active_id, threshold, now)?;
    (pace_headroom(best, now) >= active_headroom + hysteresis).then(|| best.id.clone())
}

/// Kandidat mit dem größten Pace-Vorrat unter den anderen Konten, deren 5-Stunden-Fenster unter der Schwelle liegt.
/// Bei Gleichstand gewinnt wie bei Swifts `max(by:)` der erste in der Liste.
fn best_candidate<'a>(
    among: &'a [Account],
    active_id: &str,
    threshold: i32,
    now: f64,
) -> Option<&'a Account> {
    let mut best: Option<&Account> = None;
    for a in among
        .iter()
        .filter(|a| a.id != active_id && session_used(a, now) < threshold)
    {
        if best.is_none_or(|b| pace_headroom(b, now) < pace_headroom(a, now)) {
            best = Some(a);
        }
    }
    best
}

/// 7-Tage-Pace-Vorrat: um wie viele Prozentpunkte das Konto unter dem Soll-Stand liegt (Soll = verstrichener
/// Wochenanteil). Höher = mehr Runway durch die Woche. Nie gesehen zählt als voller Vorrat.
pub fn pace_headroom(a: &Account, now: f64) -> i32 {
    match a.usage.as_ref().map(|u| u.as_usage(now)) {
        Some(u) => u.weekly_plan(now).unwrap_or(100) - u.weekly.unwrap_or(0),
        None => 100,
    }
}

/// 5-Stunden-Auslastung, reset-bereinigt und nur, wo das Limit gilt; nie gesehen zählt als leer.
pub fn session_used(a: &Account, now: f64) -> i32 {
    a.binding_session(a.usage.as_ref().and_then(|u| u.as_usage(now).session))
        .unwrap_or(0)
}

/// Gecachte Gesamtauslastung (höherer von 5 h und 7 Tagen), reset-bereinigt; nie gesehen zählt als leer.
pub fn cached_used(a: &Account, now: f64) -> i32 {
    session_used(a, now).max(a.usage.as_ref().and_then(|u| u.weekly_at(now)).unwrap_or(0))
}

/// Fällig, wenn weder ein Stand noch ein Versuch jünger als `max_age` Sekunden ist.
pub fn is_due(a: &Account, last_poll: Option<f64>, max_age: f64, now: f64) -> bool {
    let last = a
        .usage
        .as_ref()
        .map_or(f64::NEG_INFINITY, |u| u.at)
        .max(last_poll.unwrap_or(f64::NEG_INFINITY));
    now - last >= max_age
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account::CachedUsage;

    const NOW: f64 = 1_000_000.0;
    const ULTRA: &str = "default_claude_max_20x";

    /// Konto mit gesetztem Stand; Fenster resetten in der Zukunft, damit die Werte nicht reset-bereinigt auf 0 fallen.
    fn acct(
        id: &str,
        session: Option<i32>,
        weekly: Option<i32>,
        weekly_resets_in_hours: f64,
    ) -> Account {
        let mut a = Account::new(id, id);
        a.usage = Some(CachedUsage {
            session,
            weekly,
            fable: None,
            session_resets: Some(NOW + 3600.0),
            weekly_resets: Some(NOW + weekly_resets_in_hours * 3600.0),
            at: NOW,
        });
        a
    }

    fn with_tier(mut a: Account, tier: &str) -> Account {
        a.tier = Some(tier.into());
        a
    }

    /// Reset in 84 h ergibt ein Wochen-Soll von 50 %.
    fn target(accounts: &[Account], active_used: i32) -> Option<String> {
        best_target(accounts, "live", active_used, 90, 10, NOW)
    }

    /// Bei gleichem Reset zählt nicht die niedrigste Auslastung, sondern der größte Abstand zum Wochen-Soll.
    #[test]
    fn target_prefers_more_weekly_pace_headroom() {
        let accounts = [
            acct("live", Some(95), Some(30), 84.0),
            acct("x", Some(10), Some(10), 84.0), // Soll 50, Vorrat 40
            acct("y", Some(10), Some(45), 84.0), // Soll 50, Vorrat 5
        ];
        assert_eq!(target(&accounts, 95).as_deref(), Some("x"));
    }

    /// Ein Konto mit mehr 7d-Vorrat, aber vollem 5-Stunden-Fenster, bringt sofort nichts und wird übersprungen.
    #[test]
    fn target_skips_accounts_with_full_session() {
        let accounts = [
            acct("live", Some(95), Some(30), 84.0),
            acct("full5h", Some(99), Some(5), 84.0), // bester 7d-Vorrat, aber 5h voll
            acct("free", Some(10), Some(40), 84.0),
        ];
        assert_eq!(target(&accounts, 95).as_deref(), Some("free"));
    }

    /// Ist der beste Kandidat kaum leerer als der aktive, bleibt es beim aktiven (Hysterese, kein Flattern).
    #[test]
    fn target_none_when_no_real_improvement() {
        let accounts = [
            acct("live", Some(95), Some(30), 84.0),
            acct("almostfull", Some(88), Some(30), 84.0),
        ];
        assert_eq!(target(&accounts, 95), None);
    }

    fn pace_target(accounts: &[Account], active_headroom: i32) -> Option<String> {
        best_pace_target(accounts, "live", active_headroom, 90, 10, NOW)
    }

    /// Aktiver Account brennt über Plan, ein Konto mit klar mehr Wochen-Vorrat wird gewählt, obwohl dessen Füllstand
    /// die Schwelle nie erreicht.
    #[test]
    fn pace_target_switches_to_more_runway() {
        let live = acct("live", Some(40), Some(60), 84.0);
        let accounts = [live.clone(), acct("x", Some(10), Some(10), 84.0)];
        assert_eq!(
            pace_target(&accounts, pace_headroom(&live, NOW)).as_deref(),
            Some("x")
        );
    }

    /// Ist der beste Kandidat nicht spürbar entspannter als der aktive, bleibt es beim aktiven (kein Flattern).
    #[test]
    fn pace_target_none_when_no_real_improvement() {
        let live = acct("live", Some(40), Some(60), 84.0);
        let accounts = [live.clone(), acct("barely", Some(10), Some(55), 84.0)];
        assert_eq!(pace_target(&accounts, pace_headroom(&live, NOW)), None);
    }

    /// Ein noch nie gesehenes Konto (kein Stand) gilt als leer mit vollem Vorrat und kommt zuerst dran.
    #[test]
    fn target_prefers_fresh_account() {
        let accounts = [
            acct("live", Some(95), Some(30), 84.0),
            acct("seen", Some(10), Some(45), 84.0),
            Account::new("fresh", "fresh"),
        ];
        assert_eq!(target(&accounts, 95).as_deref(), Some("fresh"));
    }

    // MARK: Verteilung nach Wochentempo, Ultra ohne 5-Stunden-Limit

    /// Live-Stand des aktiven Kontos, Wochen-Reset in 84 h (Soll 50 %).
    fn live(session: Option<i32>, weekly: Option<i32>) -> Usage {
        Usage {
            session,
            weekly,
            fable: None,
            session_resets: Some(NOW + 3600.0),
            weekly_resets: Some(NOW + 84.0 * 3600.0),
        }
    }

    fn switch(
        usage: &Usage,
        active: &Account,
        others: &[Account],
        on_pace: bool,
    ) -> Option<String> {
        let mut all = vec![active.clone()];
        all.extend_from_slice(others);
        switch_target(
            usage,
            Some(active),
            &all,
            &active.id,
            SwitchPolicy {
                threshold: 90,
                on_pace,
                hysteresis: 10,
            },
            NOW,
        )
    }

    /// Ultra hat kein 5-Stunden-Limit: ein voller 5h-Wert löst keinen Wechsel aus, solange das Tempo gleich ist.
    #[test]
    fn ultra_ignores_session_for_switch() {
        let ultra = with_tier(acct("live", Some(95), Some(30), 84.0), ULTRA);
        let other = acct("x", Some(10), Some(30), 84.0);
        assert_eq!(
            switch(&live(Some(95), Some(30)), &ultra, &[other], true),
            None
        );
    }

    /// Konten mit 5-Stunden-Limit wechseln weiter, bevor das Fenster voll ist.
    #[test]
    fn session_limited_account_still_switches_at_threshold() {
        let max = with_tier(
            acct("live", Some(95), Some(30), 84.0),
            "default_claude_max_5x",
        );
        let other = acct("x", Some(10), Some(30), 84.0);
        assert_eq!(
            switch(&live(Some(95), Some(30)), &max, &[other], true).as_deref(),
            Some("x")
        );
    }

    /// Verteilt wird durchgehend nach Tempo, nicht erst, wenn der aktive Account über seinem Soll liegt.
    #[test]
    fn pace_balances_even_below_plan() {
        let active = acct("live", Some(5), Some(20), 84.0); // Soll 50, Vorrat 30
        let other = acct("x", Some(0), Some(0), 84.0); // Vorrat 50
        let usage = live(Some(5), Some(20));
        assert_eq!(
            switch(&usage, &active, std::slice::from_ref(&other), true).as_deref(),
            Some("x")
        );
        assert_eq!(switch(&usage, &active, &[other], false), None);
    }

    /// Ein Ultra-Ziel mit hohem 5h-Wert bleibt wählbar; nur ein volles Fenster (100 %) schließt es aus.
    #[test]
    fn ultra_target_blocked_only_by_full_session() {
        let active = acct("live", Some(5), Some(40), 84.0); // Vorrat 10
        let ultra = with_tier(acct("u", Some(99), Some(0), 84.0), ULTRA);
        let usage = live(Some(5), Some(40));
        assert_eq!(
            switch(&usage, &active, &[ultra], true).as_deref(),
            Some("u")
        );
        let blocked = with_tier(acct("u", Some(100), Some(0), 84.0), ULTRA);
        assert_eq!(switch(&usage, &active, &[blocked], true), None);
    }

    /// Fällig ist ein Konto erst, wenn weder sein Stand noch der letzte Versuch jünger als das Intervall ist.
    #[test]
    fn inactive_poll_dueness() {
        let seen = acct("seen", Some(10), Some(10), 84.0); // Stand von NOW
        assert!(!is_due(&seen, None, 1200.0, NOW + 600.0));
        assert!(is_due(&seen, None, 1200.0, NOW + 1200.0));
        assert!(
            !is_due(&seen, Some(NOW + 1000.0), 1200.0, NOW + 1300.0),
            "gescheiterter Versuch zählt mit"
        );
        assert!(is_due(&Account::new("n", "n"), None, 1200.0, NOW));
    }
}
