//! Kadrell-Kern: Modell und Speicher ohne Oberfläche. Gruppen, Sessions, Einstellungen, Profile, Kürzel, Zähler und
//! die Auflösung von Zielen (`-t`), portiert aus der Swift-App (`app/Kadrell`), die Quelle der Wahrheit bleibt.
//!
//! Die Dateien im Profilordner (`sessions.json`, `groups.json`, `closed.json`) haben dasselbe Format wie die der
//! Mac-App, beide Seiten lesen sich gegenseitig. Kein globaler Zustand: jeder Speicher bekommt seinen Pfad, Zeiten
//! und Prozessprüfungen reicht der Aufrufer herein.
//!
//! | Modul | Inhalt |
//! | --- | --- |
//! | [`paths`] | Profilordner je Plattform, Pfadhelfer |
//! | [`json_file`] | tolerantes Laden, atomares Schreiben, kaputte Dateien sichern |
//! | [`session`] | `Session`, Status, Titel, `Agent` (aus `claude agents --json`), `AgentKind` |
//! | [`group`] | `Group`, `GroupStore`, Paletten, Sortierung des Baums |
//! | [`store`] | `SessionStore` (mit `closed.json`) und `Store` (Sessions und Gruppen zusammen) |
//! | [`targets`] | Ziele `-t` auflösen, Freigabeprüfung |
//! | [`settings`] | `Settings` (`settings.json`) mit allen Schlüsseln, Standardwerten und Bereichen |
//! | [`profile`] | Profile (`tmp`, benannt), Lock, Aufräumen, Beenden-Marker |
//! | [`hotkeys`] | Kürzel: Aktionen, Standardbelegung, Speicherung |
//! | [`stats`] | Lebenszeit-Zähler |

#[macro_use]
mod macros;

pub mod group;
pub mod hotkeys;
pub mod json_file;
pub mod paths;
pub mod profile;
pub mod session;
pub mod settings;
pub mod stats;
pub mod store;
pub mod targets;

pub use group::{Group, GroupStore, SidebarFlat, SidebarSort};
pub use paths::Paths;
pub use profile::Profile;
pub use session::{Agent, AgentKind, ClosedSession, Session, SessionStatus};
pub use settings::Settings;
pub use store::{SessionStore, Store};
pub use targets::TargetError;
