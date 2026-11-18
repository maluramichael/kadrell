//! Oberfläche des Spikes: kleine Komponenten, die main.rs zusammensetzt.

use gpui::App;

use crate::ThemeGlobal;
use crate::theme::Theme;

pub mod bar;
pub mod chrome;
pub mod footer;
pub mod icons;
pub mod model;
pub mod folders;
pub mod new_session;
pub mod overlay;
pub mod text_input;
pub mod pill;
pub mod sidebar;
pub mod tile;

/// Aktuelles Farbschema aus dem Global.
pub fn theme(cx: &App) -> Theme {
    cx.global::<ThemeGlobal>().0
}
