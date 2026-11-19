//! Reine Logik der rechten Leistenseite: Farbe und Alter der Nutzungswerte, Text und Lampe des Status-Displays und
//! die Reihenfolge, in der die Leiste Platz schafft. Port von `StatusBarView.usageColor/staleMinutes`,
//! `StatusDisplay.swift` und `BarPlan.swift`. Gezeichnet wird in `ui::usage_bar`.

use kadrell_i18n::trd;

/// So viele Punkte vor dem Plan wird aus Grün Gelb (bei 7 Tagen rund 8 Stunden Vorlauf).
pub const USAGE_PLAN_WARNING: i32 = 5;
/// Ist die letzte Messung länger her (Sekunden), stehen die Werte grau da und der Tooltip nennt das Alter.
pub const USAGE_STALE_AFTER: f64 = 600.0;

/// Ampel eines Nutzungswerts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Good,
    Warn,
    Bad,
}

/// Farbe eines Nutzungswerts. Mit Plan (7 Tage) zählt der Vorsprung: über dem Plan rot, dicht darunter gelb, sonst
/// grün. Ohne Plan bleibt es beim absoluten Stand.
pub fn usage_tone(v: i32, plan: Option<i32>) -> Tone {
    match plan {
        None if v >= 90 => Tone::Bad,
        None if v >= 70 => Tone::Warn,
        None => Tone::Good,
        Some(p) if v > p => Tone::Bad,
        Some(p) if v >= p - USAGE_PLAN_WARNING => Tone::Warn,
        Some(_) => Tone::Good,
    }
}

/// Minuten seit der letzten Messung, wenn der Stand veraltet ist, sonst `None`.
pub fn stale_minutes(measured: Option<f64>, now: f64) -> Option<i64> {
    let age = now - measured?;
    (age > USAGE_STALE_AFTER).then_some((age / 60.0) as i64)
}

/// Die drei Werte der Leiste, in der Reihenfolge der Kürzel in `BarPlan`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UsageKey {
    Session,
    Weekly,
    Fable,
}

impl UsageKey {
    pub fn name(self) -> &'static str {
        match self {
            UsageKey::Session => "5h",
            UsageKey::Weekly => "7d",
            UsageKey::Fable => "fable",
        }
    }
}

/// Welche Werte die Einstellungen (`bar.usage.*`) zeigen; 5h zusätzlich nur mit 5-Stunden-Limit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UsageShown {
    pub session: bool,
    pub weekly: bool,
    pub fable: bool,
}

impl UsageShown {
    pub fn allows(self, key: UsageKey, has_session_limit: bool) -> bool {
        match key {
            UsageKey::Session => has_session_limit && self.session,
            UsageKey::Weekly => self.weekly,
            UsageKey::Fable => self.fable,
        }
    }
}

// MARK: Status-Display

/// Sessions je Status. `detached` heißt kein laufender Claude-Prozess, egal welcher Status zuletzt galt.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StatusCounts {
    pub running: usize,
    pub waiting: usize,
    pub idle: usize,
    pub error: usize,
    pub detached: usize,
}

