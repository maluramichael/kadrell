//! Terminal-Kachel für Kadrell: alacritty_terminal als Emulator, GPUI-Renderer, Box-Drawing, mitgelieferte Schrift.
//! Hängt nicht von der App ab; Farben kommen über `TermThemeGlobal`.

mod boxdraw;
pub mod fonts;
mod terminal;
mod theme;

pub use terminal::{FONT_SIZE, TerminalPane, strip_inherited_env};
pub use theme::{FONT, TermTheme, TermThemeGlobal};
