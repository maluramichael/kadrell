//! Lokale Modelle (Ollama, llama.cpp) in der App: Anbindung von `kadrell-models` an Zustand und Oberfläche.
//!
//! | Teil | Inhalt |
//! | --- | --- |
//! | [`backend`] | Modelle, Downloads und Suche im Einstellungen-Dialog |
//! | [`agent_pick`] | „Starten mit“: Claude oder Pi vor jeder neuen Session (Zustand und Dialog) |

pub mod agent_pick;
pub mod backend;
pub mod pi;

use gpui::{App, Global};
use kadrell_models::LocalModels;

/// Die lokalen Modelle der Shell, für Dialoge, die sie nicht über die Shell erreichen (Einstellungen).
pub struct ModelsGlobal(pub LocalModels);

impl Global for ModelsGlobal {}

/// Macht die Modelle für die Dialoge verfügbar.
pub fn install(cx: &mut App, models: &LocalModels) {
    cx.set_global(ModelsGlobal(models.clone()));
}
