//! Sprache der Oberfläche zur Laufzeit: Anbindung von `kadrell-i18n` an die Shell (wie `applyAppearance` in der
//! Swift-App), Kürzel-Beschriftungen je Betriebssystem und Sprache und der Marker `de!` für deutsche Quelltexte.
//!
//! - `trd!("…")` übersetzt sofort (deutscher Quelltext, Schlüssel der xcstrings oder der Zusatztexte).
//! - `de!("…")` markiert einen deutschen Quelltext in einer Tabelle oder Konstante; übersetzt wird erst beim Benutzen
//!   (`trd!(var)`). Beides prüft der Test `every_literal_is_in_the_catalog` gegen den Katalog.
//! - Kürzel stehen in den Texten als Mac-Zeichen (`⌘N`); `localize_shortcuts` macht daraus unter Windows `Strg+N`
//!   bzw. `Ctrl+N`.

use gpui::Context;
use kadrell_core::Settings;
use kadrell_i18n::{Lang, Language, global, id_for_de};

use crate::Shell;

/// Markiert einen deutschen Quelltext, der später mit `trd!(…)` übersetzt wird. Zur Laufzeit der Text selbst.
macro_rules! de {
    ($text:literal) => {
        $text
    };
}
pub(crate) use de;

/// Ein Modifier eines Kürzels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Modifier {
    Ctrl,
    Alt,
    Shift,
    /// Windows-Taste (steht auf dem Mac für ⌃).
    Win,
}

/// Name des Modifiers unter Windows: deutsch `Strg`, `Umschalt`, englisch `Ctrl`, `Shift`.
pub fn modifier_name(m: Modifier, lang: Lang) -> &'static str {
    match (m, lang) {
        (Modifier::Ctrl, Lang::De) => "Strg",
        (Modifier::Ctrl, Lang::En) => "Ctrl",
        (Modifier::Alt, _) => "Alt",
        (Modifier::Shift, Lang::De) => "Umschalt",
        (Modifier::Shift, Lang::En) => "Shift",
        (Modifier::Win, _) => "Win",
    }
}

fn modifier_of(glyph: char) -> Option<Modifier> {
    match glyph {
        '⌘' | '⌃' => Some(Modifier::Ctrl),
        '⌥' => Some(Modifier::Alt),
        '⇧' => Some(Modifier::Shift),
        _ => None,
    }
}

/// Folgt auf die Modifier eine Taste, die mit ihnen ein Kürzel bildet?
fn is_key_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, ',' | '+' | '-' | '↩' | '↑' | '↓' | '←' | '→')
}

/// Mac-Zeichen eines Kürzels (`⇧⌘N`, `⌥F4`) als Windows-Beschriftung (`Strg+Umschalt+N`). Reihenfolge Strg, Alt,
/// Umschalt, jeder Modifier einmal.
fn windows_run(mods: &[Modifier], key: Option<char>, lang: Lang) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for m in [Modifier::Ctrl, Modifier::Alt, Modifier::Shift] {
        if mods.contains(&m) {
            parts.push(modifier_name(m, lang));
        }
    }
    let mut out = parts.join("+");
    match key {
        Some('↩') => out.push_str("+Enter"),
        Some(k) => {
            out.push('+');
            out.push(k);
        }
        None => {}
    }
    out
}

/// Ersetzt Mac-Kürzelzeichen im Text durch die Beschriftung des Betriebssystems. Auf dem Mac bleibt der Text, wie er
/// ist. Ein Modifier ohne Taste (`⌘ Mausrad`) wird zum bloßen Namen (`Strg Mausrad`).
pub fn localize_shortcuts_for(text: &str, mac: bool, lang: Lang) -> String {
    if mac || !text.chars().any(|c| modifier_of(c).is_some()) {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let Some(first) = modifier_of(chars[i]) else {
            out.push(chars[i]);
            i += 1;
            continue;
        };
        let mut mods = vec![first];
        i += 1;
        while let Some(m) = chars.get(i).copied().and_then(modifier_of) {
            mods.push(m);
            i += 1;
        }
        let key = chars.get(i).copied().filter(|c| is_key_char(*c));
        i += usize::from(key.is_some());
        out.push_str(&windows_run(&mods, key, lang));
    }
    out
}

/// Wie `localize_shortcuts_for` für dieses Betriebssystem und die eingestellte Sprache.
pub fn localize_shortcuts(text: &str) -> String {
    localize_shortcuts_for(text, cfg!(target_os = "macos"), global().lang())
}

/// Kürzel-Beschriftung für Menüs: Mac-Zeichen, unter Windows als `Strg+N`.
pub fn shortcut(glyphs: &str) -> String {
    localize_shortcuts(glyphs)
}

/// Kürzel nur auf dem Mac (unter Windows ohne Belegung).
pub fn shortcut_mac_only(glyphs: &str) -> String {
    if cfg!(target_os = "macos") { glyphs.to_string() } else { String::new() }
}

