//! Oberfläche von Kadrell (GPUI): kleine Komponenten, die `lib.rs` zusammensetzt.

use gpui::App;

use crate::ThemeGlobal;
use crate::theme::Theme;

pub mod bar;
pub mod bar_keys;
pub mod chrome;
pub mod dialog_keys;
pub mod context_menu;
pub mod count_up;
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
pub mod instrument;
pub mod overlay;
pub mod pick;
pub mod overlay_anim;
pub mod overlay_queue;
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
pub mod tile_body;
pub mod tile_icons;
pub mod tile_actions;
pub mod tile_edge;
pub mod tile_look;
pub mod stack_row;
pub mod module_corner;
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
