//! Zustand und Steuerung der lokalen Anbieter für Einstellungen und Funktionen (Swift: `LocalModels.swift`): welche
//! Modelle wirklich installiert sind, laufende Downloads, Suche auf ollama.com und die Anfrage einer Funktion an ihr
//! Modell. Alles Blockierende läuft auf eigenen Threads, die Oberfläche liest nur Schnappschüsse und fragt
//! `generation()`, ob sich etwas geändert hat.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, MutexGuard, RwLock};
use std::time::Duration;

use kadrell_claude::local_model::ModelError;
use kadrell_i18n::trd;
use serde_json::Value;

use crate::http::{self, Http, describe, get};
use crate::llama::{LlamaClient, LlamaServer, autodetect};
use crate::ollama::{self, OllamaClient, PullEnd};
use crate::settings::{Feature, LocalSettings};
use crate::types::{LocalModel, LocalModelRef, LocalProviderKind, PROVIDERS, PullProgress, SearchHit};

/// Zustand eines Anbieters in den Einstellungen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    Off,
    Checking,
    Ready(String),
    Failed(String),
}

/// Ein laufender oder gescheiterter Download, sichtbar in den Einstellungen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pull {
    pub reference: LocalModelRef,
    pub progress: PullProgress,
    pub error: Option<String>,
}

#[derive(Default)]
struct State {
    status: HashMap<&'static str, Status>,
    models: HashMap<&'static str, Vec<LocalModel>>,
    pulls: Vec<Pull>,
    cancels: HashMap<String, Arc<AtomicBool>>,
    hits: Vec<SearchHit>,
    searching: bool,
    search_error: Option<String>,
    action_error: Option<String>,
}

struct Inner {
    http: Arc<dyn Http>,
    home: String,
    settings: RwLock<LocalSettings>,
    env: RwLock<BTreeMap<String, String>>,
    state: Mutex<State>,
    server: LlamaServer,
    generation: AtomicU64,
    later: [AtomicU64; 2],
    search_token: AtomicU64,
}

/// Gemeinsamer Handle (billig zu klonen), lebt so lange wie die App.
#[derive(Clone)]
pub struct LocalModels {
    inner: Arc<Inner>,
}

fn slot(p: LocalProviderKind) -> usize {
    match p {
        LocalProviderKind::Ollama => 0,
        LocalProviderKind::LlamaCpp => 1,
    }
}

