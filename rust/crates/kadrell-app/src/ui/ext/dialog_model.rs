//! Zustand und Regeln des Extensions-Dialogs (`ExtensionsModel.swift`), ohne GPUI: Auswahl, An/Aus mit Bestätigung
//! beim ersten Einschalten, Einstellungsformular, Marktplatz. Das Netz läuft nur auf ausdrücklichen Befehl und über
//! einen `HttpClient`, den Tests durch einen Fake ersetzen.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

use kadrell_ext::bridge::{self, SettingsStore};
use kadrell_ext::github::{GitHubExtensions, RemoteExtension};
use kadrell_ext::http::HttpClient;
use kadrell_ext::manager::ExtensionManager;
use kadrell_ext::manifest::FoundExtension;
use kadrell_ext::supervisor::ExtensionState;
use kadrell_i18n::trd;
use serde_json::{Map, Value};

/// Taste ohne Modifier für das offene Panel (`ExtensionsModel.handleKey`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Up,
    Down,
    Space,
    Char(char),
}

/// Ergebnis von `toggle`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Toggle {
    /// Die Extension hat ein Problem und lässt sich nicht schalten.
    Blocked,
    /// Das erste Einschalten braucht die Bestätigung (`confirm`).
    NeedsConfirm,
    Done,
}

pub struct DialogModel {
    pub manager: ExtensionManager,
    store: Arc<dyn SettingsStore>,
    pub selected: usize,
    pub show_log: bool,
    /// Gesetzt, solange der Nutzer das erste Einschalten einer Extension bestätigen soll.
    pub confirm: Option<String>,
    /// Gespeicherte Werte der gewählten Extension ohne Geheimnisse; von denen steht nur hier, ob sie gesetzt sind.
    pub stored: Map<String, Value>,
    pub secrets_set: HashSet<String>,
    pub query: String,
    pub results: Vec<RemoteExtension>,
    pub searching: bool,
    pub installing: HashSet<String>,
    pub store_error: Option<String>,
    /// Pieptöne, die die Oberfläche noch spielen muss (`take_beeps`): gesperrter Schalter, R ohne Berechtigung.
    beeps: u32,
}

impl DialogModel {
    pub fn new(manager: ExtensionManager, store: Arc<dyn SettingsStore>) -> Self {
        let mut model = Self {
            manager,
            store,
            selected: 0,
            show_log: false,
            confirm: None,
            stored: Map::new(),
            secrets_set: HashSet::new(),
            query: String::new(),
            results: Vec::new(),
            searching: false,
            installing: HashSet::new(),
            store_error: None,
            beeps: 0,
        };
        model.load();
        model
    }

    pub fn items(&self) -> Vec<FoundExtension> {
        self.manager.found()
    }

    pub fn current(&self) -> Option<FoundExtension> {
        self.items().into_iter().nth(self.selected)
    }

    /// Eine Extension mit Problem (kaputtes Manifest, fremde Rechte) läuft gar nicht und lässt sich nicht schalten.
    pub fn can_toggle(ext: &FoundExtension) -> bool {
        ext.problem.is_none()
    }

    pub fn is_enabled(&self, name: &str) -> bool {
        bridge::enabled_extensions(&*self.store).iter().any(|n| n == name)
    }

    pub fn state(&self, name: &str) -> ExtensionState {
        self.manager.state(name)
    }

    pub fn select(&mut self, index: usize) {
        let count = self.items().len();
        if count == 0 {
            return;
        }
        let i = index.min(count - 1);
        if i != self.selected {
            self.selected = i;
            self.load();
        }
    }

    pub fn select_delta(&mut self, delta: isize) {
        self.select((self.selected as isize + delta).max(0) as usize);
    }

    /// Verschwundene Extension am Ende der Liste: die Auswahl rückt nach.
    pub fn clamp_selection(&mut self) {
        let count = self.items().len();
        if count > 0 && self.selected >= count {
            self.select(count - 1);
        }
    }

