//! Funktionen mit lokalem Modell und ihre Einstellungen (Swift: `LocalModelSettings.swift`). Die Werte stehen wie alle
//! anderen im Profil (`kadrell_core::Settings`, Schlüssel `local.*`); hier ist ein unveränderlicher Schnappschuss davon,
//! den die Hintergrund-Threads ohne Sperre lesen.

use std::time::Duration;

use kadrell_claude::pi::{PI_DEFAULT_SETTING, PiModel};
use kadrell_core::Settings;

use crate::types::{LocalModelRef, LocalProviderKind};

/// Adresse des Ollama-Dienstes, wenn nichts eingestellt ist. `127.0.0.1` statt `localhost`: Windows löst `localhost`
/// zuerst zu `::1` auf, dort lauscht Ollama nicht, und jede Anfrage wartet erst auf den Fehlschlag.
pub const DEFAULT_OLLAMA_URL: &str = "http://127.0.0.1:11434";

/// Funktionen von Kadrell, die ein lokales Modell nutzen können. Jede ist für sich an- und abschaltbar.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Feature {
    QuickStart,
    PiSession,
    SessionTitle,
}

impl Feature {
    pub const ALL: [Feature; 3] = [Feature::QuickStart, Feature::PiSession, Feature::SessionTitle];

    /// Teil des Einstellungsschlüssels (`local.feature.<raw>.enabled`).
    pub fn raw(self) -> &'static str {
        match self {
            Feature::QuickStart => "quickStart",
            Feature::PiSession => "piSession",
            Feature::SessionTitle => "sessionTitle",
        }
    }

    /// Wie lange die Funktion auf das Modell wartet, bevor sie ohne Modell weitermacht. Pi-Sessions fragen nie selbst.
    pub fn timeout(self) -> Duration {
        match self {
            Feature::QuickStart => kadrell_claude::local_model::QUICK_START_TIMEOUT,
            Feature::PiSession => Duration::ZERO,
            Feature::SessionTitle => kadrell_claude::local_model::SESSION_TITLE_TIMEOUT,
        }
    }

    /// Modell, das Kadrell zum Laden anbietet, wenn die Funktion noch keins hat (gemessen: 0,1 s je Titel).
    pub fn suggested_model(self) -> Option<LocalModelRef> {
        match self {
            Feature::SessionTitle => Some(LocalModelRef { provider: LocalProviderKind::Ollama, name: "qwen3:1.7b".into() }),
            Feature::QuickStart | Feature::PiSession => None,
        }
    }

    fn key(self, part: &str) -> String {
        format!("local.feature.{}.{part}", self.raw())
    }
}

/// Einstellungen der lokalen Modelle zu einem Zeitpunkt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalSettings {
    pub ollama_enabled: bool,
    pub ollama_url: String,
    pub llama_enabled: bool,
    /// Pfad zu `llama-server`, leer = automatisch.
    pub llama_binary: String,
    /// Zusätzlicher Ordner mit GGUF-Dateien (`--models-dir`), leer = nur der Cache von llama.cpp.
    pub llama_models_dir: String,
    /// Adresse eines schon laufenden `llama-server`, leer = Kadrell startet ihn selbst.
    pub llama_url: String,
    features: Vec<(Feature, bool, String)>,
}

impl Default for LocalSettings {
    fn default() -> Self {
        LocalSettings::from_settings(&Settings::default())
    }
}

impl LocalSettings {
    pub fn from_settings(s: &Settings) -> LocalSettings {
        let features = Feature::ALL.iter().map(|&f| (f, s.bool_or(&f.key("enabled"), false), s.string_or(&f.key("model"), ""))).collect();
        LocalSettings {
            ollama_enabled: s.bool_or("local.ollama.enabled", true),
            ollama_url: s.string_or("local.ollama.url", DEFAULT_OLLAMA_URL),
            llama_enabled: s.bool_or("local.llama.enabled", false),
            llama_binary: s.string_or("local.llama.binary", ""),
            llama_models_dir: s.string_or("local.llama.modelsDir", ""),
            llama_url: s.string_or("local.llama.url", ""),
            features,
        }
    }

    pub fn enabled(&self, p: LocalProviderKind) -> bool {
        match p {
            LocalProviderKind::Ollama => self.ollama_enabled,
            LocalProviderKind::LlamaCpp => self.llama_enabled,
        }
    }

    fn feature(&self, f: Feature) -> (bool, &str) {
        self.features.iter().find(|(x, _, _)| *x == f).map_or((false, ""), |(_, on, model)| (*on, model.as_str()))
    }

    pub fn feature_enabled(&self, f: Feature) -> bool {
        self.feature(f).0
    }

