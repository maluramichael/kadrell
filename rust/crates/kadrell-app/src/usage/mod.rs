//! Konten, Nutzung und Statistik in der App: Anbindung von `kadrell-accounts` an Zustand und Oberfläche.
//!
//! | Teil | Inhalt |
//! | --- | --- |
//! | [`logic`] | reine Logik der Leiste: Farbe, Alter, Status-Display, Platzplan (`BarPlan`) |
//! | [`hub`] | `AccountService`, `UsageService`, Verlauf und Tempo-Warnung, läuft auf dem Hintergrund-Executor |
//! | [`fake`] | erfundene Daten für Screenshots (`KADRELL_DEV_USAGE=fake`) |
//! | [`backend`] | Konten im Einstellungen-Dialog |
//! | `shell` | Schleife, Meldungen, Konto-Menü, Statistik-Fenster an der `Shell` |
//!
//! Die Lebenszeit-Zähler (`stats.*`) gehören `kadrell_core::stats` (siehe `state::stats`), nicht `kadrell_accounts::Stats`.

pub mod backend;
pub mod fake;
pub mod hub;
pub mod logic;
mod shell;

pub use shell::{AccountCmd, UsageUi};

/// Sprache der Konten-Texte aus der der Oberfläche.
pub fn lang() -> kadrell_accounts::Lang {
    match kadrell_i18n::global().lang() {
        kadrell_i18n::Lang::De => kadrell_accounts::Lang::De,
        kadrell_i18n::Lang::En => kadrell_accounts::Lang::En,
    }
}