impl LocalModels {
    /// `profile_dir` hält die pid des eigenen `llama-server` (nur Unix), `home` ist `~` für Pfade.
    pub fn new(http: Arc<dyn Http>, settings: LocalSettings, env: BTreeMap<String, String>, home: &str, profile_dir: Option<PathBuf>) -> LocalModels {
        let server = LlamaServer::new(profile_dir.map(|d| d.join("llama-server.pid")));
        LocalModels {
            inner: Arc::new(Inner {
                http,
                home: home.to_string(),
                settings: RwLock::new(settings),
                env: RwLock::new(env),
                state: Mutex::new(State::default()),
                server,
                generation: AtomicU64::new(0),
                later: [AtomicU64::new(0), AtomicU64::new(0)],
                search_token: AtomicU64::new(0),
            }),
        }
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.inner.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Zählt bei jeder sichtbaren Änderung hoch.
    pub fn generation(&self) -> u64 {
        self.inner.generation.load(Ordering::SeqCst)
    }

    fn changed(&self) {
        self.inner.generation.fetch_add(1, Ordering::SeqCst);
    }

    fn change(&self, f: impl FnOnce(&mut State)) {
        f(&mut self.state());
        self.changed();
    }

    pub fn settings(&self) -> LocalSettings {
        self.inner.settings.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Neue Einstellungen übernehmen. Ändert sich etwas, was die Anbieter betrifft, lesen sie nach kurzer Pause neu
    /// (Adressfelder prüfen kurz nach dem Tippen von selbst, nicht bei jedem Zeichen).
    pub fn set_settings(&self, new: LocalSettings) {
        let old = std::mem::replace(&mut *self.inner.settings.write().unwrap_or_else(|e| e.into_inner()), new.clone());
        for p in PROVIDERS {
            if provider_part(&old, p) != provider_part(&new, p) {
                let delay = if old.enabled(p) != new.enabled(p) { Duration::ZERO } else { Duration::from_millis(700) };
                self.refresh_later(p, delay);
            }
        }
        self.changed();
    }

    /// Login-Shell-Umgebung (PATH, `LLAMA_CACHE`, `HF_TOKEN`), setzt die App nach dem Start.
    pub fn set_environment(&self, env: BTreeMap<String, String>) {
        *self.inner.env.write().unwrap_or_else(|e| e.into_inner()) = env;
    }

    fn environment(&self) -> BTreeMap<String, String> {
        self.inner.env.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    // MARK: Lesen

    pub fn status(&self, p: LocalProviderKind) -> Status {
        self.state().status.get(p.raw()).cloned().unwrap_or(Status::Off)
    }

    pub fn models(&self, p: LocalProviderKind) -> Vec<LocalModel> {
        self.state().models.get(p.raw()).cloned().unwrap_or_default()
    }

    /// Benutzbare Modelle aller eingeschalteten Anbieter, zur Auswahl je Funktion.
    pub fn available(&self) -> Vec<LocalModel> {
        let settings = self.settings();
        PROVIDERS.iter().filter(|p| settings.enabled(**p)).flat_map(|p| self.models(*p)).filter(|m| !m.downloading).collect()
    }

    pub fn pulls(&self) -> Vec<Pull> {
        self.state().pulls.clone()
    }

    /// Treffer, ob gerade gesucht wird, Fehler der Suche.
    pub fn search_state(&self) -> (Vec<SearchHit>, bool, Option<String>) {
        let s = self.state();
        (s.hits.clone(), s.searching, s.search_error.clone())
    }

    /// Letzter Fehler beim Löschen, bis zur nächsten Aktion.
    pub fn action_error(&self) -> Option<String> {
        self.state().action_error.clone()
    }

    // MARK: Anbieter

    pub fn refresh_all(&self) {
        PROVIDERS.iter().for_each(|p| self.refresh(*p));
    }

    pub fn refresh(&self, p: LocalProviderKind) {
        let this = self.clone();
        std::thread::spawn(move || this.refresh_blocking(p));
    }

    /// Nach `delay` neu lesen, außer es kam inzwischen eine neuere Anforderung.
    fn refresh_later(&self, p: LocalProviderKind, delay: Duration) {
        let token = self.inner.later[slot(p)].fetch_add(1, Ordering::SeqCst) + 1;
        let this = self.clone();
        std::thread::spawn(move || {
            std::thread::sleep(delay);
            if this.inner.later[slot(p)].load(Ordering::SeqCst) == token {
                this.refresh_blocking(p);
            }
        });
    }

    pub fn refresh_blocking(&self, p: LocalProviderKind) {
        if !self.settings().enabled(p) {
            self.change(|s| {
                s.status.insert(p.raw(), Status::Off);
                s.models.insert(p.raw(), Vec::new());
            });
            if p == LocalProviderKind::LlamaCpp {
                self.inner.server.stop();
            }
            return;
        }
        if self.models(p).is_empty() {
            self.change(|s| {
                s.status.insert(p.raw(), Status::Checking);
            });
        }
        match self.read_provider(p) {
            Ok((status, models)) => self.change(|s| {
                s.status.insert(p.raw(), Status::Ready(status));
                s.models.insert(p.raw(), models);
            }),
            Err(e) => self.change(|s| {
                s.status.insert(p.raw(), Status::Failed(describe(&e)));
                s.models.insert(p.raw(), Vec::new());
            }),
        }
    }

    fn read_provider(&self, p: LocalProviderKind) -> Result<(String, Vec<LocalModel>), ModelError> {
        let http = self.inner.http.as_ref();
        match p {
            LocalProviderKind::Ollama => {
                let c = self.ollama()?;
                Ok((format!("Ollama {}", c.version()?), c.models()?))
            }
            LocalProviderKind::LlamaCpp => {
                let base = self.llama_base()?;
                let own = http::base_url(&self.settings().llama_url).is_none() && self.inner.server.is_running();
                let status = if own { trd!("von Kadrell gestartet · %@", base.as_str()) } else { base.clone() };
                Ok((status, LlamaClient { base, http }.models()?))
            }
        }
    }

    pub fn ollama(&self) -> Result<OllamaClient<'_>, ModelError> {
        let base = http::base_url(&self.settings().ollama_url).ok_or_else(|| ModelError::Invalid(trd!("Adresse ungültig")))?;
        Ok(OllamaClient { base, http: self.inner.http.as_ref() })
    }

    /// Eingestellte Adresse eines laufenden Servers, sonst der eigene Kindprozess (bei Bedarf gestartet, blockiert
    /// bis zu 15 s).
    pub fn llama_base(&self) -> Result<String, ModelError> {
        let settings = self.settings();
        if let Some(url) = http::base_url(&settings.llama_url) {
            return Ok(url);
        }
        let binary = self.llama_binary();
        self.inner.server.ensure(&binary, &settings.llama_models_dir, &self.environment(), &self.inner.home, self.inner.http.as_ref())
    }

    /// Adresse des Anbieters ohne Warten: Ollama wie eingestellt, llama.cpp nur, wenn eingestellt oder schon gestartet.
    pub fn base_url_now(&self, p: LocalProviderKind) -> Option<String> {
        let settings = self.settings();
        match p {
            LocalProviderKind::Ollama => http::base_url(&settings.ollama_url),
            LocalProviderKind::LlamaCpp => http::base_url(&settings.llama_url).or_else(|| self.inner.server.url()),
        }
    }

    /// Pfad zu `llama-server` laut Einstellung oder automatisch gefunden (leer: nicht gefunden).
    pub fn llama_binary(&self) -> String {
        let setting = self.settings().llama_binary;
        let setting = setting.trim();
        if setting.is_empty() {
            return autodetect(&self.environment(), &self.inner.home, &|p| std::path::Path::new(p).is_file());
        }
        kadrell_claude::paths::expand_tilde(setting, &self.inner.home)
    }

    /// Läuft schon ein eigener `llama-server`?
    pub fn llama_running(&self) -> bool {
        self.inner.server.is_running()
    }

    /// Kadrell endet: eigener `llama-server` mit, Downloads abbrechen (Ollama setzt sie beim nächsten Mal fort).
    pub fn shutdown(&self) {
        for flag in self.state().cancels.values() {
            flag.store(true, Ordering::SeqCst);
        }
        self.inner.server.stop();
    }

    /// Server eines früheren, abgestürzten Laufs desselben Profils wegräumen (Unix; Windows macht das Job Object).
    pub fn sweep_stale(&self) {
        self.inner.server.sweep_stale();
    }

    // MARK: Modelle laden und löschen

    /// Modell aus der Ollama-Bibliothek laden (eigener Thread). llama.cpp lädt nicht über Kadrell.
    pub fn pull(&self, name: &str) {
        let Some((reference, flag)) = self.begin_pull(name) else { return };
        let this = self.clone();
        std::thread::spawn(move || this.run_pull(&reference, &flag));
    }

    /// Trägt den Download ein; `None`, wenn der Name leer ist oder das Modell schon geladen wird.
    pub fn begin_pull(&self, name: &str) -> Option<(LocalModelRef, Arc<AtomicBool>)> {
        let name = name.trim();
        let reference = LocalModelRef { provider: LocalProviderKind::Ollama, name: name.to_string() };
        let key = reference.string();
        if name.is_empty() || self.state().cancels.contains_key(&key) {
            return None;
        }
        let flag = Arc::new(AtomicBool::new(false));
        let progress = PullProgress { status: trd!("startet …"), ..Default::default() };
        self.change(|s| {
            s.pulls.retain(|p| p.reference != reference);
            s.pulls.push(Pull { reference: reference.clone(), progress, error: None });
            s.cancels.insert(key, flag.clone());
        });
        Some((reference, flag))
    }

    /// Der Download selbst, blockiert bis zum Ende.
    pub fn run_pull(&self, reference: &LocalModelRef, flag: &AtomicBool) {
        let result = self.ollama().and_then(|c| c.pull(&reference.name, &mut |p| self.set_progress(reference, flag, p)));
        self.change(|s| {
            match &result {
                Ok(PullEnd::Done | PullEnd::Cancelled) => s.pulls.retain(|p| p.reference != *reference),
                Err(e) => {
                    if let Some(p) = s.pulls.iter_mut().find(|p| p.reference == *reference) {
                        p.error = Some(describe(e));
                    }
                }
            }
            s.cancels.remove(&reference.string());
        });
        self.refresh_blocking(reference.provider);
    }

    fn set_progress(&self, reference: &LocalModelRef, flag: &AtomicBool, progress: PullProgress) -> bool {
        if flag.load(Ordering::SeqCst) {
            return false;
        }
        self.change(|s| {
            if let Some(p) = s.pulls.iter_mut().find(|p| p.reference == *reference) {
                p.progress = progress;
            }
        });
        true
    }

    /// Laufenden Download abbrechen bzw. eine Fehlermeldung wegnehmen.
    pub fn cancel_pull(&self, reference: &LocalModelRef) {
        self.change(|s| {
            if let Some(flag) = s.cancels.remove(&reference.string()) {
                flag.store(true, Ordering::SeqCst);
            }
            s.pulls.retain(|p| p.reference != *reference);
        });
    }

    pub fn delete(&self, model: &LocalModel) {
        let this = self.clone();
        let model = model.clone();
        std::thread::spawn(move || this.delete_blocking(&model));
    }

    pub fn delete_blocking(&self, m: &LocalModel) {
        self.change(|s| s.action_error = None);
        let result = match m.provider {
            LocalProviderKind::Ollama => self.ollama().and_then(|c| c.delete(&m.name)),
            LocalProviderKind::LlamaCpp => self.llama_base().and_then(|base| LlamaClient { base, http: self.inner.http.as_ref() }.delete(&m.name)),
        };
        if let Err(e) = result {
            self.change(|s| s.action_error = Some(format!("{}: {}", m.name, describe(&e))));
        }
        // Gelöschtes Modell einer Funktion: Auswahl bleibt stehen, die Funktion fällt ohne Modell zurück.
        self.refresh_blocking(m.provider);
    }

    // MARK: Suche

    /// Sucht auf ollama.com (eigener Thread); eine ältere Antwort überschreibt keine neuere.
    pub fn search(&self, query: &str) {
        let token = self.inner.search_token.fetch_add(1, Ordering::SeqCst) + 1;
        let query = query.trim().to_string();
        if query.is_empty() {
            self.change(|s| {
                s.hits.clear();
                s.search_error = None;
                s.searching = false;
            });
            return;
        }
        self.change(|s| s.searching = true);
        let this = self.clone();
        std::thread::spawn(move || this.search_blocking(&query, token));
    }

    pub fn search_blocking(&self, query: &str, token: u64) {
        let result = ollama::search(self.inner.http.as_ref(), query);
        if self.inner.search_token.load(Ordering::SeqCst) != token {
            return;
        }
        self.change(|s| {
            s.searching = false;
            match result {
                Ok(hits) => {
                    s.search_error = hits.is_empty().then(|| trd!("nichts gefunden"));
                    s.hits = hits;
                }
                Err(e) => {
                    s.hits.clear();
                    s.search_error = Some(describe(&e));
                }
            }
        });
    }

    // MARK: Funktionen

    /// Modell einer Funktion, wenn sie an ist und ihr Anbieter eingeschaltet.
    pub fn active_model(&self, f: Feature) -> Option<LocalModelRef> {
        self.settings().active_model(f)
    }

    /// Modell der Funktion schon einmal laden, ohne zu warten.
    pub fn warm_up(&self, f: Feature) {
        let Some(reference) = self.active_model(f) else { return };
        let this = self.clone();
        std::thread::spawn(move || match reference.provider {
            LocalProviderKind::Ollama => {
                if let Ok(c) = this.ollama() {
                    c.preload(&reference.name);
                }
            }
            LocalProviderKind::LlamaCpp => {
                if let Ok(base) = this.llama_base() {
                    LlamaClient { base, http: this.inner.http.as_ref() }.load(&reference.name);
                }
            }
        });
    }

    /// Eine Antwort des Modells der Funktion, höchstens `timeout` lang (Laden inklusive). Blockiert: auf einem
    /// Hintergrund-Thread rufen. `Schema` bekommt der Anbieter als Antwortformat mit.
    pub fn complete(&self, f: Feature, system: &str, user: &str, schema: Option<&Value>, timeout: Duration) -> Result<String, ModelError> {
        let Some(reference) = self.active_model(f) else { return Err(ModelError::Invalid(trd!("kein Modell gewählt"))) };
        let (this, system, user, schema) = (self.clone(), system.to_string(), user.to_string(), schema.cloned());
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(this.ask(&reference, &system, &user, schema.as_ref(), timeout));
        });
        rx.recv_timeout(timeout).unwrap_or(Err(ModelError::Timeout))
    }

