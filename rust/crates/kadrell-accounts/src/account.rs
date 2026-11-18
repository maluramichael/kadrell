//! Mehrere Claude-Konten in einem Fenster (Port von `AccountService.swift`): den einen Live-Eintrag
//! `Claude Code-credentials` gegen den gespeicherten Block eines anderen Kontos tauschen (Global-Swap). Alle
//! laufenden `claude`-Prozesse ziehen beim nächsten Token-Lesen (~30 s) nach. Nur der `claudeAiOauth`-Block wird
//! getauscht, `mcpOAuth` (MCP-Logins, konto-unabhängig) bleibt stehen.
//!
//! Der Dienst ist synchron: `&mut self` ersetzt das `exclusive`/`busy` aus Swift, der Aufrufer legt ihn hinter einen
//! Mutex und ruft ihn von einem Arbeitsthread. Rückmeldungen (Swift: Callbacks) kommen als [`AccountEvent`] über
//! [`AccountService::take_events`].

use std::collections::HashMap;
use std::sync::Arc;

use chrono::TimeZone;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::balance::{self, HYSTERESIS, SwitchPolicy};
use crate::history::UsageSample;
use crate::http::HttpClient;
use crate::oauth::{self, Obj};
use crate::secret::{LIVE_SERVICE, STORE_SERVICE, SecretStore};
use crate::stats_series::{LimitKind, LimitSource};
use crate::text::Lang;
use crate::usage::{self, FetchResult, Usage, reset_clock};

pub(crate) const PROFILE_URL: &str = "https://api.anthropic.com/api/oauth/profile";
/// Nach einem Auto-Wechsel so lange (Sekunden) nicht erneut wechseln, gegen Flattern nahe der Schwelle.
const COOLDOWN: f64 = 300.0;
/// So jung (Sekunden) muss der Stand eines Ziels sein, damit der Auto-Wechsel ihm traut. Älteres wird vorher live abgefragt.
const VERIFY_AGE: f64 = 120.0;
/// Verlauf ohne hinterlegtes Konto: Wer nur angemeldet ist und nie einen Slot angelegt hat, hat keine Id.
pub const NO_ACCOUNT_ID: &str = "current";

/// Zuletzt bekannter Nutzungsstand eines Kontos, gemerkt solange es aktiv war. So zeigt das Menü auch für gerade
/// nicht aktive Konten den letzten Stand mit Alter, ohne jedes Konto extra abzufragen.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CachedUsage {
    pub session: Option<i32>,
    pub weekly: Option<i32>,
    pub fable: Option<i32>,
    /// Wann die Fenster laut Endpoint zurücksetzen. Ist der Zeitpunkt seit der Messung vorbei, gilt die Dimension
    /// als wieder frei, ohne dass das inaktive Konto extra abgefragt werden muss.
    pub session_resets: Option<f64>,
    pub weekly_resets: Option<f64>,
    pub at: f64,
}

impl CachedUsage {
    pub fn session_at(&self, now: f64) -> Option<i32> {
        freed(self.session, self.session_resets, now)
    }

    pub fn weekly_at(&self, now: f64) -> Option<i32> {
        freed(self.weekly, self.weekly_resets, now)
    }

    /// Als Live-[`Usage`] zum Vorbelegen der Leiste beim Start, reset-bereinigt wie im Menü (Fable folgt dem
    /// 7-Tage-Reset).
    pub fn as_usage(&self, now: f64) -> Usage {
        Usage {
            session: self.session_at(now),
            weekly: self.weekly_at(now),
            fable: freed(self.fable, self.weekly_resets, now),
            session_resets: self.session_resets,
            weekly_resets: self.weekly_resets,
        }
    }

    pub fn from_usage(u: &Usage, now: f64) -> Self {
        Self {
            session: u.session,
            weekly: u.weekly,
            fable: u.fable,
            session_resets: u.session_resets,
            weekly_resets: u.weekly_resets,
            at: now,
        }
    }
}

fn freed(v: Option<i32>, resets: Option<f64>, now: f64) -> Option<i32> {
    match resets {
        Some(r) if r <= now => Some(0),
        _ => v,
    }
}

/// Ein hinterlegter Claude-Account. Das OAuth-Geheimnis (`claudeAiOauth`-Block) liegt im Secret-Speicher unter
/// `STORE_SERVICE`/`id`, hier stehen nur die anzeigbaren Metadaten.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Account {
    pub id: String,
    pub alias: String,
    pub email: Option<String>,
    pub subscription: Option<String>,
    pub usage: Option<CachedUsage>,
    /// `rateLimitTier` aus dem OAuth-Block, z. B. `default_claude_max_20x`. Fehlt bei Slots aus älteren Versionen,
    /// bis `load_tiers` sie nachträgt.
    #[serde(default)]
    pub tier: Option<String>,
}

impl Account {
    pub fn new(id: &str, alias: &str) -> Self {
        Self {
            id: id.into(),
            alias: alias.into(),
            email: None,
            subscription: None,
            usage: None,
            tier: None,
        }
    }

    /// Anzeigename: eigener Alias, sonst E-Mail, sonst ein neutraler Platzhalter.
    pub fn title(&self, lang: Lang) -> String {
        if !self.alias.is_empty() {
            return self.alias.clone();
        }
        match self.email.as_deref() {
            Some(e) if !e.is_empty() => e.to_string(),
            _ => lang.account().to_string(),
        }
    }

    /// Ultra hieß bis 30.10.2026 Max 20x, die Stufe heißt weiter `default_claude_max_20x`.
    fn is_ultra(&self) -> bool {
        let t = self.tier.as_deref().unwrap_or("").to_lowercase();
        t.contains("max_20x") || t.contains("ultra")
    }

    /// Seit 30.10.2026 hat Ultra kein 5-Stunden-Limit mehr, nur noch das Wochenlimit.
    pub fn has_session_limit(&self) -> bool {
        !self.is_ultra()
    }

    /// Planname wie bei Anthropic seit 30.10.2026 (Max 20x heißt Ultra, Max 5x heißt Max), sonst der Abo-Typ.
    pub fn plan_name(&self) -> Option<String> {
        if self.is_ultra() {
            return Some("Ultra".into());
        }
        if self
            .tier
            .as_deref()
            .is_some_and(|t| t.to_lowercase().contains("max_5x"))
        {
            return Some("Max".into());
        }
        self.subscription.as_deref().map(capitalized)
    }

    /// 5-Stunden-Wert, der für den Wechsel zählt. Ohne 5-Stunden-Limit nur ein volles Fenster: so bleibt ein noch
    /// nicht verifiziertes Ultra-Konto, das das Limit noch hat, nicht hängen.
    pub fn binding_session(&self, v: Option<i32>) -> Option<i32> {
        let v = v?;
        (self.has_session_limit() || v >= 100).then_some(v)
    }
}