impl StatusCounts {
    pub fn total(&self) -> usize {
        self.running + self.waiting + self.idle + self.error + self.detached
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lamp {
    Waiting,
    Error,
    Ok,
    Off,
}

/// Lampe nach Dringlichkeit: wartet etwas, ist es Gelb, sonst Rot bei Fehlern, sonst Grün, solange etwas lebt.
pub fn lamp(c: &StatusCounts) -> Lamp {
    if c.waiting > 0 {
        Lamp::Waiting
    } else if c.error > 0 {
        Lamp::Error
    } else if c.running + c.idle > 0 {
        Lamp::Ok
    } else {
        Lamp::Off
    }
}

/// Nur Zustände mit Sessions, in der Reihenfolge arbeiten, warten, Fehler, getrennt durch " · ", in Versalien.
/// Kompakt als Zahl mit Kürzel („2A · 1W“), wenn die Leiste für die lange Form zu schmal ist.
pub fn status_text(c: &StatusCounts, compact: bool) -> String {
    let parts = if compact { compact_parts(c) } else { long_parts(c) };
    parts.join(" · ")
}

fn long_parts(c: &StatusCounts) -> Vec<String> {
    let mut parts = Vec::new();
    if c.running > 0 {
        parts.push(trd!("%lld ARBEITEN", c.running));
    }
    if c.waiting > 0 {
        parts.push(trd!("%lld WARTEN", c.waiting));
    }
    if c.error > 0 {
        parts.push(trd!("%lld FEHLER", c.error));
    }
    parts
}

fn compact_parts(c: &StatusCounts) -> Vec<String> {
    let mut parts = Vec::new();
    if c.running > 0 {
        parts.push(trd!("%lldA", c.running));
    }
    if c.waiting > 0 {
        parts.push(trd!("%lldW", c.waiting));
    }
    if c.error > 0 {
        parts.push(trd!("%lldF", c.error));
    }
    parts
}

/// Langform aller Zustände für Tooltip und Bedienhilfe. Fehler stehen nur da, wenn es welche gibt.
pub fn status_spoken(c: &StatusCounts) -> String {
    let rows = [
        (c.running, trd!("arbeiten"), true),
        (c.waiting, trd!("warten"), true),
        (c.idle, trd!("fertig"), true),
        (c.error, trd!("mit Fehler"), false),
        (c.detached, trd!("getrennt"), true),
    ];
    let states: Vec<String> = rows.iter().filter(|(n, _, always)| *n > 0 || *always).map(|(n, label, _)| format!("{n} {label}")).collect();
    format!("{}{}", states.join(", "), trd!(" · %lld Sessions", c.total()))
}

// MARK: Platzplan

/// Was die rechte Seite der Leiste zeigt. Passt sie nicht neben die linken Tasten, fällt in fester Reihenfolge etwas
/// weg: erst die Einträge der Extensions (der linke zuerst), dann fable, 7d, der volle Kontoname, die Update-Taste, die
/// Versionswarnung, der Tipp und 5h. Dann nimmt das Display die Kurzform und wird notfalls mit „…“ gekürzt, Lampe und
/// Uhr bleiben. Reicht selbst das nicht, fällt als Letztes die Konto-Taste weg. Linke Tasten, „N warten“ und
/// „N trennen“ bleiben immer.
#[derive(Clone, Debug, PartialEq)]
pub struct BarPlan {
    /// So viele Einträge der Extensions bleiben, von rechts gezählt.
    pub extensions: usize,
    pub session: bool,
    pub weekly: bool,
    pub fable: bool,
    pub short_account: bool,
    pub account: bool,
    pub update: bool,
    pub version: bool,
    pub tip: bool,
    pub compact: bool,
    /// Um so viel (Pixel) wird der Text im Display gekürzt, wenn selbst die Kurzform nicht passt.
    pub shrink: f32,
}

impl BarPlan {
    pub fn new(extensions: usize) -> BarPlan {
        BarPlan {
            extensions,
            session: true,
            weekly: true,
            fable: true,
            short_account: false,
            account: true,
            update: true,
            version: true,
            tip: true,
            compact: false,
            shrink: 0.,
        }
    }

    pub fn shows(&self, key: UsageKey) -> bool {
        match key {
            UsageKey::Session => self.session,
            UsageKey::Weekly => self.weekly,
            UsageKey::Fable => self.fable,
        }
    }

    /// Schritte nach den Einträgen der Extensions, in der Reihenfolge, in der sie Platz schaffen.
    const STEPS: [fn(&mut BarPlan); 8] = [
        |p| p.fable = false,
        |p| p.weekly = false,
        |p| p.short_account = true,
        |p| p.update = false,
        |p| p.version = false,
        |p| p.tip = false,
        |p| p.session = false,
        |p| p.compact = true,
    ];

    /// Der vollständigste Plan, dessen rechte Seite (`width`) in `room` passt. Reicht auch die Kurzform nicht, kürzt
    /// `shrink` den Displaytext um den Rest.
    pub fn fit(extensions: usize, room: f32, width: impl Fn(&BarPlan) -> f32) -> BarPlan {
        let mut plan = BarPlan::new(extensions);
        while plan.extensions > 0 && width(&plan) > room {
            plan.extensions -= 1;
        }
        for step in Self::STEPS {
            if width(&plan) <= room {
                break;
            }
            step(&mut plan);
        }
        plan.shrink = (width(&plan) - room).max(0.);
        if width(&plan) <= room {
            return plan;
        }
        plan.account = false;
        plan.shrink = 0.;
        plan.shrink = (width(&plan) - room).max(0.);
        plan
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kadrell_i18n::{Language, set_thread_language};

    /// Die Sprache gilt nur für den Testthread, andere Tests des Prozesses bleiben unberührt.
    fn lang(language: Language) {
        set_thread_language(language);
    }

    fn counts(running: usize, waiting: usize, idle: usize, error: usize, detached: usize) -> StatusCounts {
        StatusCounts { running, waiting, idle, error, detached }
    }

    #[test]
    fn only_non_zero_states_in_order() {
        lang(Language::De);
        assert_eq!(status_text(&counts(2, 1, 3, 0, 1), false), "2 ARBEITEN · 1 WARTET");
        assert_eq!(status_text(&counts(1, 0, 0, 2, 0), false), "1 ARBEITET · 2 FEHLER");
        assert_eq!(status_text(&counts(0, 0, 0, 0, 0), false), "");
        assert_eq!(status_text(&counts(2, 1, 0, 0, 0), true), "2A · 1W");
    }

    #[test]
    fn display_switches_language() {
        let c = counts(2, 1, 0, 1, 0);
        lang(Language::En);
        assert_eq!(status_text(&c, false), "2 RUNNING · 1 WAITING · 1 ERROR");
        assert_eq!(status_text(&c, true), "2R · 1W · 1E");
        set_thread_language(Language::De);
        assert_eq!(status_text(&c, false), "2 ARBEITEN · 1 WARTET · 1 FEHLER");
        assert_eq!(status_text(&c, true), "2A · 1W · 1F");
    }

    #[test]
    fn english_plural() {
        lang(Language::En);
        assert_eq!(status_text(&counts(1, 2, 0, 2, 0), false), "1 RUNNING · 2 WAITING · 2 ERRORS");
    }

    #[test]
    fn spoken_lists_everything() {
        lang(Language::De);
        assert_eq!(status_spoken(&counts(2, 1, 3, 0, 1)), "2 arbeiten, 1 warten, 3 fertig, 1 getrennt · 7 Sessions");
        assert_eq!(status_spoken(&counts(0, 0, 1, 2, 0)), "0 arbeiten, 0 warten, 1 fertig, 2 mit Fehler, 0 getrennt · 3 Sessions");
    }

    #[test]
    fn lamp_priority() {
        assert_eq!(lamp(&counts(1, 1, 0, 1, 0)), Lamp::Waiting);
        assert_eq!(lamp(&counts(1, 0, 0, 1, 0)), Lamp::Error);
        assert_eq!(lamp(&counts(0, 0, 2, 0, 3)), Lamp::Ok);
        assert_eq!(lamp(&counts(0, 0, 0, 0, 3)), Lamp::Off);
    }

    /// Jeder Teil kostet 10, der kurze Kontoname 5, das Display 20 Text (Kurzform 10, `shrink` kürzt) plus 20 für
    /// Tasten, Lampe und Uhr.
    fn width(p: &BarPlan) -> f32 {
        let usage = [p.session, p.weekly, p.fable].iter().filter(|x| **x).count();
        let parts = p.extensions + usage + [p.update, p.version, p.tip].iter().filter(|x| **x).count();
        let account = if !p.account { 0. } else if p.short_account { 5. } else { 10. };
        parts as f32 * 10. + account + ((if p.compact { 10. } else { 20. }) - p.shrink).max(0.) + 20.
    }

    fn fit(room: f32) -> BarPlan {
        BarPlan::fit(2, room, width)
    }

    #[test]
    fn bar_plan_drop_order() {
        assert_eq!(fit(130.), BarPlan::new(2));
        assert_eq!(fit(120.).extensions, 1);
        assert_eq!(fit(110.), BarPlan::new(0));
        assert!(fit(100.).session && fit(100.).weekly && !fit(100.).fable);
        assert!(fit(90.).session && !fit(90.).weekly);
        assert!(fit(85.).short_account && fit(85.).update);
        assert!(!fit(75.).update && fit(75.).version);
        assert!(!fit(65.).version && fit(65.).tip);
        assert!(!fit(55.).tip && fit(55.).session);
        assert!(!fit(45.).session && !fit(45.).compact);
        assert!(fit(35.).compact && fit(35.).shrink == 0.);
        assert_eq!(fit(30.).shrink, 5.);
        assert!(fit(30.).account);
        let last = fit(20.);
        assert!(!last.account);
        assert_eq!(last.shrink, 10.);
        assert_eq!(width(&last), 20.);
    }

    #[test]
    fn color_follows_the_plan() {
        assert_eq!(usage_tone(32, Some(25)), Tone::Bad);
        assert_eq!(usage_tone(25, Some(25)), Tone::Warn);
        assert_eq!(usage_tone(21, Some(25)), Tone::Warn);
        assert_eq!(usage_tone(19, Some(25)), Tone::Good);
        assert_eq!(usage_tone(95, Some(96)), Tone::Warn);
        assert_eq!(usage_tone(95, None), Tone::Bad);
        assert_eq!(usage_tone(75, None), Tone::Warn);
        assert_eq!(usage_tone(10, None), Tone::Good);
    }

    #[test]
    fn stale_after_ten_minutes() {
        assert_eq!(stale_minutes(None, 1000.), None);
        assert_eq!(stale_minutes(Some(1000. - 600.), 1000.), None);
        assert_eq!(stale_minutes(Some(1000. - 1200.), 1000.), Some(20));
    }

    #[test]
    fn shown_follows_settings_and_limit() {
        let all = UsageShown { session: true, weekly: true, fable: true };
        assert!(all.allows(UsageKey::Session, true));
        assert!(!all.allows(UsageKey::Session, false), "nie bei Ultra");
        let none = UsageShown { session: false, weekly: false, fable: false };
        assert!(!none.allows(UsageKey::Weekly, true) && !none.allows(UsageKey::Fable, true));
    }

    /// Swift `testPlanTooltipIsTranslated`: der Tooltip mit Plan-Stand ist in beiden Sprachen übersetzt.
    #[test]
    fn plan_tooltip_is_translated() {
        lang(Language::En);
        assert_eq!(trd!("%@: %lld %% verbraucht, %lld %% nach Plan", "7d", 32, 25), "7d: 32% used, 25% on plan");
        assert_eq!(trd!("%@: %lld %% verbraucht", "7d", 32), "7d: 32% used");
        lang(Language::De);
        assert_eq!(trd!("%@: %lld %% verbraucht, %lld %% nach Plan", "7d", 32, 25), "7d: 32 % verbraucht, 25 % nach Plan");
    }

    /// Swift `testBarUsageFollowsSettings`: ausgeschaltete Werte fehlen, 5h zusätzlich ohne 5-Stunden-Limit.
    #[test]
    fn bar_usage_follows_settings() {
        let mut s = kadrell_core::Settings::default();
        s.set_bar_show_session(true);
        s.set_bar_show_weekly(true);
        s.set_bar_show_fable(false);
        let shown = |s: &kadrell_core::Settings| UsageShown { session: s.bar_show_session(), weekly: s.bar_show_weekly(), fable: s.bar_show_fable() };
        assert!(shown(&s).allows(UsageKey::Session, true));
        assert!(!shown(&s).allows(UsageKey::Session, false));
        assert!(shown(&s).allows(UsageKey::Weekly, true));
        assert!(!shown(&s).allows(UsageKey::Fable, true));
        s.set_bar_show_session(false);
        assert!(!shown(&s).allows(UsageKey::Session, true));
    }

    /// Ein falscher Schlüssel fiele still auf den deutschen Text zurück: jeder Text muss in beiden Sprachen auflösen.
    #[test]
    fn every_key_resolves() {
        lang(Language::En);
        assert_eq!(trd!("%lld trennen", 2), "detach 2");
        assert_eq!(trd!("%lld warten", 2), "2 waiting");
        assert_eq!(trd!("Aktives Konto"), "Active account");
        assert_eq!(trd!("Konto wählen"), "Choose account");
        assert_eq!(trd!("Automatisch wechseln bei Limit"), "Auto-switch at limit");
        assert_eq!(trd!("Fable-Kontingent"), "Fable quota");
        assert_eq!(trd!("Statistik"), "Stats");
        assert_eq!(trd!("Nachrichten geschickt"), "Messages sent");
        assert_eq!(trd!("%lld Prompts", 3), "3 prompts");
        assert_eq!(trd!("Esc oder %@ schließen", "Alt+F3"), "Esc or Alt+F3 closes");
        assert_eq!(trd!("Account automatisch gewechselt"), "Account switched automatically");
        assert_eq!(trd!("Nicht mehr anzeigen"), "Don’t show again");
    }
}