    // MARK: An und aus

    /// Schaltet um. Vor dem ersten Einschalten einmal bestätigen: fremder Code läuft ohne Sandbox mit den Rechten
    /// des Nutzers.
    pub fn toggle(&mut self, ext: &FoundExtension) -> Toggle {
        if !Self::can_toggle(ext) {
            self.beeps += 1;
            return Toggle::Blocked;
        }
        let enabling = !self.is_enabled(&ext.name);
        if enabling && !bridge::acknowledged_extensions(&*self.store).contains(&ext.name) {
            self.confirm = Some(ext.name.clone());
            return Toggle::NeedsConfirm;
        }
        self.manager.set_enabled(&ext.name, enabling);
        Toggle::Done
    }

    /// „Einschalten“ im Warnhinweis: Bestätigung merken und wirklich einschalten.
    pub fn confirm_yes(&mut self) {
        let Some(name) = self.confirm.take() else { return };
        bridge::acknowledge_extension(&*self.store, &name);
        self.manager.set_enabled(&name, true);
    }

    pub fn confirm_no(&mut self) {
        self.confirm = None;
    }

    /// Neu laden: nur eine eingeschaltete Extension ohne Problem. `false` sonst (der Aufrufer meldet es).
    pub fn reload(&mut self, ext: &FoundExtension) -> bool {
        if !Self::can_toggle(ext) || !self.is_enabled(&ext.name) {
            self.beeps += 1;
            return false;
        }
        self.manager.reload(&ext.name);
        true
    }

    /// ↑↓ wählen, Leertaste an/aus, R neu laden, L Log. `true`, wenn die Taste dem Dialog gehörte.
    pub fn handle_key(&mut self, key: Key) -> bool {
        match key {
            Key::Up => self.select_delta(-1),
            Key::Down => self.select_delta(1),
            Key::Space => {
                if let Some(ext) = self.current() {
                    self.toggle(&ext);
                }
            }
            Key::Char('r' | 'R') => {
                if let Some(ext) = self.current() {
                    self.reload(&ext);
                }
            }
            Key::Char('l' | 'L') => self.show_log = !self.show_log,
            Key::Char(_) => return false,
        }
        true
    }

    /// Wie viele Piepstöne seit dem letzten Aufruf fällig sind.
    pub fn take_beeps(&mut self) -> u32 {
        std::mem::take(&mut self.beeps)
    }

    // MARK: Einstellungen

    /// Liest die Werte der gewählten Extension neu; Geheimnisse bleiben draußen, nur „gesetzt“ wird gemerkt.
    pub fn load(&mut self) {
        let Some(ext) = self.current() else {
            self.stored = Map::new();
            self.secrets_set.clear();
            return;
        };
        let values = self.manager.settings().values(&ext);
        let secrets: HashSet<String> = ext
            .manifest
            .iter()
            .flat_map(|m| m.settings.iter())
            .filter(|s| s.kind == "secret")
            .map(|s| s.key.clone())
            .collect();
        self.secrets_set = secrets.iter().filter(|k| values.contains_key(*k)).cloned().collect();
        self.stored = values.into_iter().filter(|(k, _)| !secrets.contains(k)).collect();
    }

    /// Speichert einen Wert aus dem Formular. Eine eingeschaltete Extension startet danach neu, sonst sähe sie den
    /// neuen Wert in `kadrell.config` nicht.
    pub fn save(&mut self, ext: &FoundExtension, key: &str, value: &Value) {
        self.manager.settings().set(value, ext, key);
        if Self::can_toggle(ext) && self.is_enabled(&ext.name) {
            self.manager.reload(&ext.name);
        }
        self.load();
    }

    /// Wert des Formularfelds als Text (Zahl ohne überflüssiges „.0“, so wie jemand sie eintippen würde).
    pub fn field_text(&self, key: &str) -> String {
        self.stored.get(key).map(number_text).unwrap_or_default()
    }

