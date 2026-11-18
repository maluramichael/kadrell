//! Gemeinsame Typen der lokalen Modelle (Swift: `LocalProvider.swift`). Verweis und Anbieter kommen aus
//! `kadrell_claude::pi`, damit Pi-Sessions und Einstellungen denselben Wert `provider|name` sprechen.

pub use kadrell_claude::pi::{LocalModelRef, LocalProviderKind};

pub const PROVIDERS: [LocalProviderKind; 2] = [LocalProviderKind::Ollama, LocalProviderKind::LlamaCpp];

/// Name des Anbieters für die Anzeige (Eigennamen, nicht übersetzt).
pub fn provider_title(p: LocalProviderKind) -> &'static str {
    match p {
        LocalProviderKind::Ollama => "Ollama",
        LocalProviderKind::LlamaCpp => "llama.cpp",
    }
}

/// „Ollama · qwen3:4b“.
pub fn display(r: &LocalModelRef) -> String {
    format!("{} · {}", provider_title(r.provider), r.name)
}

/// Ein installiertes Modell, so wie der Anbieter es meldet. Nur was wirklich da ist, steht in den Listen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalModel {
    pub provider: LocalProviderKind,
    pub name: String,
    pub size: Option<i64>,
    /// Kurzinfo rechts in der Zeile: „4.0B · Q4_K_M“ (Ollama) bzw. Status „loaded“ (llama.cpp).
    pub detail: String,
    /// llama.cpp löscht nur Modelle aus seinem Cache, keine aus einem Modellordner oder Preset.
    pub can_remove: bool,
    /// llama.cpp listet laufende Downloads mit: noch nicht benutzbar.
    pub downloading: bool,
}

impl LocalModel {
    pub fn new(provider: LocalProviderKind, name: &str) -> LocalModel {
        LocalModel { provider, name: name.to_string(), size: None, detail: String::new(), can_remove: true, downloading: false }
    }

    pub fn reference(&self) -> LocalModelRef {
        LocalModelRef { provider: self.provider, name: self.name.clone() }
    }

    /// `provider|name`, Schlüssel in Einstellungen und Listen.
    pub fn id(&self) -> String {
        self.reference().string()
    }

    pub fn size_text(&self) -> String {
        self.size.map(byte_text).unwrap_or_default()
    }
}

/// Fortschritt eines Downloads. `fraction` fehlt, solange der Anbieter noch keine Gesamtgröße kennt.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PullProgress {
    pub status: String,
    pub completed: i64,
    pub total: i64,
}

impl PullProgress {
    pub fn fraction(&self) -> Option<f64> {
        (self.total > 0).then(|| (self.completed as f64 / self.total as f64).min(1.0))
    }

    /// „25 % · 250 MB / 1 GB“, ohne Gesamtgröße der Status des Anbieters (Swift: `LocalPullRows.progressText`).
    pub fn text(&self) -> String {
        match self.fraction() {
            None => self.status.clone(),
            Some(f) => format!("{} % · {} / {}", (f * 100.0) as i64, byte_text(self.completed), byte_text(self.total)),
        }
    }
}

/// Treffer der Modellsuche auf ollama.com.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchHit {
    pub name: String,
    pub summary: String,
    /// Größen-Tags wie „0.6b“, „4b“: daraus wird `name:tag` zum Laden.
    pub tags: Vec<String>,
}

/// Wie `ByteCountFormatter` mit `.file`: Zehnerpotenzen (1 GB = 1000 MB), eine Nachkommastelle ab MB.
pub fn byte_text(bytes: i64) -> String {
    let b = bytes.max(0) as f64;
    let (value, unit) = match b {
        b if b >= 1e12 => (b / 1e12, "TB"),
        b if b >= 1e9 => (b / 1e9, "GB"),
        b if b >= 1e6 => (b / 1e6, "MB"),
        b if b >= 1e3 => (b / 1e3, "KB"),
        b => return format!("{} bytes", b as i64),
    };
    let text = if value >= 100.0 || unit == "KB" { format!("{value:.0}") } else { format!("{value:.1}") };
    let text = if kadrell_i18n::global().lang() == kadrell_i18n::Lang::De { text.replace('.', ",") } else { text };
    format!("{text} {unit}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refs_roundtrip_and_reject_unknown() {
        let r = LocalModelRef { provider: LocalProviderKind::Ollama, name: "hf.co/unsloth/Qwen3.5-9B-GGUF:Q8_0".into() };
        assert_eq!(LocalModelRef::parse(&r.string()), Some(r.clone()));
        assert_eq!(LocalModelRef::parse("ollama|"), None);
        assert_eq!(LocalModelRef::parse("foo|bar"), None);
        assert_eq!(display(&r), "Ollama · hf.co/unsloth/Qwen3.5-9B-GGUF:Q8_0");
    }

    #[test]
    fn progress_fraction_and_text() {
        let p = PullProgress { status: "pulling".into(), completed: 250, total: 1000 };
        assert_eq!(p.fraction(), Some(0.25));
        assert_eq!(PullProgress { status: "pulling manifest".into(), ..Default::default() }.fraction(), None);
        assert_eq!(PullProgress { status: "x".into(), completed: 5, total: 2 }.fraction(), Some(1.0));
        let big = PullProgress { status: "x".into(), completed: 2_500_000_000, total: 5_000_000_000 };
        assert_eq!(big.text(), "50 % · 2,5 GB / 5,0 GB");
        assert_eq!(PullProgress { status: "pulling manifest".into(), ..Default::default() }.text(), "pulling manifest");
    }

    #[test]
    fn byte_text_uses_decimal_units() {
        assert_eq!(byte_text(2_500_000_000), "2,5 GB");
        assert_eq!(byte_text(291_000_000), "291 MB");
        assert_eq!(byte_text(1_400_000_000), "1,4 GB");
        assert_eq!(byte_text(12), "12 bytes");
    }
}
