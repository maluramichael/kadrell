//! Konten und Nutzung im App-Prozess: `AccountService`, `UsageService`, Verlauf und Tempo-Warnung hinter einem Griff.
//!
//! Alles hier blockiert (Netz, Schlüsselbund) und läuft deshalb auf dem Hintergrund-Executor: die Oberfläche ruft
//! nur [`Hub::step`] dort auf und liest sonst den [`View`], einen kleinen Schnappschuss mit eigener Sperre. So hält
//! ein Abruf nie das Zeichnen auf. Aufträge der Oberfläche (Wechseln, Entfernen, Hinzufügen) gehen als [`Cmd`] in
//! eine Warteschlange und laufen im nächsten Schritt.
//!
//! Zeiten sind Sekunden seit 1970. Geheimnisse landen nie im Log: es steht nirgends ein Token in einem Text.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};

use chrono::Local;
use kadrell_accounts::{
    Account, AccountEvent, AccountService, AccountSettings, AccountsState, HttpClient, LimitSource, MenuItem, Outcome, PaceWarning,
    PaceWatch, SecretStore, Usage, UsageHistory, UsageSample, UsageService, fetch_profile, live_token,
};

use super::fake::Fake;

/// Abstand der Nutzungsabfrage in Sekunden (Swift: 300).
const POLL_INTERVAL: f64 = 300.;
/// Inaktive Konten werden höchstens so oft (Sekunden) neu geprüft, fällig wird ein Konto nach `INACTIVE_MAX_AGE`.
const INACTIVE_EVERY: f64 = 60.;
const INACTIVE_MAX_AGE: f64 = 1200.;

/// Aufträge der Oberfläche.
#[derive(Clone, Debug, PartialEq)]
pub enum Cmd {
    Switch(String),
    Remove(String),
    /// Das gerade angemeldete Konto aufnehmen.
    AddCurrent,
    /// E-Mail des angemeldeten Kontos neu lesen (beim Öffnen der Einstellungen).
    RefreshEmail,
}

/// Was die Oberfläche dem Nutzer melden soll.
#[derive(Clone, Debug, PartialEq)]
pub enum Notice {
    /// Nach einem automatischen Wechsel: Titel des jetzt aktiven Kontos.
    AutoSwitched(String),
    /// Das 7-Tage-Limit läuft bei diesem Tempo zu früh voll.
    Pace(PaceWarning),
    /// „Hinzufügen“ fand kein angemeldetes Konto.
    NoAccount,
}

/// Einstellungen, die jeden Schritt neu gelesen werden (ein Wechsel wirkt ohne Neustart).
#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    pub account: AccountSettings,
    pub pace_warning: bool,
    pub notice: bool,
    pub lang: kadrell_accounts::Lang,
}

/// Zeile der Kontenliste in den Einstellungen.
#[derive(Clone, Debug, PartialEq)]
pub struct AccountRow {
    pub id: String,
    pub title: String,
    pub plan: Option<String>,
    pub email: Option<String>,
}

/// Schnappschuss für die Oberfläche.
#[derive(Clone, Debug, Default)]
pub struct View {
    pub usage: Usage,
    pub measured_at: Option<f64>,
    /// Hat der aktive Account ein 5-Stunden-Limit? Ohne (Ultra) entfällt der 5h-Wert.
    pub show_session: bool,
    pub menu: Vec<MenuItem>,
    pub accounts: Vec<AccountRow>,
    pub active_id: Option<String>,
    pub current_email: Option<String>,
    pub sources: Vec<LimitSource>,
}

impl View {
    /// Titel des aktiven Kontos für die Konto-Taste, leer solange es keine Konten gibt.
    pub fn active_title(&self) -> Option<String> {
        self.menu.iter().find(|m| m.active).map(|m| m.title.clone())
    }
}

/// Neu zu speichernder Stand der Konten (`accounts.index`, `accounts.active`).
#[derive(Clone, Debug, PartialEq)]
pub struct Persist {
    pub index: String,
    pub active: Option<String>,
}

/// Ergebnis eines Schritts.
#[derive(Debug, Default)]
pub struct Step {
    pub notices: Vec<Notice>,
    pub persist: Option<Persist>,
}

