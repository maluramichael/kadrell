//! Oberfläche des Spikes: kleine Komponenten, die main.rs zusammensetzt.

use gpui::App;

use crate::ThemeGlobal;
use crate::theme::Theme;

pub mod bar;
pub mod chrome;
pub mod footer;
pub mod model;
pub mod pill;
pub mod sidebar;
pub mod tile;

/// Aktuelles Farbschema aus dem Global.
pub fn theme(cx: &App) -> Theme {
    cx.global::<ThemeGlobal>().0
}