    fn ask(&self, r: &LocalModelRef, system: &str, user: &str, schema: Option<&Value>, timeout: Duration) -> Result<String, ModelError> {
        match r.provider {
            LocalProviderKind::Ollama => self.ollama()?.chat(&r.name, system, user, schema, timeout),
            LocalProviderKind::LlamaCpp => {
                let base = self.llama_base()?;
                LlamaClient { base, http: self.inner.http.as_ref() }.chat(&r.name, system, user, schema, timeout)
            }
        }
    }

    /// Läuft Ollama (1 s Limit)? Sonst Ollama.app im Hintergrund starten und bis zu 10 s warten (nur macOS). Nur für
    /// ein Modell der Funktion bei Ollama auf dem eigenen Rechner. Fehlt die App, passiert nichts.
    pub fn ensure_ollama_running(&self, f: Feature) {
        let Some(base) = self.active_model(f).filter(|m| m.provider == LocalProviderKind::Ollama).and_then(|_| self.base_url_now(LocalProviderKind::Ollama)) else { return };
        if !is_local(&http::host_of(&base)) || self.reachable(&base) || !crate::launcher::launch_ollama_app() {
            return;
        }
        for _ in 0..10 {
            std::thread::sleep(Duration::from_secs(1));
            if self.reachable(&base) {
                return;
            }
        }
    }

