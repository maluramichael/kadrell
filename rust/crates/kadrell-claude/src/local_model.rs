//! Schnittstelle zu einem lokalen Modell (Ollama, llama.cpp). Die HTTP-Anbindung selbst gehört nicht in dieses Crate;
//! Sessiontitel und Schnellstart fragen nur über den Trait und warten höchstens `Feature::timeout`
//! (Swift: `LocalModelError` und `LocalFeature.timeout` in `Services/LocalModels`).

use std::time::Duration;

/// Fehler einer Modellanfrage.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModelError {
    Unreachable(String),
    Http(u16, String),
    Invalid(String),
    Timeout,
}

impl std::fmt::Display for ModelError {
    /// Deutscher Grundtext wie im Swift-Quelltext; die Oberfläche übersetzt über das Sprachbündel.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ModelError::Unreachable(s) => write!(f, "nicht erreichbar: {s}"),
            ModelError::Http(code, s) if s.is_empty() => write!(f, "HTTP {code}"),
            ModelError::Http(code, s) => write!(f, "HTTP {code}: {s}"),
            ModelError::Invalid(s) => write!(f, "{s}"),
            ModelError::Timeout => write!(f, "keine Antwort in der Zeit"),
        }
    }
}

/// Wie lange eine Funktion auf das Modell wartet, bevor sie ohne Modell weitermacht.
pub const QUICK_START_TIMEOUT: Duration = Duration::from_secs(12);
pub const SESSION_TITLE_TIMEOUT: Duration = Duration::from_secs(8);

/// Eine Anfrage an das Modell der jeweiligen Funktion (System- und Nutzertext), blockierend, höchstens `timeout`
/// (Laden des Modells inklusive). Der Aufrufer legt sie auf einen Hintergrund-Thread.
pub trait LocalModel {
    fn ask(&self, system: &str, user: &str, timeout: Duration) -> Result<String, ModelError>;
}