fn capitalized(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// Was `kadrell-core` im Profil speichert: `accounts.index` (Liste) und `accounts.active` (Id des aktiven Kontos).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AccountsState {
    pub accounts: Vec<Account>,
    pub active_id: Option<String>,
}

/// Einstellungen, die der Dienst braucht (siehe Hinweis in `lib.rs`, woher sie in `kadrell-core` kommen).
#[derive(Clone, Debug, PartialEq)]
pub struct AccountSettings {
    pub autoswitch_enabled: bool,
    /// 50..=99
    pub autoswitch_threshold: i32,
    pub autoswitch_on_pace: bool,
    /// Temp-Profile (`--profile tmp`) teilen den Schlüsselbund mit dem eigenen Kadrell: dort nie rotieren und nie
    /// automatisch wechseln.
    pub temporary_profile: bool,
}

impl Default for AccountSettings {
    fn default() -> Self {
        Self {
            autoswitch_enabled: false,
            autoswitch_threshold: 90,
            autoswitch_on_pace: false,
            temporary_profile: false,
        }
    }
}

/// Rückmeldungen des Dienstes (in Swift die Callbacks `onChange`, `onActiveChange`, `onAutoSwitch`, `onUsageStored`).
#[derive(Clone, Debug, PartialEq)]
pub enum AccountEvent {
    /// Konten oder aktive Id haben sich geändert: `state()` neu speichern, Menü neu bauen.
    Changed,
    /// Nach jedem Wechsel auf ein anderes Konto (manuell oder automatisch), nicht beim Hinzufügen des angemeldeten.
    ActiveChanged,
    /// Nach einem automatischen Wechsel, mit dem jetzt aktiven Konto (für die Meldung an den Nutzer, Einstellung
    /// `autoswitch.notice`: „Jetzt aktiv: <Titel>. Grund: Nutzungslimit erreicht.“).
    AutoSwitched(Account),
    /// Nach jeder Messung eines Kontos, aktiv oder inaktiv (für den Verlauf der Limits, siehe `UsageHistory`).
    UsageStored(String, CachedUsage),
}

/// Eintrag der Konten-Auswahl in der Leiste.
#[derive(Clone, Debug, PartialEq)]
pub struct MenuItem {
    pub id: String,
    pub title: String,
    pub active: bool,
    pub detail: String,
}

/// E-Mail und aktuelle Stufe aus `api.anthropic.com/api/oauth/profile`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Profile {
    pub email: Option<String>,
    pub tier: Option<String>,
}

/// E-Mail und Stufe (`organization.rate_limit_tier`) aus der Profil-Antwort (rein, testbar).
pub fn parse_profile(root: &Obj) -> Profile {
    Profile {
        email: email_of(root),
        tier: tier_of(root),
    }
}

fn email_of(root: &Obj) -> Option<String> {
    let account = root.get("account").and_then(Value::as_object);
    let from_account = |k: &str| account.and_then(|a| a.get(k)).and_then(Value::as_str);
    from_account("email")
        .or_else(|| from_account("email_address"))
        .or_else(|| root.get("email").and_then(Value::as_str))
        .map(str::to_string)
}

