//! Daten, die der Einstellungsdialog nicht selbst besitzt: Claude-Konten und lokale Modelle. Der Dialog spricht
//! nur mit dem Trait `Backend`; echte Daten kommen später aus `kadrell-accounts` und der Anbindung der lokalen
//! Modelle. Bis dahin liefert `MockBackend` feste Beispieldaten (MOCK, nichts davon ist echt).

use super::model::Provider;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountInfo {
    pub id: String,
    pub email: String,
    pub plan: Option<String>,
}

/// Zustand eines Anbieters lokaler Modelle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderStatus {
    Off,
    Checking,
    Ready(String),
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    /// Größe und Zustand, gedämpft rechts neben dem Namen.
    pub detail: String,
    pub can_remove: bool,
}

/// Ein laufender oder gescheiterter Download.
#[derive(Debug, Clone, PartialEq)]
pub struct PullInfo {
    /// `provider|name`.
    pub id: String,
    pub name: String,
    /// Fortschritt als Text („25 % · 250 MB / 1 GB“) oder die Fehlermeldung.
    pub text: String,
    pub fraction: Option<f64>,
    pub failed: bool,
}

/// Treffer der Modellsuche auf ollama.com.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HitInfo {
    pub name: String,
    pub summary: String,
    pub tags: Vec<String>,
}

/// Stand der Suche: Treffer, ob gerade gesucht wird, Fehler oder „nichts gefunden“.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SearchInfo {
    pub hits: Vec<HitInfo>,
    pub searching: bool,
    pub error: Option<String>,
}

pub trait Backend {
    fn accounts(&self) -> Vec<AccountInfo>;
    fn active_account(&self) -> Option<String>;
    /// E-Mail des gerade angemeldeten Kontos, damit „Hinzufügen“ weiß, wer hinzukäme.
    fn current_email(&self) -> Option<String>;
    fn switch_account(&mut self, id: &str);
    fn remove_account(&mut self, id: &str);
    /// Das angemeldete Konto aufnehmen (die Mock-Daten brauchen das nicht).
    fn add_account(&mut self) {}
    fn status(&self, provider: Provider) -> ProviderStatus;
    fn models(&self, provider: Provider) -> Vec<ModelInfo>;
    fn delete_model(&mut self, id: &str);
    fn reload(&mut self, provider: Provider);
    /// Modelle für die Auswahl bei den Funktionen: (`provider|name`, Anzeige).
    fn available_models(&self) -> Vec<(String, String)>;
    /// Laufende und gescheiterte Downloads (Ollama).
    fn pulls(&self) -> Vec<PullInfo> {
        Vec::new()
    }
    /// Modell aus der Ollama-Bibliothek laden.
    fn pull(&mut self, _name: &str) {}
    /// Download abbrechen bzw. eine Fehlermeldung wegnehmen (`id` = `provider|name`).
    fn cancel_pull(&mut self, _id: &str) {}
    fn search(&mut self, _query: &str) {}
    fn search_state(&self) -> SearchInfo {
        SearchInfo::default()
    }
    /// Letzter Fehler beim Löschen.
    fn action_error(&self) -> Option<String> {
        None
    }
    /// Zählt hoch, wenn sich Daten im Hintergrund geändert haben: der Dialog zeichnet dann neu.
    fn generation(&self) -> u64 {
        0
    }
    /// Gefundener Pfad von `llama-server` (Platzhalter des Feldes), `None` = nicht gefunden oder unbekannt.
    fn llama_detected(&self) -> Option<String> {
        None
    }
    /// Pis eigenes Standardmodell (`provider/modell`) für die Auswahl „Standard von Pi“.
    fn pi_default(&self) -> Option<String> {
        None
    }
}

/// Wert in den Einstellungen (`provider|name`): `ollama` oder `llamacpp`.
pub fn raw(p: Provider) -> &'static str {
    match p {
        Provider::Ollama => "ollama",
        Provider::LlamaCpp => "llamacpp",
    }
}

/// Konten aus `kadrell-accounts` und lokale Modelle aus `kadrell-models` (siehe `usage::backend`, `models::backend`).
/// Ohne verdrahtete Konten (Tests) gilt der Mock.
pub fn default_backend(cx: &gpui::App) -> Box<dyn Backend> {
    crate::usage::backend::create(cx).unwrap_or_else(|| Box::new(MockBackend::new()))
}