    fn reachable(&self, base: &str) -> bool {
        http::call(self.inner.http.as_ref(), &get(format!("{base}/api/version"), Duration::from_secs(1))).is_ok()
    }

    /// Das Modell einer Funktion als `LocalModel` für `session_title` und den Schnellstart.
    pub fn for_feature(&self, f: Feature, schema: Option<Value>) -> FeatureModel {
        FeatureModel { models: self.clone(), feature: f, schema }
    }
}

fn is_local(host: &str) -> bool {
    ["localhost", "127.0.0.1", "::1"].contains(&host)
}

/// Der Teil der Einstellungen, der einen Anbieter betrifft: ändert er sich, liest der Anbieter neu.
fn provider_part(s: &LocalSettings, p: LocalProviderKind) -> (bool, String, String, String) {
    match p {
        LocalProviderKind::Ollama => (s.ollama_enabled, s.ollama_url.clone(), String::new(), String::new()),
        LocalProviderKind::LlamaCpp => (s.llama_enabled, s.llama_binary.clone(), s.llama_models_dir.clone(), s.llama_url.clone()),
    }
}

/// Das Modell einer Funktion, verbunden mit dem Trait von `kadrell-claude`.
pub struct FeatureModel {
    models: LocalModels,
    feature: Feature,
    schema: Option<Value>,
}

