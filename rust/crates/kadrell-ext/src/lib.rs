//! Lua-Extensions für Kadrell: Port von `app/Kadrell/Extensions` ohne die Oberfläche.
//!
//! Zwei Seiten: der Helper (`run_host`, Unterbefehl `kadrell ext-host <ordner>`) führt eine Extension in Lua aus, die
//! App-Seite (`ExtensionManager`, `ExtensionProcess`, `ExtensionSupervisor`) startet, überwacht und füttert die Helper.
//! Alles, was die App beisteuert (Steuerbefehle, Schlüsselbund, Einstellungen, HTTP), steckt hinter Traits.

pub mod footer;
pub mod host;
pub mod http;
pub mod i18n;
pub mod json;
pub mod manifest;
pub mod message;
pub mod panel;
pub mod sys;
pub mod system;
#[doc(hidden)]
pub mod testutil;

pub use host::run_host;
