//! Die Einstellungen, die der Dialog bearbeitet, als GPUI-Global. Sie sind eine Kopie der echten `Settings` aus dem
//! `AppState` der Shell: beim Öffnen des Dialogs gesetzt (`seed`), nach jeder Änderung meldet der Dialog
//! `SettingsEvent::Changed`, und die Shell schreibt die Kopie über den `AppState` ins Profil (`AppState::apply_settings`).
//! Im Wegwerfprofil (`--profile tmp`) ist das dessen Ordner, nie das Standardprofil.

use gpui::{App, Global};
use kadrell_core::Settings;

pub struct AppSettings {
    pub settings: Settings,
}

impl Global for AppSettings {}

/// Setzt die Kopie aus den echten Einstellungen; bei jedem Öffnen des Dialogs neu.
pub fn seed(cx: &mut App, settings: Settings) {
    cx.set_global(AppSettings { settings });
}

/// Kopie der aktuellen Einstellungen.
pub fn snapshot(cx: &App) -> Settings {
    cx.global::<AppSettings>().settings.clone()
}

/// Ändert die Kopie; die Shell übernimmt sie nach dem Event.
pub fn update(cx: &mut App, change: impl FnOnce(&mut Settings)) {
    change(&mut cx.global_mut::<AppSettings>().settings);
}