    // MARK: Marktplatz

    /// Installiert, wenn der aus diesem Repo angelegte Ordner (über die erfasste Herkunft) noch da ist; sonst
    /// ersatzweise ein Ordner mit dem Repo-Namen (von Hand gelegt).
    pub fn is_installed(&self, remote: &RemoteExtension) -> bool {
        let items = self.items();
        let origin = bridge::extension_origins(&*self.store).into_iter().find(|(_, v)| *v == remote.full_name()).map(|(k, _)| k);
        match origin {
            Some(name) => items.iter().any(|f| f.name == name),
            None => items.iter().any(|f| f.name == remote.repo),
        }
    }

    pub fn origins(&self) -> HashMap<String, String> {
        bridge::extension_origins(&*self.store)
    }

    /// Nach einer Installation: Herkunft merken und den Katalog sofort neu einlesen.
    pub fn installed(&mut self, name: &str, remote: &RemoteExtension) {
        bridge::set_extension_origin(&*self.store, name, &remote.full_name());
        self.manager.refresh();
    }
}

/// Taste des Dialogs aus dem Namen der Taste (`ExtensionsModel.key(for:)`): nur ohne Modifier, Pfeile über ihren Namen,
/// sonst genau ein Zeichen.
pub fn key_from(name: &str, control: bool, alt: bool, platform: bool) -> Option<Key> {
    if control || alt || platform {
        return None;
    }
    match name {
        "up" => Some(Key::Up),
        "down" => Some(Key::Down),
        "space" => Some(Key::Space),
        k if k.chars().count() == 1 => k.chars().next().map(Key::Char),
        _ => None,
    }
}

/// Gehört die Taste dem Dialog? Nicht, solange ein Textfeld die Tastatur hat: dort gehören Leertaste und Buchstaben dem
/// Feld (`ExtensionsModel.panelKey`).
pub fn panel_key(name: &str, control: bool, alt: bool, platform: bool, field_focused: bool) -> Option<Key> {
    if field_focused { None } else { key_from(name, control, alt, platform) }
}

/// Speichert ein Feld nur, wenn der Text sich geändert hat; ein leeres Geheimnisfeld heißt „nicht angefasst“, nicht
/// „löschen“ (`SettingField.apply`).
pub fn should_commit(secret: bool, text: &str, current: &str) -> bool {
    if secret { !text.is_empty() } else { text != current }
}

/// Bündelt die Meldungen des Managers (bis zu 50 je Sekunde): sofort, wenn die letzte Weitergabe lang genug her ist,
/// sonst einmal am Ende der Wartezeit (`ExtensionsModel.changed`).
#[derive(Debug, Default)]
pub struct Coalescer {
    last: Option<std::time::Instant>,
    pending: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Publish {
    Now,
    After(std::time::Duration),
    /// Eine verspätete Weitergabe ist schon unterwegs.
    Skip,
}

impl Coalescer {
    pub const WAIT: std::time::Duration = std::time::Duration::from_millis(200);

    pub fn changed(&mut self, now: std::time::Instant) -> Publish {
        if self.pending {
            return Publish::Skip;
        }
        let since = self.last.map_or(Self::WAIT, |t| now.saturating_duration_since(t));
        if since >= Self::WAIT {
            self.last = Some(now);
            return Publish::Now;
        }
        self.pending = true;
        Publish::After(Self::WAIT - since)
    }

