//! Erfundene Konten, Nutzung, Messreihen und Prompts für Screenshots und Tests (`KADRELL_DEV_USAGE=fake`). Nichts
//! davon ist echt, die Adressen liegen unter `example.com`. Die Reihen sind rein rechnerisch (kein Zufall), damit zwei
//! Läufe dasselbe Bild liefern.

use kadrell_accounts::{Account, AccountsState, CachedUsage, PromptEvent, Usage, UsageSample};

const HOUR: f64 = 3600.;
const DAY: f64 = 24. * HOUR;

pub struct Fake {
    pub accounts: AccountsState,
    pub usage: Usage,
    pub samples: Vec<UsageSample>,
    pub events: Vec<PromptEvent>,
}

impl Fake {
    pub fn new(now: f64) -> Fake {
        let usage = Usage {
            session: None,
            weekly: Some(38),
            fable: Some(12),
            session_resets: None,
            weekly_resets: Some(now + 3.2 * DAY),
        };
        Fake { accounts: accounts(now, &usage), usage, samples: samples(now), events: events(now) }
    }
}

fn accounts(now: f64, usage: &Usage) -> AccountsState {
    let anna = Account {
        email: Some("anna@example.com".into()),
        subscription: Some("max".into()),
        tier: Some("default_claude_max_20x".into()),
        usage: Some(CachedUsage::from_usage(usage, now - 120.)),
        ..Account::new("a1", "")
    };
    let ben_usage = Usage { session: Some(41), weekly: Some(66), fable: Some(30), session_resets: Some(now + 2. * HOUR), weekly_resets: Some(now + 5. * DAY) };
    let ben = Account {
        email: Some("ben@example.org".into()),
        subscription: Some("max".into()),
        tier: Some("default_claude_max_5x".into()),
        usage: Some(CachedUsage::from_usage(&ben_usage, now - 900.)),
        ..Account::new("a2", "")
    };
    AccountsState { accounts: vec![anna, ben], active_id: Some("a1".into()) }
}

/// Dreißig Tage Messungen alle 90 Minuten: das Wochenlimit steigt sägezahnartig und fällt mit dem Reset, das
/// 5-Stunden-Limit (nur Ben) pulsiert, Fable folgt der Woche in kleinerem Maßstab. Eine Lücke von einem Tag zeigt
/// die gestrichelte Brücke.
fn samples(now: f64) -> Vec<UsageSample> {
    let mut out = Vec::new();
    let mut t = now - 30. * DAY;
    while t < now {
        let gap = (now - 12. * DAY..now - 11. * DAY).contains(&t);
        if !gap {
            out.push(sample(t, "a1", false));
            out.push(sample(t + 20., "a2", true));
        }
        t += 1.5 * HOUR;
    }
    out
}

fn sample(t: f64, account: &str, session: bool) -> UsageSample {
    let week = ((t / DAY) % 7.) / 7.;
    let bump = if account == "a1" { 0.0 } else { 0.18 };
    let weekly = ((week * 85. + 6. + bump * 100.) as i32).clamp(0, 100);
    let pulse = ((t / (5. * HOUR)) % 1.) * 90.;
    UsageSample {
        t,
        account: account.into(),
        session: session.then_some(pulse as i32),
        weekly: Some(weekly),
        fable: Some(weekly / 3),
    }
}

/// Prompts über ein Jahr, an Werktagen dichter, auf sieben Projekte und einen Rest verteilt.
fn events(now: f64) -> Vec<PromptEvent> {
    const PROJECTS: [(&str, u32); 9] = [
        ("kadrell", 9),
        ("lulububu", 6),
        ("malura-static", 3),
        ("exile-manager", 4),
        ("vault-notes", 2),
        ("dotfiles", 2),
        ("paperless-tools", 1),
        ("playground", 1),
        ("scratch", 1),
    ];
    let mut out = Vec::new();
    for day in 0..365u32 {
        let start = now - f64::from(day) * DAY;
        let weekday = (day + 3) % 7;
        let per_day = if weekday >= 5 { 2 } else { 9 + (day * 7) % 11 };
        for n in 0..per_day {
            let (project, weight) = PROJECTS[((day * 5 + n * 3) % PROJECTS.len() as u32) as usize];
            if (n + day) % 10 < weight {
                out.push(PromptEvent { time: start - f64::from(n) * 1.1 * HOUR - f64::from(day % 13), project: project.into() });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_is_invented_and_deterministic() {
        let (a, b) = (Fake::new(1_800_000_000.), Fake::new(1_800_000_000.));
        assert_eq!(a.samples, b.samples);
        assert_eq!(a.events, b.events);
        assert!(a.accounts.accounts.iter().all(|x| x.email.as_deref().unwrap().contains("example")));
        assert!(a.events.len() > 300 && a.samples.len() > 800);
    }
}
