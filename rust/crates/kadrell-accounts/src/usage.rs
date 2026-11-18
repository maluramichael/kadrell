//! Claude-Nutzung (5 h, 7 Tage, Fable-Woche) von `api.anthropic.com/api/oauth/usage` (Port von `UsageService.swift`).
//!
//! Alles optional: ändert sich das Format oder fehlt der Token, bleiben die Werte `None` und die Leiste zeigt „–%“.

use std::collections::HashMap;
use std::time::Duration;

use chrono::{DateTime, Datelike, TimeZone, Timelike};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::http::{HttpClient, HttpRequest, Method};
use crate::oauth::{self, USER_AGENT};
use crate::secret::{LIVE_SERVICE, SecretStore};
use crate::text::Lang;

const DAY: f64 = 24.0 * 3600.0;
const WINDOW: f64 = 7.0 * DAY;
pub(crate) const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
pub(crate) const BETA: &str = "oauth-2025-04-20";

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Usage {
    pub session: Option<i32>,
    pub weekly: Option<i32>,
    pub fable: Option<i32>,
    pub session_resets: Option<f64>,
    pub weekly_resets: Option<f64>,
}

impl Usage {
    pub fn is_empty(&self) -> bool {
        *self == Usage::default()
    }

    /// Der Stand, den man bei gleichmäßiger Verteilung über die Woche gerade haben dürfte, tageweise hochgerechnet:
    /// Tag 1 gibt gleich nach dem Reset rund 100/7 % frei, jeder weitere angebrochene Tag rund 100/7 % mehr, der
    /// 7. Tag 100 %. Bewusst nicht sekündlich, sonst stünde direkt nach dem Reset 0 % und jede Nutzung wäre „über
    /// Plan“. Die Fensterlänge liefert der Endpunkt nicht mit, nur `resets_at`, sie ist hier 7 Tage. `None` ohne Reset.
    pub fn weekly_plan(&self, now: f64) -> Option<i32> {
        let resets = self.weekly_resets?;
        let elapsed = (WINDOW - (resets - now)).clamp(0.0, WINDOW);
        let days_elapsed = (elapsed / DAY).floor();
        Some((((days_elapsed + 1.0) * 100.0 / 7.0) as i32).min(100))
    }

    /// Bei gleichbleibendem Tempo hochgerechnet: Zeitpunkt, an dem das 7-Tage-Limit 100 % erreicht. `None`, solange
    /// man nicht über dem tageweisen Plan liegt (sonst schlüge die Warnung gleich nach dem Reset an), und `None`, wenn
    /// die Hochrechnung erst am oder nach dem Reset läge: dann läuft man nicht früher in die Wand.
    pub fn weekly_lockout(&self, now: f64) -> Option<f64> {
        let (resets, weekly) = (self.weekly_resets?, self.weekly?);
        if weekly <= self.weekly_plan(now)? {
            return None;
        }
        let elapsed = WINDOW - (resets - now);
        if elapsed <= 0.0 {
            return None;
        }
        let rate_per_second = f64::from(weekly) / elapsed;
        let lockout = now + f64::from(100 - weekly) / rate_per_second;
        (lockout < resets).then_some(lockout)
    }

    /// Bevorzugt das `limits`-Array (kind session / weekly_all / weekly_scoped mit Modellname),
    /// fällt auf `five_hour` / `seven_day` zurück.
    pub fn parse(data: &[u8]) -> Usage {
        let Ok(Value::Object(root)) = serde_json::from_slice::<Value>(data) else {
            return Usage::default();
        };
        let mut u = Usage::default();
        if let Some(limits) = root.get("limits").and_then(Value::as_array) {
            for l in limits {
                u.apply_limit(l);
            }
        }
        u.apply_fallbacks(&root);
        u
    }

    fn apply_limit(&mut self, l: &Value) {
        let kind = l.get("kind").and_then(Value::as_str).unwrap_or("");
        let p = pct(l.get("percent"));
        match kind {
            "session" => {
                self.session = p;
                self.session_resets = date(l.get("resets_at"));
            }
            "weekly_all" => {
                self.weekly = p;
                self.weekly_resets = date(l.get("resets_at"));
            }
            "weekly_scoped" => {
                let name = l
                    .pointer("/scope/model/display_name")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                if self.fable.is_none() || name.to_lowercase().contains("fable") {
                    self.fable = p;
                }
            }
            _ => {}
        }
    }