    /// Die verspätete Weitergabe ist erfolgt.
    pub fn flushed(&mut self, now: std::time::Instant) {
        self.pending = false;
        self.last = Some(now);
    }
}

/// Zahl ohne überflüssiges „.0“.
pub fn number_text(v: &Value) -> String {
    match v {
        Value::Number(n) => match n.as_f64() {
            Some(f) if f == f.round() && f.abs() < 1e15 => (f as i64).to_string(),
            _ => n.to_string(),
        },
        Value::String(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        _ => String::new(),
    }
}

/// Die Suche, blockierend: nur auf ausdrücklichen Befehl (Suchen-Taste oder ⏎), im Hintergrund.
pub fn run_search(client: Arc<dyn HttpClient>, query: &str) -> Result<Vec<RemoteExtension>, String> {
    GitHubExtensions::new(client).search(query).map_err(|e| trd!("Suche fehlgeschlagen: %@", e.to_string()))
}

/// Das Installieren, blockierend: nur auf Klick, im Hintergrund. Liefert den Ordnernamen.
pub fn run_install(
    client: Arc<dyn HttpClient>,
    remote: &RemoteExtension,
    catalog_dir: &Path,
    origins: &HashMap<String, String>,
) -> Result<String, String> {
    GitHubExtensions::new(client)
        .install(remote, catalog_dir, origins)
        .map_err(|e| trd!("Installieren fehlgeschlagen: %@", e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use kadrell_ext::bridge::MemoryStore;
    use kadrell_ext::http::{HttpError, HttpRequest, HttpResponse};
    use kadrell_ext::manager::ManagerConfig;
    use kadrell_ext::process::ProcessConfig;
    use kadrell_ext::testutil::{TempDir, make_extension};
    use std::sync::Mutex;

    struct Fixture {
        _dir: TempDir,
        model: DialogModel,
        store: Arc<MemoryStore>,
    }

    fn fixture(manifests: &[(&str, serde_json::Value)]) -> Fixture {
        let dir = TempDir::new("dlg");
        for (name, manifest) in manifests {
            make_extension(dir.path(), name, "", Some(manifest.clone()));
        }
        let store = Arc::new(MemoryStore::new());
        let config = ManagerConfig::new(dir.path(), ProcessConfig::new("kadrell-fake-exe"), dir.path().join("storage"));
        let manager = ExtensionManager::new(
            config,
            Arc::new(|_: &[String]| bridge::ControlResponse::ok("[]")),
            store.clone(),
            store.clone(),
            Arc::new(Vec::new),
        );
        manager.refresh();
        let model = DialogModel::new(manager, store.clone());
        Fixture { _dir: dir, model, store }
    }

    fn manifest(name: &str) -> serde_json::Value {
        serde_json::json!({"name": name, "version": "1.0", "apiVersion": 1})
    }

    fn with_settings(name: &str) -> serde_json::Value {
        serde_json::json!({"name": name, "apiVersion": 1, "settings": [
            {"key": "alle", "type": "number", "label": "Alle", "default": 5},
            {"key": "token", "type": "secret", "label": "Token"},
            {"key": "url", "type": "string", "label": "URL", "default": "x"}
        ]})
    }

    fn remote(owner: &str, repo: &str) -> RemoteExtension {
        RemoteExtension { owner: owner.into(), repo: repo.into(), description: String::new(), stars: 1, html_url: "https://github.com/x/y".into() }
    }

    #[test]
    fn lists_found_extensions_and_selection_clamps() {
        let mut f = fixture(&[("a", manifest("a")), ("b", manifest("b"))]);
        assert_eq!(f.model.items().iter().map(|e| e.name.as_str()).collect::<Vec<_>>(), ["a", "b"]);
        assert_eq!(f.model.current().unwrap().name, "a");
        f.model.select_delta(1);
        f.model.select_delta(1);
        assert_eq!(f.model.current().unwrap().name, "b", "am Ende bleibt es stehen");
        f.model.select_delta(-5);
        assert_eq!(f.model.current().unwrap().name, "a");
    }

    #[test]
    fn first_enable_asks_for_confirmation_and_remembers_it() {
        let mut f = fixture(&[("a", manifest("a"))]);
        let ext = f.model.current().unwrap();
        assert_eq!(f.model.toggle(&ext), Toggle::NeedsConfirm);
        assert!(!f.model.is_enabled("a"), "noch aus");
        f.model.confirm_no();
        assert!(f.model.confirm.is_none());
        assert!(!f.model.is_enabled("a"), "Abbrechen lässt sie aus");
        f.model.toggle(&ext);
        f.model.confirm_yes();
        assert!(f.model.is_enabled("a"));
        assert!(bridge::acknowledged_extensions(&*f.store).contains(&"a".to_string()));
        assert_eq!(f.model.toggle(&ext), Toggle::Done, "ausschalten fragt nicht");
        assert!(!f.model.is_enabled("a"));
        assert_eq!(f.model.toggle(&ext), Toggle::Done, "zweites Einschalten fragt nicht mehr");
        assert!(f.model.is_enabled("a"));
    }

    #[test]
    fn extension_with_problem_cannot_be_toggled() {
        let mut f = fixture(&[("a", manifest("kaputt"))]);
        let ext = f.model.current().expect("Ordner wird gefunden");
        assert!(ext.problem.is_some(), "Name passt nicht zum Ordner");
        assert_eq!(f.model.toggle(&ext), Toggle::Blocked);
        assert!(!f.model.reload(&ext));
    }

    #[test]
    fn keys_select_toggle_and_show_log() {
        let mut f = fixture(&[("a", manifest("a")), ("b", manifest("b"))]);
        assert!(f.model.handle_key(Key::Down));
        assert_eq!(f.model.selected, 1);
        assert!(f.model.handle_key(Key::Up));
        assert_eq!(f.model.selected, 0);
        assert!(f.model.handle_key(Key::Space));
        assert_eq!(f.model.confirm.as_deref(), Some("a"), "Leertaste schaltet, erstes Mal mit Bestätigung");
        assert!(f.model.handle_key(Key::Char('L')));
        assert!(f.model.show_log);
        assert!(f.model.handle_key(Key::Char('l')));
        assert!(!f.model.show_log);
        assert!(!f.model.handle_key(Key::Char('x')), "andere Tasten gehören dem Dialog nicht");
        assert!(f.model.handle_key(Key::Char('r')), "R gehört dem Dialog, auch wenn nichts läuft");
    }

    #[test]
    fn is_installed_uses_recorded_origin_then_repo_name() {
        let mut f = fixture(&[("tool", manifest("tool"))]);
        assert!(!f.model.is_installed(&remote("me", "other")));
        assert!(f.model.is_installed(&remote("me", "tool")), "ersatzweise Ordner mit dem Repo-Namen");
        f.model.installed("tool", &remote("me", "renamed"));
        assert!(f.model.is_installed(&remote("me", "renamed")), "über die erfasste Herkunft");
        assert!(f.model.is_installed(&remote("me", "tool")), "ohne eigene Herkunft gilt ersatzweise der Repo-Name");
        f.model.installed("gone", &remote("me", "weg"));
        assert!(!f.model.is_installed(&remote("me", "weg")), "Ordner ist nicht mehr da");
    }

    #[test]
    fn sample_extensions_are_valid_and_load_without_problem() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../kadrell-ext/examples/extensions");
        let found = kadrell_ext::manifest::scan(&dir);
        let names: Vec<&str> = found.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["cpu-footer", "panel-demo"]);
        for f in &found {
            assert!(f.problem.is_none(), "{}: {:?}", f.name, f.problem);
            assert!(f.manifest.is_some());
        }
    }

    #[test]
    fn number_text_drops_trailing_zero() {
        assert_eq!(number_text(&serde_json::json!(5)), "5");
        assert_eq!(number_text(&serde_json::json!(5.0)), "5");
        assert_eq!(number_text(&serde_json::json!(2.5)), "2.5");
        assert_eq!(number_text(&serde_json::json!("abc")), "abc");
    }

    #[test]
    fn secrets_only_show_whether_they_are_set() {
        let mut f = fixture(&[("a", with_settings("a"))]);
        assert_eq!(f.model.field_text("alle"), "5", "Default");
        assert!(f.model.secrets_set.is_empty());
        let ext = f.model.current().unwrap();
        f.model.save(&ext, "token", &Value::from("geheim"));
        assert!(f.model.secrets_set.contains("token"));
        assert!(!f.model.stored.contains_key("token"), "der Wert selbst bleibt draußen");
        assert!(!format!("{:?}", f.model.stored).contains("geheim"));
        f.model.save(&ext, "token", &Value::from(""));
        assert!(!f.model.secrets_set.contains("token"), "leer löscht");
    }

    #[test]
    fn saving_stores_the_value_and_enabled_extension_reloads() {
        let mut f = fixture(&[("a", with_settings("a"))]);
        let ext = f.model.current().unwrap();
        f.model.save(&ext, "alle", &Value::from("7,5"));
        assert_eq!(f.model.field_text("alle"), "7.5", "Komma wird zu Punkt");
        f.model.save(&ext, "alle", &Value::from("kaputt"));
        assert_eq!(f.model.field_text("alle"), "5", "ungültig heißt Default");
        f.model.save(&ext, "url", &Value::from("https://x"));
        assert_eq!(f.model.field_text("url"), "https://x");
        // Eingeschaltet: ein Speichern fordert den Neustart an (Zustand Reloading oder Starting statt Off).
        bridge::acknowledge_extension(&*f.store, "a");
        f.model.toggle(&ext);
        f.model.save(&ext, "url", &Value::from("https://y"));
        assert_ne!(f.model.state("a"), ExtensionState::Off);
    }

    struct FakeHttp {
        status: u16,
        body: &'static str,
        calls: Mutex<Vec<String>>,
    }

    impl HttpClient for FakeHttp {
        fn send(&self, request: HttpRequest) -> Result<HttpResponse, HttpError> {
            self.calls.lock().unwrap().push(request.url);
            Ok(HttpResponse { status: self.status, headers: Vec::new(), body: self.body.as_bytes().to_vec() })
        }
    }

    #[test]
    fn search_goes_through_the_injected_client() {
        let body = r#"{"items":[{"name":"kadrell-example","owner":{"login":"maluramichael"},"description":"Beispiel","stargazers_count":7,"html_url":"https://github.com/maluramichael/kadrell-example"}]}"#;
        let fake = Arc::new(FakeHttp { status: 200, body, calls: Mutex::new(Vec::new()) });
        let found = run_search(fake.clone(), "beispiel").unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].full_name(), "maluramichael/kadrell-example");
        assert_eq!(found[0].stars, 7);
        let calls = fake.calls.lock().unwrap();
        assert_eq!(calls.len(), 1, "genau eine Anfrage, nur auf Befehl");
        assert!(calls[0].contains("topic:kadrell-extension") && calls[0].contains("beispiel"), "{}", calls[0]);
    }

