//! Mitgelieferte Schrift: JetBrainsMono Nerd Font Mono (SIL OFL 1.1, siehe `assets/fonts/OFL.txt`).
//! Auf Mac und Windows gleich, mit Nerd-Font-Symbolen (Prompt, Claude-Code-Zeichen).

use std::borrow::Cow;

const REGULAR: &[u8] = include_bytes!("../assets/fonts/JetBrainsMonoNerdFontMono-Regular.ttf");
const BOLD: &[u8] = include_bytes!("../assets/fonts/JetBrainsMonoNerdFontMono-Bold.ttf");
const ITALIC: &[u8] = include_bytes!("../assets/fonts/JetBrainsMonoNerdFontMono-Italic.ttf");
const BOLD_ITALIC: &[u8] = include_bytes!("../assets/fonts/JetBrainsMonoNerdFontMono-BoldItalic.ttf");

/// Registriert die Schriftschnitte im Textsystem. Vor dem Öffnen des ersten Fensters aufrufen.
pub fn load(cx: &mut gpui::App) {
    let fonts = [REGULAR, BOLD, ITALIC, BOLD_ITALIC].map(Cow::Borrowed).into();
    if let Err(e) = cx.text_system().add_fonts(fonts) {
        eprintln!("Schriften konnten nicht geladen werden: {e:#}");
    }
    if std::env::var_os("KADRELL_FONT_DEBUG").is_some() {
        let names = cx.text_system().all_font_names();
        eprintln!("{:?}", names.iter().filter(|n| n.contains("JetBrains")).collect::<Vec<_>>());
    }
}