impl kadrell_claude::local_model::LocalModel for FeatureModel {
    fn ask(&self, system: &str, user: &str, timeout: Duration) -> Result<String, ModelError> {
        if self.feature == Feature::SessionTitle {
            self.models.ensure_ollama_running(self.feature);
        }
        self.models.complete(self.feature, system, user, self.schema.as_ref(), timeout)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::TransportError;
    use crate::http::fake::Fake;
    use kadrell_claude::local_model::LocalModel as _;
    use kadrell_core::Settings;
    use serde_json::json;

    fn settings(pairs: &[(&str, Value)]) -> LocalSettings {
        let mut s = Settings::default();
        for (k, v) in pairs {
            s.set(k, v.clone());
        }
        LocalSettings::from_settings(&s)
    }

    fn hub(fake: Arc<Fake>, settings: LocalSettings) -> LocalModels {
        LocalModels::new(fake, settings, BTreeMap::new(), "/h", None)
    }

    fn ollama_fake() -> Arc<Fake> {
        Arc::new(Fake::new(|r| {
            let path = r.url.split("11434").nth(1).unwrap_or("");
            match path {
                "/api/version" => Ok((200, "{\"version\":\"0.9.0\"}".into())),
                "/api/tags" => Ok((200, "{\"models\":[{\"name\":\"b:1\",\"size\":1000000000},{\"name\":\"a:1\",\"size\":5}]}".into())),
                "/api/chat" => Ok((200, "{\"message\":{\"content\":\"Titel\"}}".into())),
                "/api/delete" => Ok((200, String::new())),
                _ => Ok((404, String::new())),
            }
        }))
    }

    #[test]
    fn refresh_reads_version_and_models() {
        let models = hub(ollama_fake(), LocalSettings::default());
        assert_eq!(models.status(LocalProviderKind::Ollama), Status::Off, "vor dem ersten Lesen");
        models.refresh_blocking(LocalProviderKind::Ollama);
        assert_eq!(models.status(LocalProviderKind::Ollama), Status::Ready("Ollama 0.9.0".into()));
        let names: Vec<String> = models.models(LocalProviderKind::Ollama).into_iter().map(|m| m.name).collect();
        assert_eq!(names, ["a:1", "b:1"]);
        assert_eq!(models.available().len(), 2);
    }

    #[test]
    fn disabled_provider_is_off_and_empty() {
        let models = hub(ollama_fake(), settings(&[("local.ollama.enabled", json!(false))]));
        models.refresh_blocking(LocalProviderKind::Ollama);
        assert_eq!(models.status(LocalProviderKind::Ollama), Status::Off);
        assert!(models.available().is_empty());
    }

    #[test]
    fn unreachable_provider_fails_with_a_readable_message() {
        let fake = Arc::new(Fake::new(|_| Err(TransportError::Unreachable("refused".into()))));
        let models = hub(fake, LocalSettings::default());
        models.refresh_blocking(LocalProviderKind::Ollama);
        assert_eq!(models.status(LocalProviderKind::Ollama), Status::Failed("nicht erreichbar: 127.0.0.1".into()));
        assert!(models.models(LocalProviderKind::Ollama).is_empty());
    }

    #[test]
    fn invalid_address_is_reported() {
        let models = hub(ollama_fake(), settings(&[("local.ollama.url", json!("  "))]));
        models.refresh_blocking(LocalProviderKind::Ollama);
        assert_eq!(models.status(LocalProviderKind::Ollama), Status::Failed("Adresse ungültig".into()));
    }

    #[test]
    fn llama_with_configured_address_lists_models() {
        let fake = Arc::new(Fake::ok("{\"data\":[{\"id\":\"m\",\"status\":{\"value\":\"loaded\"},\"can_remove\":true},{\"id\":\"d\",\"status\":{\"value\":\"downloading\"}}]}"));
        let models = hub(fake, settings(&[("local.llama.enabled", json!(true)), ("local.llama.url", json!("127.0.0.1:8080/"))]));
        models.refresh_blocking(LocalProviderKind::LlamaCpp);
        assert_eq!(models.status(LocalProviderKind::LlamaCpp), Status::Ready("http://127.0.0.1:8080".into()));
        assert_eq!(models.models(LocalProviderKind::LlamaCpp).len(), 2);
        assert_eq!(models.available().len(), 1, "der Download zählt nicht");
        assert_eq!(models.base_url_now(LocalProviderKind::LlamaCpp).as_deref(), Some("http://127.0.0.1:8080"));
    }

    #[test]
    fn llama_without_binary_does_not_start_anything() {
        let models = hub(Arc::new(Fake::ok("")), settings(&[("local.llama.enabled", json!(true)), ("local.llama.binary", json!("/gibt/es/nicht"))]));
        models.refresh_blocking(LocalProviderKind::LlamaCpp);
        assert!(matches!(models.status(LocalProviderKind::LlamaCpp), Status::Failed(m) if m.contains("llama-server nicht gefunden")));
        assert!(!models.llama_running());
        assert_eq!(models.base_url_now(LocalProviderKind::LlamaCpp), None);
    }

    #[test]
    fn pull_tracks_progress_and_clears_on_success() {
        let fake = Arc::new(Fake::new(|r| {
            if r.url.ends_with("/api/pull") {
                return Ok((200, "{\"status\":\"pulling manifest\"}\n{\"status\":\"pulling a\",\"total\":100,\"completed\":50}\n{\"status\":\"success\"}\n".into()));
            }
            Ok((200, "{\"models\":[],\"version\":\"1\"}".into()))
        }));
        let models = hub(fake, LocalSettings::default());
        assert!(models.begin_pull("   ").is_none());
        let (reference, flag) = models.begin_pull(" qwen3:0.6b ").unwrap();
        assert_eq!(reference.name, "qwen3:0.6b");
        assert_eq!(models.pulls()[0].progress.status, "startet …");
        assert!(models.begin_pull("qwen3:0.6b").is_none(), "läuft schon");
        models.run_pull(&reference, &flag);
        assert!(models.pulls().is_empty());
        assert!(models.begin_pull("qwen3:0.6b").is_some(), "danach wieder möglich");
    }

    #[test]
    fn failed_pull_stays_with_its_error_until_dismissed() {
        let fake = Arc::new(Fake::new(|r| {
            if r.url.ends_with("/api/pull") {
                return Ok((200, "{\"error\":\"pull model manifest: file does not exist\"}\n".into()));
            }
            Ok((200, "{\"models\":[],\"version\":\"1\"}".into()))
        }));
        let models = hub(fake, LocalSettings::default());
        let (reference, flag) = models.begin_pull("gibtsnicht").unwrap();
        models.run_pull(&reference, &flag);
        let pulls = models.pulls();
        assert!(pulls[0].error.as_deref().unwrap().contains("does not exist"));
        models.cancel_pull(&reference);
        assert!(models.pulls().is_empty());
    }

    #[test]
    fn cancelled_pull_disappears_and_stops_reading() {
        let fake = Arc::new(Fake::new(|r| {
            if r.url.ends_with("/api/pull") {
                return Ok((200, "{\"status\":\"a\"}\n{\"status\":\"b\"}\n{\"status\":\"success\"}\n".into()));
            }
            Ok((200, "{\"models\":[],\"version\":\"1\"}".into()))
        }));
        let models = hub(fake, LocalSettings::default());
        let (reference, flag) = models.begin_pull("x").unwrap();
        models.cancel_pull(&reference);
        assert!(flag.load(Ordering::SeqCst));
        models.run_pull(&reference, &flag);
        assert!(models.pulls().is_empty());
    }

    #[test]
    fn delete_reports_errors_and_refreshes() {
        let fake = Arc::new(Fake::new(|r| {
            if r.url.ends_with("/api/delete") {
                return Ok((404, "{\"error\":\"model 'x' not found\"}".into()));
            }
            Ok((200, "{\"models\":[],\"version\":\"1\"}".into()))
        }));
        let models = hub(fake, LocalSettings::default());
        models.delete_blocking(&LocalModel::new(LocalProviderKind::Ollama, "x"));
        assert_eq!(models.action_error().as_deref(), Some("x: HTTP 404: model 'x' not found"));
        assert_eq!(models.status(LocalProviderKind::Ollama), Status::Ready("Ollama 1".into()));
        models.delete_blocking(&LocalModel::new(LocalProviderKind::Ollama, "y"));
    }

    #[test]
    fn search_keeps_only_the_latest_answer() {
        let fake = Arc::new(Fake::ok("<a href=\"/library/qwen3\"></a>"));
        let models = hub(fake, LocalSettings::default());
        let old = models.inner.search_token.fetch_add(1, Ordering::SeqCst) + 1;
        let token = models.inner.search_token.fetch_add(1, Ordering::SeqCst) + 1;
        models.search_blocking("qwen", old);
        assert!(models.search_state().0.is_empty(), "veraltete Antwort wird verworfen");
        models.search_blocking("qwen", token);
        let (hits, searching, error) = models.search_state();
        assert_eq!(hits[0].name, "qwen3");
        assert!(!searching && error.is_none());
        let empty = hub(Arc::new(Fake::ok("<html></html>")), LocalSettings::default());
        empty.search_blocking("nix", empty.inner.search_token.load(Ordering::SeqCst));
        assert_eq!(empty.search_state().2.as_deref(), Some("nichts gefunden"));
    }

    #[test]
    fn feature_model_asks_the_configured_model() {
        let on = settings(&[("local.feature.sessionTitle.enabled", json!(true)), ("local.feature.sessionTitle.model", json!("ollama|qwen3:1.7b"))]);
        let fake = ollama_fake();
        let models = hub(fake.clone(), on);
        let reply = models.for_feature(Feature::SessionTitle, None).ask("system", "user", Duration::from_secs(5)).unwrap();
        assert_eq!(reply, "Titel");
        let body = fake.body_json(fake.requests().len() - 1);
        assert_eq!(body["model"], "qwen3:1.7b");
        assert_eq!(body["messages"][1]["content"], "user");
    }

    #[test]
    fn without_a_model_the_feature_fails_fast_and_a_slow_model_times_out() {
        let models = hub(ollama_fake(), LocalSettings::default());
        let err = models.for_feature(Feature::QuickStart, None).ask("s", "u", Duration::from_secs(1)).unwrap_err();
        assert_eq!(err, ModelError::Invalid("kein Modell gewählt".into()));
        let slow = Arc::new(Fake::new(|_| {
            std::thread::sleep(Duration::from_millis(1500));
            Ok((200, "{\"message\":{\"content\":\"zu spät\"}}".into()))
        }));
        let on = settings(&[("local.feature.quickStart.enabled", json!(true)), ("local.feature.quickStart.model", json!("ollama|m"))]);
        let started = std::time::Instant::now();
        let err = hub(slow, on).complete(Feature::QuickStart, "s", "u", None, Duration::from_millis(200)).unwrap_err();
        assert_eq!(err, ModelError::Timeout);
        assert!(started.elapsed() < Duration::from_millis(1200), "wartet nicht auf die Antwort");
    }

    #[test]
    fn settings_change_bumps_generation_and_schedules_a_refresh() {
        let models = hub(ollama_fake(), LocalSettings::default());
        let before = models.generation();
        models.set_settings(settings(&[("local.ollama.enabled", json!(false))]));
        assert!(models.generation() > before);
        assert!(!models.settings().ollama_enabled);
        for _ in 0..50 {
            if models.status(LocalProviderKind::Ollama) == Status::Off && models.generation() > before + 1 {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(models.status(LocalProviderKind::Ollama), Status::Off);
    }

    #[test]
    fn warm_up_without_a_model_does_nothing() {
        let fake = ollama_fake();
        hub(fake.clone(), LocalSettings::default()).warm_up(Feature::PiSession);
        std::thread::sleep(Duration::from_millis(50));
        assert!(fake.requests().is_empty());
    }
}