    /// Gespeicherter Wert der Modellauswahl: `provider|name`, leer = kein Modell, bei Pi auch `pi`.
    pub fn feature_model_setting(&self, f: Feature) -> &str {
        self.feature(f).1
    }

    pub fn feature_model(&self, f: Feature) -> Option<LocalModelRef> {
        LocalModelRef::parse(self.feature_model_setting(f))
    }

    /// Modell einer Funktion, wenn sie an ist und ihr Anbieter eingeschaltet. Ob das Modell noch installiert ist,
    /// zeigt erst die Anfrage: fehlt es, scheitert sie und die Funktion macht ohne Modell weiter.
    pub fn active_model(&self, f: Feature) -> Option<LocalModelRef> {
        let model = self.feature_model(f).filter(|m| self.enabled(m.provider))?;
        self.feature_enabled(f).then_some(model)
    }

    /// Womit Pi-Sessions starten, `None` = Funktion aus oder kein Modell gewählt.
    pub fn pi_model(&self) -> Option<PiModel> {
        if !self.feature_enabled(Feature::PiSession) {
            return None;
        }
        if self.feature_model_setting(Feature::PiSession) == PI_DEFAULT_SETTING {
            return Some(PiModel::PiDefault);
        }
        self.active_model(Feature::PiSession).map(PiModel::Local)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn settings(pairs: &[(&str, serde_json::Value)]) -> Settings {
        let mut s = Settings::default();
        for (k, v) in pairs {
            s.set(k, v.clone());
        }
        s
    }

    #[test]
    fn defaults_match_the_mac_app() {
        let l = LocalSettings::default();
        assert!(l.ollama_enabled && !l.llama_enabled);
        assert_eq!(l.ollama_url, DEFAULT_OLLAMA_URL);
        assert!(Feature::ALL.iter().all(|&f| !l.feature_enabled(f) && l.active_model(f).is_none()));
        assert_eq!(l.pi_model(), None);
    }

    #[test]
    fn active_model_needs_feature_model_and_provider() {
        let on = settings(&[("local.feature.sessionTitle.enabled", json!(true)), ("local.feature.sessionTitle.model", json!("ollama|qwen3:1.7b"))]);
        let m = LocalSettings::from_settings(&on).active_model(Feature::SessionTitle).unwrap();
        assert_eq!((m.provider, m.name.as_str()), (LocalProviderKind::Ollama, "qwen3:1.7b"));
        let provider_off = settings(&[("local.feature.sessionTitle.enabled", json!(true)), ("local.feature.sessionTitle.model", json!("ollama|m")), ("local.ollama.enabled", json!(false))]);
        assert_eq!(LocalSettings::from_settings(&provider_off).active_model(Feature::SessionTitle), None);
        let feature_off = settings(&[("local.feature.sessionTitle.model", json!("ollama|m"))]);
        assert_eq!(LocalSettings::from_settings(&feature_off).active_model(Feature::SessionTitle), None);
        let unknown = settings(&[("local.feature.sessionTitle.enabled", json!(true)), ("local.feature.sessionTitle.model", json!("foo|m"))]);
        assert_eq!(LocalSettings::from_settings(&unknown).active_model(Feature::SessionTitle), None);
    }

    #[test]
    fn pi_model_is_default_or_local() {
        let base = [("local.feature.piSession.enabled", json!(true))];
        let mut pairs = base.to_vec();
        pairs.push(("local.feature.piSession.model", json!("pi")));
        assert_eq!(LocalSettings::from_settings(&settings(&pairs)).pi_model(), Some(PiModel::PiDefault));
        let mut pairs = base.to_vec();
        pairs.push(("local.feature.piSession.model", json!("llamacpp|m")));
        pairs.push(("local.llama.enabled", json!(true)));
        let model = LocalSettings::from_settings(&settings(&pairs)).pi_model();
        assert_eq!(model.and_then(|m| m.local().map(|r| r.name.clone())), Some("m".to_string()));
        assert_eq!(LocalSettings::from_settings(&settings(&base)).pi_model(), None, "ohne Modell keine Pi-Session");
    }

    #[test]
    fn timeouts_and_suggestions() {
        assert_eq!(Feature::QuickStart.timeout(), Duration::from_secs(12));
        assert_eq!(Feature::SessionTitle.timeout(), Duration::from_secs(8));
        assert_eq!(Feature::PiSession.timeout(), Duration::ZERO);
        assert_eq!(Feature::SessionTitle.suggested_model().unwrap().string(), "ollama|qwen3:1.7b");
        assert!(Feature::QuickStart.suggested_model().is_none());
    }
}
