//! Lokale Modelle für Kadrell, ohne Oberfläche: Ollama (laufender Dienst) und llama.cpp (`llama-server`, bei Bedarf
//! als Kindprozess von Kadrell im Router-Modus gestartet, endet mit Kadrell). Portiert aus
//! `app/Kadrell/Services/LocalModels` (Swift ist die Quelle der Wahrheit), die Swift-Tests laufen als Unit-Tests mit.
//!
//! Modelle gibt es für drei Funktionen: Schnellstart, Pi-Sessions und Sessiontitel (`settings::Feature`). HTTP steckt
//! hinter dem Trait [`http::Http`], Tests laufen mit einer Attrappe und laden oder löschen nie ein echtes Modell.
//! [`hub::LocalModels`] hält den Zustand für die Einstellungen, [`hub::FeatureModel`] verbindet das Modell einer
//! Funktion mit `kadrell_claude::local_model::LocalModel` (Schnellstart 12 s, Sessiontitel 8 s).

pub mod guard;
pub mod http;
pub mod hub;
pub mod launcher;
pub mod llama;
pub mod ollama;
pub mod settings;
pub mod types;

pub use http::{Http, UreqHttp};
pub use hub::{FeatureModel, LocalModels, Pull, Status};
pub use settings::{DEFAULT_OLLAMA_URL, Feature, LocalSettings};
pub use types::{LocalModel, LocalModelRef, LocalProviderKind, PullProgress, SearchHit};

#[cfg(test)]
mod catalog_test {
    //! Jeder deutsche Quelltext aus `trd!` in diesem Crate steht im Katalog (xcstrings oder Zusatztexte).
    use std::path::Path;

    fn literals(source: &str) -> Vec<String> {
        let code: String = source.lines().filter(|l| !l.trim_start().starts_with("//")).collect::<Vec<_>>().join("\n");
        let mut found = Vec::new();
        let mut from = 0;
        while let Some(at) = code[from..].find("trd!(\"") {
            let start = from + at + 6;
            if let Some(end) = code[start..].find('"') {
                found.push(code[start..start + end].to_string());
            }
            from = start;
        }
        found
    }

    #[test]
    fn every_literal_is_in_the_catalog() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut checked = 0;
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.file_name().is_some_and(|n| n == "lib.rs") {
                continue;
            }
            for text in literals(&std::fs::read_to_string(&path).unwrap()) {
                checked += 1;
                assert!(kadrell_i18n::id_for_de(&text).is_some(), "{}: {text} fehlt im Katalog", path.display());
            }
        }
        assert!(checked >= 8, "zu wenige Literale gefunden ({checked})");
    }
}