    #[test]
    fn rate_limit_is_an_error_not_an_empty_list() {
        let fake = Arc::new(FakeHttp { status: 403, body: "{}", calls: Mutex::new(Vec::new()) });
        let err = run_search(fake, "x").unwrap_err();
        assert!(err.contains("Download") || err.contains("download"), "{err}");
    }

    #[test]
    fn install_failure_reports_without_touching_the_catalog() {
        let dir = TempDir::new("inst");
        let fake = Arc::new(FakeHttp { status: 404, body: "", calls: Mutex::new(Vec::new()) });
        let err = run_install(fake, &remote("a", "b"), dir.path(), &HashMap::new()).unwrap_err();
        assert!(!err.is_empty());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    /// `testKeyFromEvent`: nur Tasten ohne Modifier, Pfeile über ihren Namen.
    #[test]
    fn keys_need_no_modifier_and_arrows_use_their_name() {
        assert_eq!(key_from("up", false, false, false), Some(Key::Up));
        assert_eq!(key_from("down", false, false, false), Some(Key::Down));
        assert_eq!(key_from("space", false, false, false), Some(Key::Space));
        assert_eq!(key_from("r", false, false, false), Some(Key::Char('r')));
        assert_eq!(key_from("r", true, false, false), None);
        assert_eq!(key_from("r", false, true, false), None);
        assert_eq!(key_from("r", false, false, true), None);
        assert_eq!(key_from("enter", false, false, false), None, "Namen von Sondertasten sind keine Zeichen");
    }

    /// `testPanelKeysPassThroughWhileEditing`: im Textfeld gehören Leertaste, R und L dem Feld.
    #[test]
    fn panel_keys_belong_to_a_focused_field() {
        for name in ["space", "r", "l"] {
            assert_eq!(panel_key(name, false, false, false, true), None, "{name} ins Feld");
            assert!(panel_key(name, false, false, false, false).is_some(), "{name} an den Dialog");
        }
    }

    /// `testSpaceTogglesSelected`: Leertaste schaltet die gewählte Extension (das erste Mal mit Bestätigung).
    #[test]
    fn space_toggles_the_selected_extension() {
        let mut f = fixture(&[("a", manifest("a")), ("b", manifest("b"))]);
        bridge::acknowledge_extension(&*f.store, "b");
        assert!(f.model.handle_key(Key::Down));
        assert!(f.model.handle_key(Key::Space));
        assert!(f.model.is_enabled("b"));
        assert!(!f.model.is_enabled("a"));
    }

    /// `testReloadKeyRestartsRunning`: R lädt nur eine eingeschaltete Extension neu, sonst gibt es den Piepton.
    #[test]
    fn reload_key_needs_an_enabled_extension_and_beeps_otherwise() {
        let mut f = fixture(&[("a", manifest("a"))]);
        assert!(f.model.handle_key(Key::Char('r')));
        assert_eq!(f.model.take_beeps(), 1, "aus: Piepton");
        bridge::acknowledge_extension(&*f.store, "a");
        f.model.handle_key(Key::Space);
        assert!(f.model.is_enabled("a"));
        f.model.handle_key(Key::Char('R'));
        assert_eq!(f.model.take_beeps(), 0, "an: kein Piepton");
        assert_ne!(f.model.state("a"), ExtensionState::Off);
    }

    #[test]
    fn blocked_switch_beeps() {
        let mut f = fixture(&[("a", manifest("kaputt"))]);
        let ext = f.model.current().unwrap();
        assert_eq!(f.model.toggle(&ext), Toggle::Blocked);
        assert_eq!(f.model.take_beeps(), 1);
        assert_eq!(f.model.take_beeps(), 0, "einmal abgeholt");
    }

    /// `testClosingDiscardsUncommittedDraft` und `testMovingToAnotherFieldCommits`: gespeichert wird nur Geändertes,
    /// ein leeres Geheimnisfeld heißt „nicht angefasst“.
    #[test]
    fn only_changed_text_is_committed_and_an_empty_secret_is_untouched() {
        assert!(should_commit(false, "8", "5"));
        assert!(!should_commit(false, "5", "5"));
        assert!(should_commit(false, "", "5"), "ein leeres Textfeld löscht den Wert");
        assert!(should_commit(true, "neu", ""));
        assert!(!should_commit(true, "", ""));
    }

    /// `testChangesAreCoalesced`: höchstens alle 0,2 s eine Weitergabe.
    #[test]
    fn changes_are_coalesced_to_one_per_200_ms() {
        use std::time::{Duration, Instant};
        let start = Instant::now();
        let mut c = Coalescer::default();
        assert_eq!(c.changed(start), Publish::Now, "die erste geht sofort");
        let soon = start + Duration::from_millis(50);
        assert_eq!(c.changed(soon), Publish::After(Duration::from_millis(150)));
        assert_eq!(c.changed(soon), Publish::Skip, "eine verspätete ist schon unterwegs");
        c.flushed(start + Duration::from_millis(200));
        assert_eq!(c.changed(start + Duration::from_millis(500)), Publish::Now);
    }
}