/// Kürzel, das auf dem Mac und unter Windows verschieden ist (Mac-Zeichen, Windows-Zeichen im selben Format).
pub fn shortcut_os(mac: &str, win: &str) -> String {
    if cfg!(target_os = "macos") { mac.to_string() } else { localize_shortcuts(win) }
}

/// Name des Dateimanagers im Text („Finder“ oder „Explorer“).
pub const fn file_manager() -> &'static str {
    if cfg!(target_os = "macos") { "Finder" } else { "Explorer" }
}

/// Übersetzt einen Text, wenn er ein bekannter deutscher Quelltext ist (Einheiten wie `min`, `%`, `px` bleiben).
pub fn tr_known(source: &str) -> String {
    if id_for_de(source).is_some() { global().t_de(source) } else { source.to_string() }
}

/// Text für die Anzeige: deutscher Quelltext in der eingestellten Sprache (unbekannte Texte bleiben), Kürzel in der
/// Schreibweise des Betriebssystems.
pub fn tx(source: &str) -> String {
    localize_shortcuts(&tr_known(source))
}

/// Die Einstellung `language` des Kerns als Sprachwahl von `kadrell-i18n`.
pub fn language_of(settings: &Settings) -> Language {
    Language::parse(settings.language().raw()).unwrap_or_default()
}

/// Sprache aus den Einstellungen übernehmen. `KADRELL_DEV_LANG=de|en|system` überschreibt sie beim Start (für
/// Screenshots ohne Eingabe). Gibt zurück, ob sich die Sprache geändert hat.
pub fn apply(settings: &Settings) -> bool {
    let wanted = language_of(settings);
    if global().language() == wanted {
        return false;
    }
    global().set_language(wanted);
    true
}

/// Beim Start: Einstellung, ersatzweise `KADRELL_DEV_LANG`.
pub fn apply_at_start(settings: &Settings) {
    let dev = std::env::var("KADRELL_DEV_LANG").ok().and_then(|v| Language::parse(&v));
    global().set_language(dev.unwrap_or_else(|| language_of(settings)));
}

impl Shell {
    /// Einmal je Frame: hat sich die Sprache geändert, alles neu bauen, was Texte beim Erzeugen zwischenspeichert
    /// (Menüleiste, System-Menü auf dem Mac, Ansicht der Sessions). Dialoge bauen ihre Texte bei jedem Zeichnen.
    pub(crate) fn sync_language(&mut self, cx: &mut Context<Self>) {
        let generation = global().generation();
        if generation == self.language_generation {
            return;
        }
        self.language_generation = generation;
        self.menubar.update(cx, |bar, cx| bar.reload(cx));
        crate::menus::set_native_menus(cx);
        self.rebuild();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn win(text: &str, lang: Lang) -> String {
        localize_shortcuts_for(text, false, lang)
    }

    #[test]
    fn mac_keeps_glyphs() {
        assert_eq!(localize_shortcuts_for("Projektordner für ⌘N", true, Lang::De), "Projektordner für ⌘N");
    }

    #[test]
    fn windows_german_and_english_names() {
        assert_eq!(win("Projektordner für ⌘N", Lang::De), "Projektordner für Strg+N");
        assert_eq!(win("Project folder for ⌘N", Lang::En), "Project folder for Ctrl+N");
        assert_eq!(win("⇧⌘N", Lang::De), "Strg+Umschalt+N");
        assert_eq!(win("⇧⌘N", Lang::En), "Ctrl+Shift+N");
        assert_eq!(win("⌥⌘N", Lang::En), "Ctrl+Alt+N");
        assert_eq!(win("⌥F4", Lang::De), "Alt+F4");
    }

    #[test]
    fn punctuation_and_enter_keys() {
        assert_eq!(win("⌘,", Lang::De), "Strg+,");
        assert_eq!(win("⌘+", Lang::De), "Strg++");
        assert_eq!(win("⌘-", Lang::En), "Ctrl+-");
        assert_eq!(win("⌘↩", Lang::De), "Strg+Enter");
        assert_eq!(win("⌘⇧↩", Lang::En), "Ctrl+Shift+Enter");
    }

    #[test]
    fn modifier_without_key_becomes_a_name() {
        assert_eq!(win("Terminal-Schriftgröße  ⌘+ ⌘- ⌘0  ⌘ Mausrad", Lang::De), "Terminal-Schriftgröße  Strg++ Strg+- Strg+0  Strg Mausrad");
        assert_eq!(win("zweimal ⌃C, /exit", Lang::En), "zweimal Ctrl+C, /exit");
    }

    #[test]
    fn text_without_glyphs_is_unchanged() {
        assert_eq!(win("⏎ starten · Esc schließen", Lang::De), "⏎ starten · Esc schließen");
        assert_eq!(win("↑↓ wählen", Lang::De), "↑↓ wählen");
    }

    #[test]
    fn language_follows_the_setting() {
        let mut s = Settings::default();
        assert_eq!(language_of(&s), Language::System);
        s.set_language(kadrell_core::settings::Language::En);
        assert_eq!(language_of(&s), Language::En);
    }
}
