//! Oberfläche von Kadrell (GPUI): kleine Komponenten, die `lib.rs` zusammensetzt.

use gpui::App;

use crate::ThemeGlobal;
use crate::theme::Theme;

pub mod bar;
pub mod chrome;
pub mod context_menu;
pub mod ext;
pub mod footer;
pub mod icons;
pub mod menubar;
pub mod layout;
pub mod model;
pub mod folder_row;
pub mod folders;
pub mod groups;
pub mod new_session;
pub mod overlay;
pub mod palette;
pub mod quick_start;
pub mod notice;
pub mod stats;
pub mod text_input;
pub mod tooltip;
pub mod usage_bar;
pub mod pill;
pub mod settings;
pub mod rename;
pub mod sidebar;
pub mod tile;
pub mod tiling;
pub mod tree_actions;
pub mod tree_anim;
pub mod tree_icons;
pub mod tree_rules;
#[cfg(test)]
mod tree_tests;
pub mod tree_toolbar;

/// Aktuelles Farbschema aus dem Global.
pub fn theme(cx: &App) -> Theme {
    cx.global::<ThemeGlobal>().0
}
