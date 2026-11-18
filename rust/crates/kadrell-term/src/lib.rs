//! Terminal-Kachel für Kadrell: alacritty_terminal als Emulator, GPUI-Renderer, Box-Drawing, mitgelieferte Schrift.
//! Hängt nicht von der App ab; Farben kommen über `TermThemeGlobal`.

mod boxdraw;
pub mod fonts;
pub mod links;
mod terminal;
mod textsearch;
mod theme;

pub use terminal::{LINE_HEIGHT, PtyParts, TermHandle, TermSpec, TerminalEvent, TerminalPane, start_pty, strip_inherited_env};
pub use textsearch::{find_byte_ranges, find_char_ranges};
pub use theme::{FONT, TermStyle, TermStyleGlobal, TermTheme, TermThemeGlobal};