fn tier_of(root: &Obj) -> Option<String> {
    root.get("organization")
        .and_then(|o| o.get("rate_limit_tier"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// Profil über die API. Scheitert still.
pub fn fetch_profile(http: &dyn HttpClient, token: &str) -> Option<Profile> {
    let resp = http.send(&usage::profile_request(token)).ok()?;
    if resp.status != 200 {
        return None;
    }
    match serde_json::from_slice::<Value>(&resp.body).ok()? {
        Value::Object(root) => Some(parse_profile(&root)),
        _ => None,
    }
}

/// „5h 45% bis 14:00 · 7d 56% bis Do., 09:00 · vor 12 min“ aus dem gecachten Stand, leer wenn nie ein Stand gemerkt
/// wurde. Ein Fenster, das seit der Messung zurückgesetzt hat, steht als „frei“. Ohne 5-Stunden-Limit (`session`
/// false) entfällt der 5h-Teil.
pub fn usage_detail<Tz: TimeZone>(
    u: Option<&CachedUsage>,
    session: bool,
    now: f64,
    tz: &Tz,
    lang: Lang,
) -> String {
    let Some(u) = u else { return String::new() };
    let five = if session {
        dimension("5h", u.session, u.session_resets, now, tz, lang)
    } else {
        String::new()
    };
    let seven = dimension("7d", u.weekly, u.weekly_resets, now, tz, lang);
    let mut parts: Vec<String> = [five, seven]
        .into_iter()
        .filter(|p| !p.is_empty())
        .collect();
    if parts.is_empty() {
        return String::new();
    }
    let mins = (((now - u.at) / 60.0) as i64).max(0);
    parts.push(if mins < 1 {
        lang.just_now().to_string()
    } else {
        lang.minutes_ago(mins)
    });
    parts.join(" · ")
}

fn dimension<Tz: TimeZone>(
    label: &str,
    pct: Option<i32>,
    resets: Option<f64>,
    now: f64,
    tz: &Tz,
    lang: Lang,
) -> String {
    let Some(pct) = pct else { return String::new() };
    let Some(resets) = resets else {
        return format!("{label} {pct}%");
    };
    if resets <= now {
        return format!("{label} {}", lang.free());
    }
    format!(
        "{label} {pct}% {}",
        lang.until(&reset_clock(resets, now, tz, lang))
    )
}

/// Name des angemeldeten Systembenutzers: Konto-Name des Live-Eintrags im macOS-Schlüsselbund.
pub fn current_user_name() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_default()
}

pub struct AccountService {
    state: AccountsState,
    secrets: Arc<dyn SecretStore>,
    http: Arc<dyn HttpClient>,
    settings: AccountSettings,
    live_account: String,
    last_switch: f64,
    /// Letzter Abfrageversuch je inaktivem Konto, damit ein 429 oder ein toter Token nicht jede Minute neu anklopft.
    last_poll: HashMap<String, f64>,
    events: Vec<AccountEvent>,
}

impl AccountService {
    pub fn new(
        state: AccountsState,
        secrets: Arc<dyn SecretStore>,
        http: Arc<dyn HttpClient>,
        settings: AccountSettings,
    ) -> Self {
        Self {
            state,
            secrets,
            http,
            settings,
            live_account: current_user_name(),
            last_switch: f64::NEG_INFINITY,
            last_poll: HashMap::new(),
            events: Vec::new(),
        }
    }

    /// Konto-Name des Live-Eintrags (macOS-Schlüsselbund), Voreinstellung der Systembenutzer.
    pub fn set_live_account(&mut self, name: &str) {
        self.live_account = name.into();
    }

    pub fn set_settings(&mut self, settings: AccountSettings) {
        self.settings = settings;
    }

    /// Zu speichernder Stand (nach jedem [`AccountEvent::Changed`]).
    pub fn state(&self) -> &AccountsState {
        &self.state
    }

    pub fn accounts(&self) -> &[Account] {
        &self.state.accounts
    }

    pub fn active_id(&self) -> Option<&str> {
        self.state.active_id.as_deref()
    }

    pub fn active(&self) -> Option<&Account> {
        let id = self.state.active_id.as_deref()?;
        self.state.accounts.iter().find(|a| a.id == id)
    }

    pub fn take_events(&mut self) -> Vec<AccountEvent> {
        std::mem::take(&mut self.events)
    }

    /// Für die Leiste: je Konto Kürzel, Markierung des aktiven und der zwischengespeicherte Nutzungsstand.
    pub fn menu_items<Tz: TimeZone>(&self, now: f64, tz: &Tz, lang: Lang) -> Vec<MenuItem> {
        self.state
            .accounts
            .iter()
            .map(|a| MenuItem {
                id: a.id.clone(),
                title: a.title(lang),
                active: Some(a.id.as_str()) == self.active_id(),
                detail: usage_detail(a.usage.as_ref(), a.has_session_limit(), now, tz, lang),
            })
            .collect()
    }

    /// Kacheln der Statistik: je Konto eine, ohne hinterlegtes Konto eine für das angemeldete.
    pub fn limit_sources<Tz: TimeZone>(&self, now: f64, tz: &Tz, lang: Lang) -> Vec<LimitSource> {
        if self.state.accounts.is_empty() {
            return vec![LimitSource {
                id: NO_ACCOUNT_ID.into(),
                title: lang.current_account().into(),
                detail: String::new(),
                kinds: LimitKind::ALL.to_vec(),
                current: None,
            }];
        }
        self.state
            .accounts
            .iter()
            .map(|a| LimitSource {
                id: a.id.clone(),
                title: a.title(lang),
                detail: usage_detail(a.usage.as_ref(), a.has_session_limit(), now, tz, lang),
                kinds: if a.has_session_limit() {
                    LimitKind::ALL.to_vec()
                } else {
                    vec![LimitKind::Weekly, LimitKind::Fable]
                },
                current: a.usage.as_ref().map(|u| UsageSample::from_cached(&a.id, u)),
            })
            .collect()
    }

    /// Nutzung des aktiven Kontos aus dem Poll merken, damit das Menü sie später auch für das inaktive zeigt.
    pub fn record_usage(&mut self, u: &Usage, now: f64) {
        match self.state.active_id.clone() {
            Some(id) => self.store(u, &id, now),
            None => self.events.push(AccountEvent::UsageStored(
                NO_ACCOUNT_ID.into(),
                CachedUsage::from_usage(u, now),
            )),
        }
    }

    fn store(&mut self, u: &Usage, id: &str, now: f64) {
        let Some(i) = self.state.accounts.iter().position(|a| a.id == id) else {
            return;
        };
        let cached = CachedUsage::from_usage(u, now);
        self.state.accounts[i].usage = Some(cached.clone());
        self.persist();
        self.events
            .push(AccountEvent::UsageStored(id.into(), cached));
    }

    fn persist(&mut self) {
        self.events.push(AccountEvent::Changed);
    }

    fn set_active(&mut self, id: &str) {
        self.state.active_id = Some(id.into());
        self.persist();
    }

    // MARK: Hinzufügen

    /// Nimmt das gerade angemeldete Konto (aktueller `Claude Code-credentials`-Eintrag) als Slot auf. Ist es als
    /// E-Mail schon dabei, wird nur sein Block aufgefrischt und es aktiv gesetzt. `false`, wenn kein angemeldetes
    /// Konto gefunden wurde.
    pub fn add_current(&mut self, lang: Lang) -> bool {
        let Some(oauth) = self
            .secrets
            .read(LIVE_SERVICE, None)
            .and_then(|raw| oauth::oauth_from(&raw))
        else {
            return false;
        };
        let Some(token) = oauth::text(&oauth, "accessToken").map(str::to_string) else {
            return false;
        };
        let profile = fetch_profile(&*self.http, &token);
        let email = profile.as_ref().and_then(|p| p.email.clone());
        let sub = oauth::text(&oauth, "subscriptionType").map(str::to_string);
        // Der Token kennt nur die Stufe vom Login, das Profil die aktuelle (Upgrade danach, Umbenennung Max 20x → Ultra).
        let tier = profile
            .and_then(|p| p.tier)
            .or_else(|| oauth::text(&oauth, "rateLimitTier").map(str::to_string));
        let blob = oauth::string_from(&oauth);
        let existing = email.as_ref().and_then(|e| {
            self.state
                .accounts
                .iter()
                .position(|a| a.email.as_ref() == Some(e))
        });
        match existing {
            Some(i) => self.refresh_slot(i, &blob, sub, tier),
            None => self.add_slot(&blob, email, sub, tier, lang),
        }
    }

    fn refresh_slot(
        &mut self,
        i: usize,
        blob: &str,
        sub: Option<String>,
        tier: Option<String>,
    ) -> bool {
        let id = self.state.accounts[i].id.clone();
        if !self.secrets.write(STORE_SERVICE, &id, blob) {
            return false;
        }
        self.state.accounts[i].subscription = sub;
        self.state.accounts[i].tier = tier;
        self.set_active(&id);
        true
    }

    fn add_slot(
        &mut self,
        blob: &str,
        email: Option<String>,
        sub: Option<String>,
        tier: Option<String>,
        lang: Lang,
    ) -> bool {
        let id = uuid::Uuid::new_v4().to_string().to_uppercase();
        if !self.secrets.write(STORE_SERVICE, &id, blob) {
            return false;
        }
        let alias = email
            .clone()
            .unwrap_or_else(|| format!("{} {}", lang.account(), self.state.accounts.len() + 1));
        self.state.accounts.push(Account {
            id: id.clone(),
            alias,
            email,
            subscription: sub,
            usage: None,
            tier,
        });
        self.set_active(&id);
        true
    }

    // MARK: Wechseln

    /// Schaltet das aktive Konto auf `id`. Der aktuell live liegende Block wird vorher in den Slot des bisher
    /// aktiven Kontos zurückgeschrieben (die `claude`-CLI rotiert die Tokens im Betrieb).
    pub fn switch_to(&mut self, id: &str, now: f64) -> bool {
        if Some(id) == self.active_id() || !self.state.accounts.iter().any(|a| a.id == id) {
            return false;
        }
        let Some(mut full) = self
            .secrets
            .read(LIVE_SERVICE, None)
            .and_then(|raw| oauth::object_from(&raw))
        else {
            return false;
        };
        self.capture_live(&full);
        let Some(target) = self
            .secrets
            .read(STORE_SERVICE, Some(id))
            .and_then(|raw| oauth::object_from(&raw))
        else {
            return false;
        };
        full.insert("claudeAiOauth".into(), Value::Object(target.clone()));
        if !self
            .secrets
            .write(LIVE_SERVICE, &self.live_account, &oauth::string_from(&full))
        {
            return false;
        }
        self.last_switch = now;
        self.fill_tier(id, &target);
        self.set_active(id);
        self.events.push(AccountEvent::ActiveChanged);
        log::info!("Account gewechselt");
        true
    }

    fn fill_tier(&mut self, id: &str, target: &Obj) {
        let Some(tier) = oauth::text(target, "rateLimitTier") else {
            return;
        };
        if let Some(a) = self
            .state
            .accounts
            .iter_mut()
            .find(|a| a.id == id && a.tier.is_none())
        {
            a.tier = Some(tier.into());
        }
    }

    /// Den live liegenden `claudeAiOauth`-Block in den Slot des bisher aktiven Kontos sichern, damit dessen
    /// rotierter Refresh-Token nicht verloren geht.
    ///
    /// Bekannte Grenze wie in Swift: schreibt immer in `active_id`. Meldet man sich außerhalb von Kadrell an einem
    /// anderen Konto an und lässt dann wechseln, landet dessen Block im falschen Slot. Upgrade-Pfad: vor dem Sichern
    /// die E-Mail per `/oauth/profile` gegen den aktiven Slot prüfen.
    fn capture_live(&self, full: &Obj) {
        let Some(cur) = self.active_id() else { return };
        let Some(Value::Object(oauth)) = full.get("claudeAiOauth") else {
            return;
        };
        self.secrets
            .write(STORE_SERVICE, cur, &oauth::string_from(oauth));
    }

    /// Aktuelle Stufe jedes Kontos über `/api/oauth/profile` holen. Das `rateLimitTier` im Token stammt vom Login
    /// und bleibt nach einem Upgrade stehen (Max 5x → Ultra), das Profil kennt die gültige. Scheitert die Abfrage,
    /// bleibt die gemerkte Stufe, fehlt auch die, kommt sie aus dem Token.
    pub fn load_tiers(&mut self, now: f64) {
        let known: Vec<(String, bool)> = self
            .state
            .accounts
            .iter()
            .map(|a| (a.id.clone(), a.tier.is_some()))
            .collect();
        for (id, has_tier) in known {
            let mut tier = self.profile_tier(&id, now);
            if tier.is_none() && !has_tier {
                tier = self.stored_tier(&id);
            }
            if let (Some(tier), Some(a)) =
                (tier, self.state.accounts.iter_mut().find(|a| a.id == id))
            {
                a.tier = Some(tier);
            }
        }
        self.persist();
    }

    /// Stufe laut Profil, mit dem Live-Token für das aktive Konto und dem Slot-Token für die übrigen.
    fn profile_tier(&mut self, id: &str, now: f64) -> Option<String> {
        let token = if Some(id) == self.active_id() {
            usage::live_token(&*self.secrets)
        } else {
            self.slot_token(id, false, now)
        }?;
        fetch_profile(&*self.http, &token)?.tier
    }

    /// Stufe aus dem Secret-Speicher: aus dem Live-Eintrag für das aktive, aus dem Slot für die übrigen. Nur lesen.
    fn stored_tier(&self, id: &str) -> Option<String> {
        let active = Some(id) == self.active_id();
        let raw = if active {
            self.secrets.read(LIVE_SERVICE, None)
        } else {
            self.secrets.read(STORE_SERVICE, Some(id))
        }?;
        // Der Live-Eintrag hat den Block unter `claudeAiOauth`, ein Slot ist der Block selbst.
        let block = oauth::oauth_from(&raw).or_else(|| oauth::object_from(&raw))?;
        oauth::text(&block, "rateLimitTier").map(str::to_string)
    }

    // MARK: Inaktive Konten abfragen

    /// Nutzung jedes inaktiven Kontos, dessen Stand (oder letzter Versuch) älter als `max_age` Sekunden ist, mit dessen
    /// eigenem Token abfragen. Scheitert eine Abfrage, bleibt der alte Stand mit seinem alten Zeitpunkt stehen. Der
    /// Aufrufer ruft das etwa jede Minute mit `max_age` 1200 (Swift: `startInactivePolling`), abgefragt wird nur,
    /// was fällig ist.
    pub fn refresh_inactive(&mut self, max_age: f64, now: f64) {
        let active = self.state.active_id.clone();
        let due: Vec<String> = self
            .state
            .accounts
            .iter()
            .filter(|a| {
                Some(&a.id) != active.as_ref()
                    && balance::is_due(a, self.last_poll.get(&a.id).copied(), max_age, now)
            })
            .map(|a| a.id.clone())
            .collect();
        if !due.is_empty() {
            log::info!("inaktive Accounts abfragen: {}", due.len());
        }
        for id in due {
            self.last_poll.insert(id.clone(), now);
            self.poll_inactive(&id, now);
        }
    }

    fn poll_inactive(&mut self, id: &str, now: f64) {
        let Some(token) = self.slot_token(id, false, now) else {
            log::warn!("inaktiver Account: kein gültiger Token");
            return;
        };
        let mut result = usage::fetch(&*self.http, &token);
        // Das 429 hängt am Access-Token, nicht am Konto: den alten Token des Slots fragen womöglich noch andere
        // Clients ab, ein frisch erneuerter kommt sofort durch. Deshalb einmal mit neuem Token nachfassen.
        if matches!(result, FetchResult::RateLimited(_))
            && let Some(renewed) = self.slot_token(id, true, now)
        {
            result = usage::fetch(&*self.http, &renewed);
        }
        match result {
            FetchResult::Ok(u) if Some(id) != self.active_id() => self.store(&u, id, now),
            FetchResult::RateLimited(_) => {
                log::warn!("inaktiver Account: 429, nächster Versuch im nächsten Intervall")
            }
            _ => {}
        }
    }

    /// Access-Token eines inaktiven Slots; ist er (fast) abgelaufen oder `renew` gesetzt, wird er erneuert und der
    /// neue Block sofort zurückgeschrieben.
    fn slot_token(&mut self, id: &str, renew: bool, now: f64) -> Option<String> {
        if Some(id) == self.active_id() {
            return None;
        }
        let mut oauth = oauth::object_from(&self.secrets.read(STORE_SERVICE, Some(id))?)?;
        if renew || oauth::needs_refresh(&oauth, now) {
            oauth = self.renewed_block(id, &oauth, now)?;
        }
        oauth::text(&oauth, "accessToken").map(str::to_string)
    }

    fn renewed_block(&self, id: &str, oauth: &Obj, now: f64) -> Option<Obj> {
        // Temp-Profile teilen den Secret-Speicher mit dem eigenen Kadrell: dort nie rotieren, sonst verbraucht die
        // Testinstanz den Refresh-Token, den das eigene Kadrell beim nächsten Wechsel braucht.
        if self.settings.temporary_profile {
            return None;
        }
        let req = oauth::refresh_request(oauth)?;
        let data = oauth::send(&*self.http, &req)?;
        let renewed = oauth::applying(&data, oauth, now)?;
        if !self
            .secrets
            .write(STORE_SERVICE, id, &oauth::string_from(&renewed))
        {
            log::error!("Token erneuert, aber nicht gespeichert");
            return None;
        }
        Some(renewed)
    }

    // MARK: Verwalten

    pub fn remove(&mut self, id: &str) {
        self.secrets.delete(STORE_SERVICE, id);
        self.state.accounts.retain(|a| a.id != id);
        if self.active_id() == Some(id) {
            self.state.active_id = None;
        }
        self.persist();
    }

    // MARK: Auto-Wechsel

    /// Nach jedem Usage-Update: ist das aktive Konto über der Schwelle und die Sperrzeit vorbei, auf das Konto mit dem
    /// meisten Rest wechseln, aber nur wenn das spürbar leerer ist. Die Kandidaten werden vorher live abgefragt;
    /// scheitert das, wird nicht gewechselt (ein alter Stand kann längst voll sein). Blockiert für die Abfragen, der
    /// Aufrufer ruft es auf einem Arbeitsthread. Gibt das jetzt aktive Konto zurück, wenn gewechselt wurde.
    pub fn consider_auto_switch(&mut self, usage: &Usage, now: f64) -> Option<Account> {
        let active_id = self.state.active_id.clone()?;
        if !self.auto_switch_allowed(now)
            || self.auto_switch_target(usage, &active_id, now).is_none()
        {
            return None;
        }
        self.refresh_inactive(VERIFY_AGE, now);
        let target = self.verified_target(usage, &active_id, now)?;
        if !self.switch_to(&target, now) {
            return None;
        }
        let to = self.state.accounts.iter().find(|a| a.id == target)?.clone();
        self.events.push(AccountEvent::AutoSwitched(to.clone()));
        Some(to)
    }

    /// Temp-Profile tauschten das Konto aller laufenden Claude-Prozesse, deshalb nie automatisch.
    fn auto_switch_allowed(&self, now: f64) -> bool {
        !self.settings.temporary_profile
            && self.settings.autoswitch_enabled
            && self.state.accounts.len() > 1
            && now - self.last_switch >= COOLDOWN
    }

    /// Das Ziel nach der Live-Abfrage, aber nur, wenn sein Stand dabei wirklich frisch geworden ist.
    fn verified_target(&self, usage: &Usage, active_id: &str, now: f64) -> Option<String> {
        let target = self.auto_switch_target(usage, active_id, now)?;
        let at = self
            .state
            .accounts
            .iter()
            .find(|a| a.id == target)?
            .usage
            .as_ref()
            .map(|u| u.at)?;
        if now - at > VERIFY_AGE * 2.0 {
            log::info!("Auto-Wechsel verworfen: Ziel nicht live bestätigt");
            return None;
        }
        Some(target)
    }

    fn auto_switch_target(&self, usage: &Usage, active_id: &str, now: f64) -> Option<String> {
        let active = self.state.accounts.iter().find(|a| a.id == active_id);
        balance::switch_target(
            usage,
            active,
            &self.state.accounts,
            active_id,
            SwitchPolicy {
                threshold: self.settings.autoswitch_threshold,
                on_pace: self.settings.autoswitch_on_pace,
                hysteresis: HYSTERESIS,
            },
            now,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::{HttpError, HttpResponse};
    use crate::secret::MemorySecretStore;
    use crate::testing::FakeHttp;
    use chrono::Utc;
    use serde_json::json;

    const NOW: f64 = 1_000_000.0;

    /// Vollständiges Credential-JSON mit MCP-Logins und einem Account-Block.
    fn live_json(access: &str, mcp: &str) -> String {
        format!(
            r#"{{"mcpOAuth":{{"srv":{{"accessToken":"{mcp}"}}}},"claudeAiOauth":{{"accessToken":"{access}","refreshToken":"r-{access}","subscriptionType":"max"}}}}"#
        )
    }

    #[test]
    fn oauth_extraction_reads_claude_block() {
        let oauth = oauth::oauth_from(&live_json("A", "M")).unwrap();
        assert_eq!(oauth["accessToken"], "A");
        assert_eq!(oauth["subscriptionType"], "max");
    }

    /// Der Kern des Global-Swaps: nur `claudeAiOauth` wird getauscht, `mcpOAuth` (konto-unabhängig) bleibt stehen.
    #[test]
    fn swap_replaces_claude_block_but_keeps_mcp() {
        let mut full = oauth::object_from(&live_json("A", "M")).unwrap();
        let target = oauth::oauth_from(&live_json("B", "ignored")).unwrap();
        full.insert("claudeAiOauth".into(), Value::Object(target));
        let back = oauth::object_from(&oauth::string_from(&full)).unwrap();
        assert_eq!(
            back["claudeAiOauth"]["accessToken"], "B",
            "Account-Block getauscht"
        );
        assert_eq!(
            back["mcpOAuth"]["srv"]["accessToken"], "M",
            "MCP-Logins unangetastet"
        );
    }

    #[test]
    fn object_from_garbage_is_none() {
        assert!(oauth::object_from("kein json").is_none());
        assert!(oauth::oauth_from("{}").is_none());
        assert!(oauth::object_from("[1]").is_none());
    }

    /// Seed für die Leiste: abgelaufene Fenster kommen reset-bereinigt als 0 zurück, laufende behalten ihren Wert.
    #[test]
    fn as_usage_applies_resets() {
        let cached = CachedUsage {
            session: Some(18),
            weekly: Some(60),
            fable: Some(19),
            session_resets: Some(NOW - 60.0), // 5h schon zurückgesetzt
            weekly_resets: Some(NOW + 3600.0), // 7d/Fable noch offen
            at: NOW,
        };
        let u = cached.as_usage(NOW);
        assert_eq!(u.session, Some(0), "abgelaufenes 5h-Fenster ist frei");
        assert_eq!(u.weekly, Some(60), "laufendes 7d-Fenster behält den Wert");
        assert_eq!(
            u.fable,
            Some(19),
            "Fable folgt dem 7d-Reset, hier noch offen"
        );
    }

    /// Planname nach der Umbenennung vom 30.10.2026, ohne Stufe der Abo-Typ.
    #[test]
    fn plan_name_follows_tier() {
        let a = Account::new("a", "a");
        let tiered = |t: &str| Account {
            tier: Some(t.into()),
            ..a.clone()
        };
        assert_eq!(
            tiered("default_claude_max_20x").plan_name().as_deref(),
            Some("Ultra")
        );
        assert_eq!(
            tiered("default_claude_max_5x").plan_name().as_deref(),
            Some("Max")
        );
        let pro = Account {
            subscription: Some("pro".into()),
            ..a.clone()
        };
        assert_eq!(pro.plan_name().as_deref(), Some("Pro"));
        assert!(!tiered("default_claude_max_20x").has_session_limit());
        assert!(
            a.has_session_limit(),
            "ohne bekannte Stufe gilt das Limit weiter"
        );
    }

    #[test]
    fn title_falls_back_from_alias_to_email_to_placeholder() {
        let mut a = Account::new("a", "");
        assert_eq!(a.title(Lang::De), "Konto");
        assert_eq!(a.title(Lang::En), "Account");
        a.email = Some("a@b.de".into());
        assert_eq!(a.title(Lang::De), "a@b.de");
        a.alias = "Privat".into();
        assert_eq!(a.title(Lang::De), "Privat");
    }

    /// Im Konto-Menü entfällt der 5h-Teil bei Konten ohne 5-Stunden-Limit.
    #[test]
    fn usage_detail_hides_session_without_limit() {
        let u = CachedUsage {
            session: Some(40),
            weekly: Some(20),
            session_resets: Some(NOW + 3600.0),
            weekly_resets: Some(NOW + 3600.0),
            at: NOW,
            ..CachedUsage::default()
        };
        assert!(usage_detail(Some(&u), true, NOW, &Utc, Lang::De).contains("5h"));
        assert!(!usage_detail(Some(&u), false, NOW, &Utc, Lang::De).contains("5h"));
        assert_eq!(usage_detail(None, true, NOW, &Utc, Lang::De), "");
    }

    /// Im Konto-Menü steht, bis wann ein Fenster läuft: heute nur die Uhrzeit, sonst mit Wochentag.
    #[test]
    fn usage_detail_shows_reset_age_and_free() {
        let now = 1_800_000_000.0; // 2027-01-15 08:00 UTC, Freitag
        let u = CachedUsage {
            session: Some(45),
            weekly: Some(56),
            session_resets: Some(now + 6.0 * 3600.0),
            weekly_resets: Some(now + 3.0 * 86_400.0),
            at: now - 12.0 * 60.0,
            ..CachedUsage::default()
        };
        assert_eq!(
            usage_detail(Some(&u), true, now, &Utc, Lang::De),
            "5h 45% bis 14:00 · 7d 56% bis Mo., 08:00 · vor 12 min"
        );
        assert_eq!(
            usage_detail(Some(&u), true, now, &Utc, Lang::En),
            "5h 45% until 14:00 · 7d 56% until Mon 08:00 · 12 min ago"
        );
        let freed = CachedUsage {
            session_resets: Some(now - 1.0),
            at: now,
            ..u
        };
        assert!(
            usage_detail(Some(&freed), true, now, &Utc, Lang::De).starts_with("5h frei · 7d 56%")
        );
        assert!(usage_detail(Some(&freed), true, now, &Utc, Lang::De).ends_with("gerade eben"));
    }

    /// Das Profil liefert die gültige Stufe, auch wenn der Token noch die vom Login trägt.
    #[test]
    fn profile_carries_current_tier() {
        let Value::Object(root) = json!({"account": {"email": "a@b.de"}, "organization": {"rate_limit_tier": "default_claude_max_20x"}})
        else {
            unreachable!()
        };
        let p = parse_profile(&root);
        assert_eq!(p.email.as_deref(), Some("a@b.de"));
        assert_eq!(p.tier.as_deref(), Some("default_claude_max_20x"));
        assert_eq!(parse_profile(&Obj::new()).tier, None);
        let Value::Object(alt) = json!({"account": {"email_address": "x@y.de"}}) else {
            unreachable!()
        };
        assert_eq!(parse_profile(&alt).email.as_deref(), Some("x@y.de"));
        let Value::Object(top) = json!({"email": "t@y.de"}) else {
            unreachable!()
        };
        assert_eq!(parse_profile(&top).email.as_deref(), Some("t@y.de"));
    }

    // MARK: Dienst mit Fake-Speicher und Fake-HTTP

    fn service(
        secrets: &Arc<MemorySecretStore>,
        http: FakeHttp,
        state: AccountsState,
        settings: AccountSettings,
    ) -> AccountService {
        let mut s = AccountService::new(state, secrets.clone(), Arc::new(http), settings);
        s.set_live_account("user");
        s
    }

    fn state(active: &str, ids: &[&str]) -> AccountsState {
        AccountsState {
            accounts: ids.iter().map(|i| Account::new(i, i)).collect(),
            active_id: Some(active.into()),
        }
    }

    fn slot(access: &str, tier: Option<&str>) -> String {
        let mut v = json!({"accessToken": access, "refreshToken": format!("r-{access}")});
        if let Some(t) = tier {
            v["rateLimitTier"] = t.into();
        }
        v.to_string()
    }

    #[test]
    fn switch_swaps_block_captures_live_and_keeps_mcp() {
        let secrets = Arc::new(
            MemorySecretStore::new()
                .with(LIVE_SERVICE, "user", &live_json("A-rotated", "M"))
                .with(
                    STORE_SERVICE,
                    "b",
                    &slot("B", Some("default_claude_max_5x")),
                ),
        );
        let mut s = service(
            &secrets,
            FakeHttp::offline(),
            state("a", &["a", "b"]),
            AccountSettings::default(),
        );
        assert!(!s.switch_to("a", NOW), "schon aktiv");
        assert!(!s.switch_to("nope", NOW), "unbekannt");
        assert!(s.switch_to("b", NOW));
        let live = oauth::object_from(&secrets.get(LIVE_SERVICE, "user").unwrap()).unwrap();
        assert_eq!(live["claudeAiOauth"]["accessToken"], "B");
        assert_eq!(live["mcpOAuth"]["srv"]["accessToken"], "M");
        let saved = oauth::object_from(&secrets.get(STORE_SERVICE, "a").unwrap()).unwrap();
        assert_eq!(
            saved["accessToken"], "A-rotated",
            "rotierter Block des bisherigen Kontos gesichert"
        );
        assert_eq!(s.active_id(), Some("b"));
        assert_eq!(
            s.accounts()[1].tier.as_deref(),
            Some("default_claude_max_5x"),
            "Stufe aus dem Slot nachgetragen"
        );
        let events = s.take_events();
        assert!(events.contains(&AccountEvent::ActiveChanged));
        assert!(events.contains(&AccountEvent::Changed));
        let order = secrets.write_log();
        assert_eq!(order[0].1, "a", "erst sichern, dann tauschen");
    }

    #[test]
    fn failed_live_write_keeps_active_account() {
        let secrets = Arc::new(
            MemorySecretStore::new()
                .with(LIVE_SERVICE, "user", &live_json("A", "M"))
                .with(STORE_SERVICE, "b", &slot("B", None)),
        );
        let mut s = service(
            &secrets,
            FakeHttp::offline(),
            state("a", &["a", "b"]),
            AccountSettings::default(),
        );
        secrets.set_fail_writes(true);
        assert!(!s.switch_to("b", NOW));
        assert_eq!(s.active_id(), Some("a"));
    }

    #[test]
    fn add_current_creates_slot_then_refreshes_by_email() {
        let secrets =
            Arc::new(MemorySecretStore::new().with(LIVE_SERVICE, "user", &live_json("A", "M")));
        let http = FakeHttp::json(
            200,
            r#"{"account":{"email":"me@x.de"},"organization":{"rate_limit_tier":"default_claude_max_20x"}}"#,
        );
        let mut s = service(
            &secrets,
            http,
            AccountsState::default(),
            AccountSettings::default(),
        );
        assert!(s.add_current(Lang::De));
        assert_eq!(s.accounts().len(), 1);
        let a = s.accounts()[0].clone();
        assert_eq!(
            (
                a.alias.as_str(),
                a.email.as_deref(),
                a.tier.as_deref(),
                a.subscription.as_deref()
            ),
            (
                "me@x.de",
                Some("me@x.de"),
                Some("default_claude_max_20x"),
                Some("max")
            )
        );
        assert_eq!(s.active_id(), Some(a.id.as_str()));
        assert!(secrets.get(STORE_SERVICE, &a.id).unwrap().contains("\"A\""));
        // Zweiter Aufruf mit neuem Token: gleiche E-Mail, kein zweiter Slot.
        secrets.write(LIVE_SERVICE, "user", &live_json("A2", "M"));
        assert!(s.add_current(Lang::De));
        assert_eq!(s.accounts().len(), 1);
        assert!(secrets.get(STORE_SERVICE, &a.id).unwrap().contains("A2"));
    }

    #[test]
    fn add_current_without_login_fails_and_unnamed_gets_numbered_alias() {
        let empty = Arc::new(MemorySecretStore::new());
        let mut s = service(
            &empty,
            FakeHttp::offline(),
            AccountsState::default(),
            AccountSettings::default(),
        );
        assert!(!s.add_current(Lang::De));
        let secrets =
            Arc::new(MemorySecretStore::new().with(LIVE_SERVICE, "user", &live_json("A", "M")));
        let mut s = service(
            &secrets,
            FakeHttp::offline(),
            AccountsState::default(),
            AccountSettings::default(),
        );
        assert!(s.add_current(Lang::En));
        assert_eq!(s.accounts()[0].alias, "Account 1");
        assert_eq!(s.accounts()[0].email, None);
    }

    #[test]
    fn remove_deletes_slot_and_clears_active() {
        let secrets = Arc::new(MemorySecretStore::new().with(STORE_SERVICE, "a", &slot("A", None)));
        let mut s = service(
            &secrets,
            FakeHttp::offline(),
            state("a", &["a", "b"]),
            AccountSettings::default(),
        );
        s.remove("a");
        assert_eq!(s.accounts().len(), 1);
        assert_eq!(s.active_id(), None);
        assert_eq!(secrets.get(STORE_SERVICE, "a"), None);
    }

    #[test]
    fn record_usage_stores_for_active_or_reports_no_account() {
        let secrets = Arc::new(MemorySecretStore::new());
        let mut s = service(
            &secrets,
            FakeHttp::offline(),
            state("a", &["a"]),
            AccountSettings::default(),
        );
        let u = Usage {
            weekly: Some(5),
            ..Usage::default()
        };
        s.record_usage(&u, NOW);
        assert_eq!(
            s.accounts()[0].usage.as_ref().and_then(|c| c.weekly),
            Some(5)
        );
        assert!(
            matches!(s.take_events().last(), Some(AccountEvent::UsageStored(id, _)) if id == "a")
        );
        let mut none = service(
            &secrets,
            FakeHttp::offline(),
            AccountsState::default(),
            AccountSettings::default(),
        );
        none.record_usage(&u, NOW);
        assert!(
            matches!(none.take_events().as_slice(), [AccountEvent::UsageStored(id, _)] if id == NO_ACCOUNT_ID)
        );
    }

    #[test]
    fn limit_sources_per_account_or_one_for_current() {
        let secrets = Arc::new(MemorySecretStore::new());
        let none = service(
            &secrets,
            FakeHttp::offline(),
            AccountsState::default(),
            AccountSettings::default(),
        );
        let src = none.limit_sources(NOW, &Utc, Lang::De);
        assert_eq!(
            (src.len(), src[0].id.as_str(), src[0].title.as_str()),
            (1, "current", "Aktueller Account")
        );
        let mut st = state("a", &["a", "u"]);
        st.accounts[1].tier = Some("default_claude_max_20x".into());
        let s = service(
            &secrets,
            FakeHttp::offline(),
            st,
            AccountSettings::default(),
        );
        let src = s.limit_sources(NOW, &Utc, Lang::De);
        assert_eq!(src[0].kinds.len(), 3);
        assert_eq!(
            src[1].kinds,
            vec![LimitKind::Weekly, LimitKind::Fable],
            "Ultra ohne 5h"
        );
    }

    /// Router für inaktive Konten: Token-Endpunkt, Nutzung und Profil.
    fn routed(usage_statuses: Vec<u16>) -> FakeHttp {
        let mut statuses = usage_statuses.into_iter();
        FakeHttp::new(move |req| {
            let (status, body) = if req.url.contains("/oauth/token") {
                (
                    200,
                    r#"{"access_token":"fresh","refresh_token":"r-fresh","expires_in":3600}"#
                        .to_string(),
                )
            } else if req.url.contains("/oauth/usage") {
                (
                    statuses.next().unwrap_or(200),
                    r#"{"seven_day":{"utilization":33}}"#.to_string(),
                )
            } else {
                return Err(HttpError("unerwartet".into()));
            };
            Ok(HttpResponse {
                status,
                headers: Vec::new(),
                body: body.into_bytes(),
            })
        })
    }

    fn expired_slot() -> String {
        json!({"accessToken": "stale", "refreshToken": "r-stale", "expiresAt": ((NOW - 100.0) * 1000.0) as i64}).to_string()
    }

    #[test]
    fn refresh_inactive_renews_expired_token_and_stores_usage() {
        let secrets = Arc::new(MemorySecretStore::new().with(STORE_SERVICE, "b", &expired_slot()));
        let mut s = service(
            &secrets,
            routed(vec![200]),
            state("a", &["a", "b"]),
            AccountSettings::default(),
        );
        s.refresh_inactive(1200.0, NOW);
        assert_eq!(
            s.accounts()[1].usage.as_ref().and_then(|u| u.weekly),
            Some(33)
        );
        let saved = oauth::object_from(&secrets.get(STORE_SERVICE, "b").unwrap()).unwrap();
        assert_eq!(
            saved["accessToken"], "fresh",
            "erneuerter Block sofort zurückgeschrieben"
        );
        assert_eq!(saved["refreshToken"], "r-fresh");
        // Gerade erst versucht: nicht gleich noch einmal.
        let before = s.take_events().len();
        s.refresh_inactive(1200.0, NOW + 10.0);
        assert_eq!(s.take_events().len(), 0);
        assert!(before > 0);
    }

    #[test]
    fn rate_limited_inactive_account_retries_once_with_renewed_token() {
        let valid = json!({"accessToken": "ok", "refreshToken": "r-ok", "expiresAt": ((NOW + 9999.0) * 1000.0) as i64}).to_string();
        let secrets = Arc::new(MemorySecretStore::new().with(STORE_SERVICE, "b", &valid));
        let mut s = service(
            &secrets,
            routed(vec![429, 200]),
            state("a", &["a", "b"]),
            AccountSettings::default(),
        );
        s.refresh_inactive(1200.0, NOW);
        assert_eq!(
            s.accounts()[1].usage.as_ref().and_then(|u| u.weekly),
            Some(33)
        );
        let saved = oauth::object_from(&secrets.get(STORE_SERVICE, "b").unwrap()).unwrap();
        assert_eq!(saved["accessToken"], "fresh");
    }

    #[test]
    fn temporary_profile_never_rotates_tokens() {
        let secrets = Arc::new(MemorySecretStore::new().with(STORE_SERVICE, "b", &expired_slot()));
        let settings = AccountSettings {
            temporary_profile: true,
            ..AccountSettings::default()
        };
        let mut s = service(
            &secrets,
            routed(vec![200]),
            state("a", &["a", "b"]),
            settings,
        );
        s.refresh_inactive(1200.0, NOW);
        assert!(s.accounts()[1].usage.is_none());
        assert!(
            secrets.get(STORE_SERVICE, "b").unwrap().contains("r-stale"),
            "Refresh-Token unverbraucht"
        );
    }

    fn two_accounts_for_auto_switch() -> (Arc<MemorySecretStore>, AccountsState) {
        let secrets = Arc::new(
            MemorySecretStore::new()
                .with(LIVE_SERVICE, "user", &live_json("A", "M"))
                .with(STORE_SERVICE, "b", &json!({"accessToken": "B", "refreshToken": "r", "expiresAt": ((NOW + 9999.0) * 1000.0) as i64}).to_string()),
        );
        let mut st = state("a", &["a", "b"]);
        st.accounts[0].usage = Some(CachedUsage {
            session: Some(95),
            weekly: Some(30),
            session_resets: Some(NOW + 3600.0),
            weekly_resets: Some(NOW + 84.0 * 3600.0),
            at: NOW,
            ..CachedUsage::default()
        });
        (secrets, st)
    }

    fn busy_usage() -> Usage {
        Usage {
            session: Some(95),
            weekly: Some(30),
            session_resets: Some(NOW + 3600.0),
            weekly_resets: Some(NOW + 84.0 * 3600.0),
            fable: None,
        }
    }

    #[test]
    fn auto_switch_verifies_target_live_and_respects_cooldown() {
        let (secrets, st) = two_accounts_for_auto_switch();
        let settings = AccountSettings {
            autoswitch_enabled: true,
            ..AccountSettings::default()
        };
        let mut s = service(&secrets, routed(vec![200]), st, settings);
        let to = s
            .consider_auto_switch(&busy_usage(), NOW)
            .expect("wechselt auf b");
        assert_eq!(to.id, "b");
        assert_eq!(s.active_id(), Some("b"));
        assert!(
            s.take_events()
                .iter()
                .any(|e| matches!(e, AccountEvent::AutoSwitched(a) if a.id == "b"))
        );
        // Sperrzeit: sofort danach kein weiterer Wechsel.
        assert!(s.consider_auto_switch(&busy_usage(), NOW + 10.0).is_none());
    }

    #[test]
    fn auto_switch_does_nothing_when_disabled_temporary_or_unverifiable() {
        let (secrets, st) = two_accounts_for_auto_switch();
        let mut off = service(
            &secrets,
            routed(vec![200]),
            st.clone(),
            AccountSettings::default(),
        );
        assert!(
            off.consider_auto_switch(&busy_usage(), NOW).is_none(),
            "ausgeschaltet"
        );
        let tmp = AccountSettings {
            autoswitch_enabled: true,
            temporary_profile: true,
            ..AccountSettings::default()
        };
        let mut t = service(&secrets, routed(vec![200]), st.clone(), tmp);
        assert!(
            t.consider_auto_switch(&busy_usage(), NOW).is_none(),
            "Temp-Profil"
        );
        // Live-Abfrage des Ziels scheitert: der unbekannte Stand reicht nicht, es wird nicht gewechselt.
        let on = AccountSettings {
            autoswitch_enabled: true,
            ..AccountSettings::default()
        };
        let mut fail = service(&secrets, FakeHttp::offline(), st, on);
        assert!(
            fail.consider_auto_switch(&busy_usage(), NOW + 1000.0)
                .is_none()
        );
        assert_eq!(fail.active_id(), Some("a"));
    }

    #[test]
    fn load_tiers_prefers_profile_then_stored_token_tier() {
        let secrets = Arc::new(
            MemorySecretStore::new()
                .with(LIVE_SERVICE, "user", &live_json("A", "M"))
                .with(STORE_SERVICE, "b", &json!({"accessToken": "B", "rateLimitTier": "from_token", "expiresAt": ((NOW + 9999.0) * 1000.0) as i64}).to_string()),
        );
        let http = FakeHttp::new(|req| {
            let body = if req.header("Authorization") == Some("Bearer A") {
                r#"{"organization":{"rate_limit_tier":"from_profile"}}"#
            } else {
                "{}"
            };
            Ok(HttpResponse {
                status: 200,
                headers: Vec::new(),
                body: body.as_bytes().to_vec(),
            })
        });
        let mut s = service(
            &secrets,
            http,
            state("a", &["a", "b"]),
            AccountSettings::default(),
        );
        s.load_tiers(NOW);
        assert_eq!(s.accounts()[0].tier.as_deref(), Some("from_profile"));
        assert_eq!(s.accounts()[1].tier.as_deref(), Some("from_token"));
    }
}