    fn apply_fallbacks(&mut self, root: &serde_json::Map<String, Value>) {
        if self.session.is_none()
            && let Some(f) = root.get("five_hour")
        {
            self.session = pct(f.get("utilization"));
            self.session_resets = date(f.get("resets_at"));
        }
        if self.weekly.is_none()
            && let Some(s) = root.get("seven_day")
        {
            self.weekly = pct(s.get("utilization"));
            self.weekly_resets = date(s.get("resets_at"));
        }
        if self.fable.is_none()
            && let Some(o) = root.get("seven_day_opus")
        {
            self.fable = pct(o.get("utilization"));
        }
    }
}

fn pct(v: Option<&Value>) -> Option<i32> {
    let n = v?.as_number()?;
    match n.as_i64() {
        Some(i) => i32::try_from(i).ok(),
        None => n.as_f64().map(|d| d.round() as i32),
    }
}

/// ISO 8601 mit oder ohne Sekundenbruchteile.
fn date(v: Option<&Value>) -> Option<f64> {
    let dt = DateTime::parse_from_rfc3339(v?.as_str()?).ok()?;
    Some(dt.timestamp() as f64 + f64::from(dt.timestamp_subsec_nanos()) / 1e9)
}

/// Reset-Zeitpunkt in der Zeitzone `tz`, 24 Stunden: heute nur die Uhrzeit („14:00“), sonst mit Wochentag
/// („Do., 14:00“ bzw. „Thu 14:00“).
pub fn reset_clock<Tz: TimeZone>(at: f64, now: f64, tz: &Tz, lang: Lang) -> String {
    let local = |t: f64| tz.timestamp_opt(t.floor() as i64, 0).single();
    let (Some(d), Some(n)) = (local(at), local(now)) else {
        return String::new();
    };
    let clock = format!("{:02}:{:02}", d.hour(), d.minute());
    if d.date_naive() == n.date_naive() {
        clock
    } else {
        lang.weekday_clock(d.weekday().num_days_from_monday(), &clock)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum FetchResult {
    Ok(Usage),
    /// Mit `Retry-After` in Sekunden, wenn der Header kam.
    RateLimited(Option<f64>),
    Failed,
}

/// OAuth-Token des live angemeldeten Kontos aus dem Eintrag „Claude Code-credentials“. Der Eintrag ist ein einziges
/// JSON, nur `accessToken` wird gelesen, der Rest verworfen. Der Token wird nie geloggt.
pub fn live_token(secrets: &dyn SecretStore) -> Option<String> {
    let Some(raw) = secrets.read(LIVE_SERVICE, None) else {
        log::warn!("usage: kein Live-Eintrag lesbar");
        return None;
    };
    let token =
        oauth::oauth_from(&raw).and_then(|o| oauth::text(&o, "accessToken").map(str::to_string));
    if token.as_deref().is_none_or(str::is_empty) {
        log::warn!("usage: Live-Eintrag ohne accessToken");
        return None;
    }
    token
}

fn bearer_request(url: &str, token: &str, timeout_secs: u64) -> HttpRequest {
    HttpRequest {
        method: Method::Get,
        url: url.into(),
        headers: vec![
            ("Authorization".into(), format!("Bearer {token}")),
            ("anthropic-beta".into(), BETA.into()),
            ("User-Agent".into(), USER_AGENT.into()),
        ],
        body: None,
        timeout: Duration::from_secs(timeout_secs),
    }
}

pub(crate) fn profile_request(token: &str) -> HttpRequest {
    bearer_request(crate::account::PROFILE_URL, token, 15)
}

/// Eine Abfrage mit dem übergebenen Token (live angemeldetes Konto oder Slot eines inaktiven, siehe `oauth`).
pub fn fetch(http: &dyn HttpClient, token: &str) -> FetchResult {
    let resp = match http.send(&bearer_request(USAGE_URL, token, 15)) {
        Ok(r) => r,
        Err(e) => {
            log::warn!("usage: {e}");
            return FetchResult::Failed;
        }
    };
    match resp.status {
        429 => FetchResult::RateLimited(
            resp.header("Retry-After")
                .and_then(|v| v.trim().parse::<f64>().ok()),
        ),
        200 => {
            let u = Usage::parse(&resp.body);
            log::info!(
                "usage: 5h {} 7d {} fable {}",
                show(u.session),
                show(u.weekly),
                show(u.fable)
            );
            FetchResult::Ok(u)
        }
        s => {
            log::warn!("usage: HTTP {s}");
            FetchResult::Failed
        }
    }
}

fn show(v: Option<i32>) -> i32 {
    v.unwrap_or(-1)
}

/// Wartezeit bis zur nächsten Abfrage (Sekunden). `Retry-After` gilt voll (höchstens zwei Stunden), nur die selbst
/// geschätzte Pause ist bei 15 min gedeckelt.
pub(crate) fn next_delay(after: &FetchResult, current: f64, interval: f64) -> f64 {
    match after {
        FetchResult::Ok(_) => interval,
        FetchResult::RateLimited(retry) => retry
            .unwrap_or_else(|| (current * 2.0).min(900.0))
            .max(interval)
            .min(7200.0),
        FetchResult::Failed => (current * 2.0).min(900.0),
    }
}

/// Ergebnis einer Messung für den Aufrufer (Leiste, Verlauf, Auto-Wechsel).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Outcome {
    /// Wartezeit bis zur nächsten Abfrage in Sekunden.
    pub delay: f64,
    /// Erfolgreich gemessen (`onMeasured` in Swift, auch bei unveränderten Werten): die Leiste zeigt daran Frische.
    pub measured: Option<Usage>,
    /// Die Werte haben sich gegenüber dem letzten Stand geändert (`onChange`, Auslöser für den Auto-Wechsel).
    pub changed: bool,
}

/// Nutzung des aktiven Kontos. Zustand und Taktung, die Abfrage selbst macht [`UsageService::tick`].
#[derive(Debug)]
pub struct UsageService {
    usage: Usage,
    measured_at: Option<f64>,
    seeded_at: Option<f64>,
    /// Bis wann der Endpunkt je Konto-Id sperrt (429). Davor wird nicht gefragt.
    backoff: HashMap<String, f64>,
    interval: f64,
    delay: f64,
    next_poll: f64,
}

impl UsageService {
    /// `interval` in Sekunden (Swift: 300).
    pub fn new(interval: f64) -> Self {
        Self {
            usage: Usage::default(),
            measured_at: None,
            seeded_at: None,
            backoff: HashMap::new(),
            interval,
            delay: interval,
            next_poll: 0.0,
        }
    }

    pub fn usage(&self) -> &Usage {
        &self.usage
    }

    /// Zeitpunkt der letzten erfolgreichen Abfrage (beim Vorbelegen der des Caches).
    pub fn measured_at(&self) -> Option<f64> {
        self.measured_at
    }

    /// Beim Start mit dem letzten bekannten Stand (persistierter Account-Cache) vorbelegen, damit die Leiste nicht
    /// bei „–%“ steht, bis der erste Poll durch ist. Löst bewusst keine Änderung aus (kein Auto-Wechsel auf
    /// veraltete Werte). Greift nur, solange noch kein Live-Stand da ist.
    pub fn seed(&mut self, u: Usage, at: f64) {
        if !self.usage.is_empty() {
            return;
        }
        self.usage = u;
        self.seeded_at = Some(at);
        self.measured_at = Some(at);
    }

    /// Nach einem Account-Wechsel den Stand des neuen Kontos zeigen (leer, wenn nie einer gemessen wurde), statt die
    /// Werte des alten stehen zu lassen, bis die nächste Abfrage durchkommt.
    pub fn replace(&mut self, u: Usage, at: Option<f64>) {
        self.usage = u;
        self.measured_at = at;
    }

    /// Startet die Taktung. Ist der geseedete Stand noch frisch, wird der erste Poll bis zum Ende des Intervalls
    /// aufgeschoben: sonst hämmert jeder (Debug-)Neustart sofort gegen den streng limitierten Endpunkt.
    pub fn start(&mut self, now: f64) {
        self.delay = self.interval;
        self.next_poll = match self.seeded_at {
            Some(at) if at + self.interval > now => at + self.interval,
            _ => now,
        };
    }

    /// Wann der nächste Poll fällig ist (Sekunden seit Epoche).
    pub fn next_poll_at(&self) -> f64 {
        self.next_poll
    }

    /// Ruft der Aufrufer regelmäßig: fragt ab, sobald der Poll fällig ist. `account` ist die Id des live
    /// angemeldeten Kontos (`None`: keines hinterlegt).
    pub fn tick(
        &mut self,
        now: f64,
        account: Option<&str>,
        http: &dyn HttpClient,
        secrets: &dyn SecretStore,
    ) -> Option<Outcome> {
        if now < self.next_poll {
            return None;
        }
        let out = self.measure(now, account, http, secrets, self.delay);
        self.delay = out.delay;
        self.next_poll = now + out.delay;
        Some(out)
    }

    /// Sofort einmal abfragen, etwa direkt nach einem Wechsel, ohne den Poll-Takt zu stören.
    pub fn refresh_now(
        &mut self,
        now: f64,
        account: Option<&str>,
        http: &dyn HttpClient,
        secrets: &dyn SecretStore,
    ) -> Outcome {
        self.measure(now, account, http, secrets, 300.0)
    }

    fn measure(
        &mut self,
        now: f64,
        account: Option<&str>,
        http: &dyn HttpClient,
        secrets: &dyn SecretStore,
        delay: f64,
    ) -> Outcome {
        if let Some(wait) = self.backoff_left(account, now) {
            return Outcome {
                delay: wait,
                ..Outcome::default()
            };
        }
        let result = match live_token(secrets) {
            Some(token) => fetch(http, &token),
            None => FetchResult::Failed,
        };
        self.finish(now, account, account, result, delay)
    }

    fn backoff_left(&self, account: Option<&str>, now: f64) -> Option<f64> {
        self.backoff
            .get(account.unwrap_or(""))
            .filter(|until| **until > now)
            .map(|until| until - now)
    }

    /// Wertet ein Ergebnis aus. Gehört es einem anderen Konto als dem jetzt aktiven (`started_for` ≠ `current`),
    /// wird es verworfen, sonst landete es im Cache des neuen. Öffentlich für Aufrufer, die die Abfrage selbst
    /// auf einem anderen Thread machen.
    pub fn finish(
        &mut self,
        now: f64,
        started_for: Option<&str>,
        current: Option<&str>,
        result: FetchResult,
        delay: f64,
    ) -> Outcome {
        if started_for != current {
            return Outcome {
                delay: self.interval,
                ..Outcome::default()
            };
        }
        let key = started_for.unwrap_or("").to_string();
        let next = next_delay(&result, delay, self.interval);
        let mut out = Outcome {
            delay: next,
            ..Outcome::default()
        };
        match result {
            FetchResult::Ok(fresh) => {
                self.backoff.remove(&key);
                out.changed = fresh != self.usage;
                self.usage = fresh.clone();
                self.measured_at = Some(now);
                out.measured = Some(fresh);
            }
            FetchResult::RateLimited(_) => {
                self.backoff.insert(key, now + next);
                log::warn!("usage: 429, nächster Versuch in {} s", next as i64);
            }
            FetchResult::Failed => {}
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secret::MemorySecretStore;
    use crate::testing::FakeHttp;
    use chrono::Utc;

    // MARK: UsageBackoffTests

    #[test]
    fn retry_after_honored_beyond_fifteen_minutes() {
        assert_eq!(
            next_delay(&FetchResult::RateLimited(Some(1079.0)), 300.0, 300.0),
            1079.0
        );
    }

    #[test]
    fn retry_after_never_shorter_than_interval() {
        assert_eq!(
            next_delay(&FetchResult::RateLimited(Some(10.0)), 300.0, 300.0),
            300.0
        );
    }

    #[test]
    fn retry_after_capped_at_two_hours() {
        assert_eq!(
            next_delay(&FetchResult::RateLimited(Some(99_999.0)), 300.0, 300.0),
            7200.0
        );
    }

    #[test]
    fn without_retry_after_doubles_up_to_900() {
        assert_eq!(
            next_delay(&FetchResult::RateLimited(None), 300.0, 300.0),
            600.0
        );
        assert_eq!(
            next_delay(&FetchResult::RateLimited(None), 600.0, 300.0),
            900.0
        );
    }

    #[test]
    fn failure_doubles_and_success_resets() {
        assert_eq!(next_delay(&FetchResult::Failed, 600.0, 300.0), 900.0);
        assert_eq!(
            next_delay(&FetchResult::Ok(Usage::default()), 900.0, 300.0),
            300.0
        );
    }

    /// Nach einem Wechsel zeigt die Leiste den Stand des neuen Kontos, nicht den des alten.
    #[test]
    fn replace_overwrites_previous_accounts_usage() {
        let mut s = UsageService::new(300.0);
        s.seed(
            Usage {
                weekly: Some(99),
                ..Usage::default()
            },
            5.0,
        );
        s.replace(
            Usage {
                weekly: Some(12),
                ..Usage::default()
            },
            None,
        );
        assert_eq!(s.usage().weekly, Some(12));
        assert_eq!(s.measured_at(), None);
        s.replace(Usage::default(), None);
        assert!(s.usage().is_empty());
    }

    // MARK: Dienst

    const LIVE: &str = r#"{"claudeAiOauth":{"accessToken":"tok-live","refreshToken":"r"}}"#;

    fn secrets() -> MemorySecretStore {
        MemorySecretStore::new().with(LIVE_SERVICE, "user", LIVE)
    }

    #[test]
    fn rate_limit_blocks_until_retry_after_and_is_per_account() {
        let http = FakeHttp::status_with_header(429, "Retry-After", "1000");
        let mut s = UsageService::new(300.0);
        s.start(0.0);
        let out = s.tick(0.0, Some("a"), &http, &secrets()).unwrap();
        assert_eq!(out.delay, 1000.0);
        assert!(out.measured.is_none());
        assert_eq!(http.requests().len(), 1);
        assert_eq!(
            s.tick(10.0, Some("a"), &http, &secrets()),
            None,
            "Takt noch nicht fällig"
        );
        let blocked = s.refresh_now(10.0, Some("a"), &http, &secrets());
        assert_eq!(
            blocked.delay, 990.0,
            "Sperre gilt auch für die Sofort-Abfrage"
        );
        assert_eq!(http.requests().len(), 1, "während der Sperre kein Request");
        s.refresh_now(10.0, Some("b"), &http, &secrets());
        assert_eq!(http.requests().len(), 2, "anderes Konto hat eigene Sperre");
    }

    #[test]
    fn success_clears_backoff_and_reports_change_once() {
        let http = FakeHttp::json(
            200,
            r#"{"five_hour":{"utilization":10,"resets_at":"2030-01-01T00:00:00Z"},"seven_day":{"utilization":20}}"#,
        );
        let mut s = UsageService::new(300.0);
        let first = s.refresh_now(1.0, Some("a"), &http, &secrets());
        assert!(first.changed);
        assert_eq!(first.measured.as_ref().and_then(|u| u.weekly), Some(20));
        let second = s.refresh_now(2.0, Some("a"), &http, &secrets());
        assert!(!second.changed);
        assert!(
            second.measured.is_some(),
            "frisch gemessen, auch ohne Änderung"
        );
        assert_eq!(s.measured_at(), Some(2.0));
        let req = &http.requests()[0];
        assert_eq!(req.header("Authorization"), Some("Bearer tok-live"));
        assert_eq!(req.header("anthropic-beta"), Some("oauth-2025-04-20"));
        assert_eq!(req.url, "https://api.anthropic.com/api/oauth/usage");
    }

    #[test]
    fn result_for_switched_account_is_discarded() {
        let mut s = UsageService::new(300.0);
        let out = s.finish(
            1.0,
            Some("a"),
            Some("b"),
            FetchResult::Ok(Usage {
                weekly: Some(5),
                ..Usage::default()
            }),
            300.0,
        );
        assert_eq!(
            out,
            Outcome {
                delay: 300.0,
                measured: None,
                changed: false
            }
        );
        assert!(s.usage().is_empty());
    }

    #[test]
    fn missing_token_fails_without_request() {
        let http = FakeHttp::json(200, "{}");
        let mut s = UsageService::new(300.0);
        let out = s.refresh_now(1.0, None, &http, &MemorySecretStore::new());
        assert_eq!(out.delay, 600.0);
        assert!(http.requests().is_empty());
    }

    #[test]
    fn fresh_seed_postpones_first_poll() {
        let mut s = UsageService::new(300.0);
        s.seed(
            Usage {
                weekly: Some(1),
                ..Usage::default()
            },
            1000.0,
        );
        s.start(1100.0);
        assert_eq!(s.next_poll_at(), 1300.0);
        let mut old = UsageService::new(300.0);
        old.seed(
            Usage {
                weekly: Some(1),
                ..Usage::default()
            },
            0.0,
        );
        old.start(1100.0);
        assert_eq!(old.next_poll_at(), 1100.0);
    }

    // MARK: Parsen und Plan

    #[test]
    fn parse_prefers_limits_array_and_picks_fable() {
        let json = r#"{"limits":[{"kind":"session","percent":12.4,"resets_at":"2030-01-01T10:00:00.500Z"},
            {"kind":"weekly_all","percent":56,"resets_at":"2030-01-05T10:00:00Z"},
            {"kind":"weekly_scoped","percent":3,"scope":{"model":{"display_name":"Sonnet"}}},
            {"kind":"weekly_scoped","percent":19,"scope":{"model":{"display_name":"Claude Fable"}}}],
            "five_hour":{"utilization":99}}"#;
        let u = Usage::parse(json.as_bytes());
        assert_eq!(
            (u.session, u.weekly, u.fable),
            (Some(12), Some(56), Some(19))
        );
        assert_eq!(u.session_resets, Some(1_893_492_000.5));
    }

    #[test]
    fn parse_falls_back_to_legacy_fields_and_garbage_is_empty() {
        let u = Usage::parse(br#"{"five_hour":{"utilization":7.6},"seven_day":{"utilization":30},"seven_day_opus":{"utilization":4}}"#);
        assert_eq!((u.session, u.weekly, u.fable), (Some(8), Some(30), Some(4)));
        assert!(Usage::parse(b"nope").is_empty());
    }

    fn weekly(resets_in_hours: f64, weekly: i32) -> Usage {
        Usage {
            weekly: Some(weekly),
            weekly_resets: Some(1000.0 + resets_in_hours * 3600.0),
            ..Usage::default()
        }
    }

    #[test]
    fn weekly_plan_is_daywise() {
        assert_eq!(
            weekly(168.0, 0).weekly_plan(1000.0),
            Some(14),
            "gleich nach dem Reset: Tag 1"
        );
        assert_eq!(weekly(84.0, 0).weekly_plan(1000.0), Some(57), "Tag 4 von 7");
        assert_eq!(weekly(0.0, 0).weekly_plan(1000.0), Some(100));
        assert_eq!(Usage::default().weekly_plan(0.0), None);
    }

    #[test]
    fn weekly_lockout_only_when_ahead_of_plan_and_before_reset() {
        assert_eq!(weekly(84.0, 50).weekly_lockout(1000.0), None, "unter Plan");
        // 80 % nach 3,5 Tagen: 100 % nach weiteren 0,875 Tagen, vor dem Reset.
        let lock = weekly(84.0, 80).weekly_lockout(1000.0).unwrap();
        assert!((lock - (1000.0 + 0.25 * 3.5 * DAY)).abs() < 1.0, "{lock}");
        assert_eq!(
            weekly(1.0, 99).weekly_lockout(1000.0),
            None,
            "am 7. Tag liegt der Plan bei 100 %"
        );
        assert_eq!(
            Usage::default().weekly_lockout(1000.0),
            None,
            "ohne Reset keine Hochrechnung"
        );
    }

    #[test]
    fn reset_clock_shows_weekday_on_other_days() {
        let tz = Utc;
        let now = 1_800_000_000.0; // 2027-01-15 08:00 UTC, Freitag
        assert_eq!(reset_clock(now + 3600.0, now, &tz, Lang::De), "09:00");
        assert_eq!(
            reset_clock(now + 86_400.0, now, &tz, Lang::De),
            "Sa., 08:00"
        );
        assert_eq!(reset_clock(now + 86_400.0, now, &tz, Lang::En), "Sat 08:00");
    }
}