#[derive(Default)]
struct Timing {
    started: bool,
    last_inactive: f64,
}

pub struct Hub {
    service: Mutex<AccountService>,
    usage: Mutex<UsageService>,
    history: Mutex<UsageHistory>,
    pace: Mutex<PaceWatch>,
    view: Mutex<View>,
    commands: Mutex<Vec<Cmd>>,
    timing: Mutex<Timing>,
    http: Arc<dyn HttpClient>,
    secrets: Arc<dyn SecretStore>,
    fake: Option<Fake>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Hub {
    /// Mit dem Speicher des Systems und echtem Netz. `state` kommt aus den Einstellungen des Profils.
    pub fn system(state: AccountsState, history_path: PathBuf, cfg: &Config, now: f64) -> Hub {
        let http: Arc<dyn HttpClient> = Arc::new(kadrell_accounts::UreqClient);
        let secrets: Arc<dyn SecretStore> = Arc::from(kadrell_accounts::default_secret_store());
        Hub::with(state, history_path, cfg, now, http, secrets, None)
    }

    /// Erfundene Konten und Werte, kein Netz, kein Speicher des Systems (`KADRELL_DEV_USAGE=fake`).
    pub fn fake(now: f64) -> Hub {
        let fake = Fake::new(now);
        let http: Arc<dyn HttpClient> = Arc::new(kadrell_accounts::testing::FakeHttp::offline());
        let secrets: Arc<dyn SecretStore> = Arc::new(kadrell_accounts::MemorySecretStore::new());
        let cfg = Config { account: AccountSettings { temporary_profile: true, ..AccountSettings::default() }, pace_warning: false, notice: false, lang: super::lang() };
        let hub = Hub::with(fake.accounts.clone(), std::env::temp_dir().join("kadrell-fake-usage.jsonl"), &cfg, now, http, secrets, Some(fake));
        hub.publish(now, cfg.lang);
        hub
    }

    pub(crate) fn with(
        state: AccountsState,
        history_path: PathBuf,
        cfg: &Config,
        now: f64,
        http: Arc<dyn HttpClient>,
        secrets: Arc<dyn SecretStore>,
        fake: Option<Fake>,
    ) -> Hub {
        let mut service = AccountService::new(state, secrets.clone(), http.clone(), cfg.account.clone());
        let mut usage = UsageService::new(POLL_INTERVAL);
        let seed = fake.as_ref().map(|f| (f.usage.clone(), now)).or_else(|| active_cache(&service, now));
        if let Some((u, at)) = seed {
            usage.seed(u, at);
        }
        usage.start(now);
        service.set_settings(cfg.account.clone());
        let hub = Hub {
            service: Mutex::new(service),
            usage: Mutex::new(usage),
            history: Mutex::new(UsageHistory::new(history_path)),
            pace: Mutex::new(PaceWatch::default()),
            view: Mutex::new(View::default()),
            commands: Mutex::new(Vec::new()),
            timing: Mutex::new(Timing::default()),
            http,
            secrets,
            fake,
        };
        hub.publish(now, cfg.lang);
        hub
    }

    pub fn is_fake(&self) -> bool {
        self.fake.is_some()
    }

    /// Schnappschuss für die Oberfläche (billig, blockiert nie auf dem Netz).
    pub fn view(&self) -> View {
        lock(&self.view).clone()
    }

    pub fn request(&self, cmd: Cmd) {
        lock(&self.commands).push(cmd);
    }

    /// Messungen für die Statistik: alle gespeicherten, bei erfundenen Daten die erfundenen.
    pub fn samples(&self) -> Vec<UsageSample> {
        match &self.fake {
            Some(f) => f.samples.clone(),
            None => lock(&self.history).samples().to_vec(),
        }
    }

    pub fn fake_events(&self) -> Option<Vec<kadrell_accounts::PromptEvent>> {
        self.fake.as_ref().map(|f| f.events.clone())
    }

    /// Ein Durchlauf auf dem Hintergrund-Executor: Aufträge, Nutzungsabfrage, Wechsel, inaktive Konten, Schnappschuss.
    pub fn step(&self, now: f64, cfg: &Config) -> Step {
        let mut step = Step::default();
        if self.fake.is_some() {
            return step;
        }
        lock(&self.service).set_settings(cfg.account.clone());
        self.first_time(now);
        self.run_commands(now, cfg, &mut step);
        self.poll_usage(now, cfg, &mut step);
        self.poll_inactive(now);
        self.collect_events(now, cfg, &mut step);
        self.publish(now, cfg.lang);
        step
    }

    /// Einmal beim Start: Stufen der Konten und E-Mail des angemeldeten Kontos nachladen.
    fn first_time(&self, now: f64) {
        {
            let mut timing = lock(&self.timing);
            if std::mem::replace(&mut timing.started, true) {
                return;
            }
        }
        lock(&self.service).load_tiers(now);
        self.refresh_email();
    }

    fn refresh_email(&self) {
        let email = live_token(&*self.secrets).and_then(|t| fetch_profile(&*self.http, &t)).and_then(|p| p.email);
        lock(&self.view).current_email = email;
    }

    fn run_commands(&self, now: f64, cfg: &Config, step: &mut Step) {
        let cmds: Vec<Cmd> = std::mem::take(&mut *lock(&self.commands));
        for cmd in cmds {
            match cmd {
                Cmd::Switch(id) => {
                    lock(&self.service).switch_to(&id, now);
                }
                Cmd::Remove(id) => lock(&self.service).remove(&id),
                Cmd::AddCurrent => {
                    if !lock(&self.service).add_current(cfg.lang) {
                        step.notices.push(Notice::NoAccount);
                    }
                    self.refresh_email();
                }
                Cmd::RefreshEmail => self.refresh_email(),
            }
        }
    }

    fn poll_usage(&self, now: f64, cfg: &Config, step: &mut Step) {
        let account = lock(&self.service).active_id().map(str::to_string);
        let out = lock(&self.usage).tick(now, account.as_deref(), &*self.http, &*self.secrets);
        if let Some(out) = out {
            self.handle_outcome(&out, now, cfg, step);
        }
    }

    /// Nach jeder Messung: dem Konto merken (Menü, Verlauf); haben sich die Werte geändert, Auto-Wechsel und Warnung.
    fn handle_outcome(&self, out: &Outcome, now: f64, cfg: &Config, step: &mut Step) {
        let Some(measured) = &out.measured else { return };
        lock(&self.service).record_usage(measured, now);
        if !out.changed {
            return;
        }
        lock(&self.service).consider_auto_switch(measured, now);
        if let Some(warning) = lock(&self.pace).check(measured, cfg.pace_warning, now) {
            step.notices.push(Notice::Pace(warning));
        }
    }

    fn poll_inactive(&self, now: f64) {
        {
            let mut timing = lock(&self.timing);
            if now - timing.last_inactive < INACTIVE_EVERY {
                return;
            }
            timing.last_inactive = now;
        }
        lock(&self.service).refresh_inactive(INACTIVE_MAX_AGE, now);
    }

    /// Rückmeldungen des Kontendienstes: Verlauf, Speichern, Wechsel.
    fn collect_events(&self, now: f64, cfg: &Config, step: &mut Step) {
        let events = lock(&self.service).take_events();
        let mut changed = false;
        let mut switched = false;
        for event in events {
            match event {
                AccountEvent::Changed => changed = true,
                AccountEvent::ActiveChanged => switched = true,
                AccountEvent::AutoSwitched(account) if cfg.notice => step.notices.push(Notice::AutoSwitched(account.title(cfg.lang))),
                AccountEvent::AutoSwitched(_) => {}
                AccountEvent::UsageStored(id, cached) => lock(&self.history).append(UsageSample::from_cached(&id, &cached)),
            }
        }
        if switched {
            self.after_switch(now, cfg, step);
        }
        if changed || switched {
            step.persist = Some(self.persisted());
        }
    }

    /// Der aktive Account hat gewechselt: die Leiste zeigt sofort dessen letzten Stand, danach wird live nachgefragt.
    fn after_switch(&self, now: f64, cfg: &Config, step: &mut Step) {
        let cached = lock(&self.service).active().and_then(|a| a.usage.clone());
        lock(&self.usage).replace(cached.as_ref().map(|c| c.as_usage(now)).unwrap_or_default(), cached.map(|c| c.at));
        let account = lock(&self.service).active_id().map(str::to_string);
        let out = lock(&self.usage).refresh_now(now, account.as_deref(), &*self.http, &*self.secrets);
        self.handle_outcome(&out, now, cfg, step);
        // Die Messung kann weitere Rückmeldungen erzeugt haben (Merken, Speichern).
        for event in lock(&self.service).take_events() {
            if let AccountEvent::UsageStored(id, cached) = event {
                lock(&self.history).append(UsageSample::from_cached(&id, &cached));
            }
        }
    }

    fn persisted(&self) -> Persist {
        let service = lock(&self.service);
        let state = service.state();
        Persist { index: serde_json::to_string(&state.accounts).unwrap_or_else(|_| "[]".into()), active: state.active_id.clone() }
    }

    /// Schnappschuss neu bauen.
    fn publish(&self, now: f64, lang: kadrell_accounts::Lang) {
        let (menu, accounts, active_id, show_session, sources) = {
            let service = lock(&self.service);
            let tz = Local;
            (
                service.menu_items(now, &tz, lang),
                service.accounts().iter().map(|a| row(a, lang)).collect::<Vec<_>>(),
                service.active_id().map(str::to_string),
                service.active().is_none_or(Account::has_session_limit),
                service.limit_sources(now, &tz, lang),
            )
        };
        let (usage, measured_at) = {
            let u = lock(&self.usage);
            (u.usage().clone(), u.measured_at())
        };
        let mut view = lock(&self.view);
        let current_email = view.current_email.take();
        *view = View { usage, measured_at, show_session, menu, accounts, active_id, current_email, sources };
    }
}

fn row(a: &Account, lang: kadrell_accounts::Lang) -> AccountRow {
    AccountRow { id: a.id.clone(), title: a.title(lang), plan: a.plan_name(), email: a.email.clone() }
}

/// Letzter bekannter Stand des aktiven Kontos, zum Vorbelegen der Leiste.
fn active_cache(service: &AccountService, now: f64) -> Option<(Usage, f64)> {
    let cached = service.active()?.usage.as_ref()?;
    Some((cached.as_usage(now), cached.at))
}

/// `accounts.index` (JSON-Liste) und `accounts.active` der Einstellungen als Zustand des Dienstes.
pub fn state_from(index: Option<&str>, active: Option<String>) -> AccountsState {
    let accounts: Vec<Account> = index.and_then(|j| serde_json::from_str(j).ok()).unwrap_or_default();
    let active_id = active.filter(|id| accounts.iter().any(|a| &a.id == id));
    AccountsState { accounts, active_id }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kadrell_accounts::testing::FakeHttp;
    use kadrell_accounts::{LIVE_SERVICE, MemorySecretStore, STORE_SERVICE};

    const NOW: f64 = 1_800_000_000.;

    fn cfg() -> Config {
        Config {
            account: AccountSettings { temporary_profile: true, ..AccountSettings::default() },
            pace_warning: true,
            notice: true,
            lang: kadrell_accounts::Lang::De,
        }
    }

    fn live(token: &str) -> String {
        format!(r#"{{"claudeAiOauth":{{"accessToken":"{token}","refreshToken":"r","expiresAt":9999999999999,"subscriptionType":"max"}}}}"#)
    }

    fn hub(http: FakeHttp, secrets: MemorySecretStore, state: AccountsState) -> Hub {
        let path = std::env::temp_dir().join(format!("kadrell-hub-test-{}.jsonl", std::process::id()));
        Hub::with(state, path, &cfg(), NOW, Arc::new(http), Arc::new(secrets), None)
    }

    fn account(id: &str, email: &str) -> Account {
        Account { email: Some(email.into()), ..Account::new(id, "") }
    }

    #[test]
    fn state_round_trips_through_settings_text() {
        let accounts = vec![account("a1", "anna@example.com"), account("a2", "ben@example.org")];
        let json = serde_json::to_string(&accounts).unwrap();
        let state = state_from(Some(&json), Some("a2".into()));
        assert_eq!(state.accounts.len(), 2);
        assert_eq!(state.active_id.as_deref(), Some("a2"));
        assert_eq!(state_from(Some(&json), Some("gone".into())).active_id, None, "unbekannte aktive Id fällt weg");
        assert!(state_from(Some("kaputt"), None).accounts.is_empty());
    }

    #[test]
    fn fake_hub_never_touches_the_network_or_the_keychain() {
        let hub = Hub::fake(NOW);
        let view = hub.view();
        assert!(hub.is_fake());
        assert_eq!(view.accounts.len(), 2);
        assert!(view.accounts.iter().all(|a| a.email.as_deref().is_some_and(|e| e.contains("example"))));
        assert!(view.usage.weekly.is_some());
        assert!(hub.step(NOW + 1000., &cfg()).persist.is_none());
        hub.request(Cmd::Switch("x".into()));
        assert!(hub.step(NOW + 1000., &cfg()).notices.is_empty());
    }

    #[test]
    fn step_without_a_login_does_not_crash_and_shows_dashes() {
        let hub = hub(FakeHttp::offline(), MemorySecretStore::new(), AccountsState::default());
        let step = hub.step(NOW, &cfg());
        assert!(step.notices.is_empty());
        assert!(hub.view().usage.is_empty());
        assert!(hub.view().show_session);
    }

    #[test]
    fn add_current_without_login_reports_no_account() {
        let hub = hub(FakeHttp::offline(), MemorySecretStore::new(), AccountsState::default());
        hub.request(Cmd::AddCurrent);
        assert_eq!(hub.step(NOW, &cfg()).notices, vec![Notice::NoAccount]);
    }

    #[test]
    fn add_current_stores_a_slot_and_persists() {
        let secrets = MemorySecretStore::new().with(LIVE_SERVICE, "u", &live("tok-1"));
        let profile = r#"{"account":{"email":"anna@example.com"},"organization":{"rate_limit_tier":"default_claude_max_5x"}}"#;
        let hub = hub(FakeHttp::json(200, profile), secrets, AccountsState::default());
        hub.request(Cmd::AddCurrent);
        let step = hub.step(NOW, &cfg());
        let persist = step.persist.expect("gespeichert");
        assert!(persist.index.contains("anna@example.com"));
        assert!(persist.active.is_some());
        assert_eq!(hub.view().accounts.len(), 1);
        assert_eq!(hub.view().accounts[0].plan.as_deref(), Some("Max"));
    }

    #[test]
    fn remove_drops_the_slot_and_persists() {
        let secrets = MemorySecretStore::new().with(STORE_SERVICE, "a1", &live("tok-1"));
        let state = AccountsState { accounts: vec![account("a1", "anna@example.com")], active_id: None };
        let hub = hub(FakeHttp::offline(), secrets, state);
        hub.request(Cmd::Remove("a1".into()));
        let step = hub.step(NOW, &cfg());
        assert_eq!(step.persist.expect("gespeichert").index, "[]");
        assert!(hub.view().accounts.is_empty());
    }

    #[test]
    fn usage_poll_fills_the_view_and_records_history() {
        let secrets = MemorySecretStore::new().with(LIVE_SERVICE, "u", &live("tok-1"));
        let body = r#"{"five_hour":{"utilization":40,"resets_at":"2030-01-01T10:00:00Z"},"seven_day":{"utilization":25,"resets_at":"2030-01-05T10:00:00Z"}}"#;
        let state = AccountsState { accounts: vec![account("a1", "anna@example.com")], active_id: Some("a1".into()) };
        let hub = hub(FakeHttp::json(200, body), secrets, state);
        hub.step(NOW, &cfg());
        let view = hub.view();
        assert_eq!(view.usage.weekly, Some(25));
        assert_eq!(view.usage.session, Some(40));
        assert_eq!(view.measured_at, Some(NOW));
        assert_eq!(view.active_title().as_deref(), Some("anna@example.com"));
    }
}