/// MOCK: feste Beispieldaten mit erfundenen Adressen.
pub struct MockBackend {
    accounts: Vec<AccountInfo>,
    active: Option<String>,
    models: Vec<(Provider, ModelInfo)>,
}

impl MockBackend {
    pub fn new() -> MockBackend {
        let account = |id: &str, email: &str| AccountInfo { id: id.to_string(), email: email.to_string(), plan: Some("Ultra".into()) };
        let model = |p, name: &str, detail: &str| (p, ModelInfo { id: format!("{}|{name}", raw(p)), name: name.to_string(), detail: detail.to_string(), can_remove: true });
        MockBackend {
            accounts: vec![account("a1", "anna@example.com"), account("a2", "ben@example.org")],
            active: Some("a2".to_string()),
            models: vec![
                model(Provider::Ollama, "qwen3:8b", "5,2 GB"),
                model(Provider::Ollama, "qwen3:1.7b", "1,4 GB"),
                model(Provider::LlamaCpp, "ggml-org/Qwen2.5-Coder-7B-Q8_0-GGUF:Q8_0", "entladen"),
                model(Provider::LlamaCpp, "example/nemotron-3.5-asr-streaming-0.6b-gguf:Q8_0", "entladen"),
                model(Provider::LlamaCpp, "example/parakeet-unified-en-0.6b-gguf:Q8_0", "entladen"),
                model(Provider::LlamaCpp, "example/Voxtral-Mini-4B-Realtime-2602-gguf:Q5_K_M", "entladen"),
                model(Provider::LlamaCpp, "example/whisper-large-v3-turbo-gguf:Q8_0", "entladen"),
            ],
        }
    }
}

impl Default for MockBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl Backend for MockBackend {
    fn accounts(&self) -> Vec<AccountInfo> {
        self.accounts.clone()
    }

    fn active_account(&self) -> Option<String> {
        self.active.clone()
    }

    fn current_email(&self) -> Option<String> {
        let id = self.active.as_deref()?;
        self.accounts.iter().find(|a| a.id == id).map(|a| a.email.clone())
    }

    fn switch_account(&mut self, id: &str) {
        if self.accounts.iter().any(|a| a.id == id) {
            self.active = Some(id.to_string());
        }
    }

    fn remove_account(&mut self, id: &str) {
        self.accounts.retain(|a| a.id != id);
        if self.active.as_deref() == Some(id) {
            self.active = None;
        }
    }

    fn status(&self, provider: Provider) -> ProviderStatus {
        match provider {
            Provider::Ollama => ProviderStatus::Ready("Ollama 0.9.0 · 127.0.0.1:11434".into()),
            Provider::LlamaCpp => ProviderStatus::Ready("von Kadrell gestartet · 127.0.0.1:59060".into()),
        }
    }

    fn models(&self, provider: Provider) -> Vec<ModelInfo> {
        self.models.iter().filter(|(p, _)| *p == provider).map(|(_, m)| m.clone()).collect()
    }

    fn delete_model(&mut self, id: &str) {
        self.models.retain(|(_, m)| m.id != id);
    }

    fn reload(&mut self, _provider: Provider) {}

    fn available_models(&self) -> Vec<(String, String)> {
        let label = |p: Provider| if p == Provider::Ollama { "Ollama" } else { "llama.cpp" };
        self.models.iter().map(|(p, m)| (m.id.clone(), format!("{} · {}", label(*p), m.name))).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_switch_and_remove() {
        let mut b = MockBackend::new();
        assert_eq!(b.active_account().as_deref(), Some("a2"));
        b.switch_account("a1");
        assert_eq!(b.current_email().as_deref(), Some("anna@example.com"));
        b.remove_account("a1");
        assert_eq!(b.accounts().len(), 1);
        assert_eq!(b.active_account(), None);
    }

    #[test]
    fn mock_delete_model_only_removes_that_one() {
        let mut b = MockBackend::new();
        let before = b.models(Provider::LlamaCpp).len();
        let id = b.models(Provider::LlamaCpp)[0].id.clone();
        b.delete_model(&id);
        assert_eq!(b.models(Provider::LlamaCpp).len(), before - 1);
        assert_eq!(b.models(Provider::Ollama).len(), 2);
    }
}
