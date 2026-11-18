//! Terminal-Schrift: alle installierten Monospace-Familien plus die mitgelieferte (`Settings.monospaceFonts` der
//! Mac-App). GPUI kennt Familiennamen, die Einstellung `terminal.fontName` trägt auf dem Mac PostScript-Namen
//! („JetBrainsMonoNF-Regular“); beides wird auf eine Familie abgebildet.

use std::sync::{Mutex, PoisonError};

use gpui::{App, font};
use kadrell_core::settings::DEFAULT_FONT_NAME;

/// Die mitgelieferte Familie (steht immer vorn in der Liste).
pub const BUNDLED_FAMILY: &str = kadrell_term::FONT;

/// Eintrag der Auswahl: Wert der Einstellung und Anzeigename.
pub type FontChoice = (String, String);

static LIST: Mutex<Vec<FontChoice>> = Mutex::new(Vec::new());

/// Merkt die Liste für den Einstellungen-Dialog.
pub fn set_list(list: Vec<FontChoice>) {
    *LIST.lock().unwrap_or_else(PoisonError::into_inner) = list;
}

/// Die zuletzt ermittelte Liste (leer, bis `scan` gelaufen ist).
pub fn list() -> Vec<FontChoice> {
    LIST.lock().unwrap_or_else(PoisonError::into_inner).clone()
}

/// Familie zu einem gespeicherten Namen: bekannte Familie direkt, die mitgelieferte unter ihrem PostScript-Namen, sonst
/// die Familie, die vor einem `-Schnitt` steht, sonst die mitgelieferte.
pub fn resolve_family(stored: &str, installed: &[String]) -> String {
    let known = |n: &str| installed.iter().any(|f| f == n);
    if known(stored) {
        return stored.to_string();
    }
    if stored == DEFAULT_FONT_NAME || stored.starts_with("JetBrainsMonoNF") {
        return BUNDLED_FAMILY.to_string();
    }
    let base = stored.split('-').next().unwrap_or(stored);
    if known(base) { base.to_string() } else { BUNDLED_FAMILY.to_string() }
}

/// Dicktengleich: schmale und breite Zeichen (`i`, `W`, `m`) belegen dieselbe Breite.
pub fn monospace_advances(i: Option<f32>, w: Option<f32>, m: Option<f32>) -> bool {
    match (i, w, m) {
        (Some(i), Some(w), Some(m)) => i > 0. && (i - w).abs() < 0.01 && (i - m).abs() < 0.01,
        _ => false,
    }
}

/// Schriften des Systems einlesen und die dicktengleichen als Liste ablegen. Läuft einmal beim Start nach dem Laden der
/// mitgelieferten Schrift. Symbol- und Emoji-Familien fallen weg, wenn sie keine Breite für alle drei Zeichen liefern.
pub fn scan(cx: &mut App) -> Vec<String> {
    let names = cx.text_system().all_font_names();
    let ts = cx.text_system().clone();
    let probe = |family: &str, ch: char| -> Option<f32> {
        let id = ts.resolve_font(&font(family.to_string()));
        ts.advance(id, gpui::px(16.), ch).ok().map(|s| f32::from(s.width))
    };
    let mut mono: Vec<String> = names.iter().filter(|n| monospace_advances(probe(n, 'i'), probe(n, 'W'), probe(n, 'm'))).cloned().collect();
    mono.retain(|n| n != BUNDLED_FAMILY);
    let mut list: Vec<FontChoice> = vec![(DEFAULT_FONT_NAME.to_string(), BUNDLED_FAMILY.to_string())];
    list.extend(mono.iter().map(|n| (n.clone(), n.clone())));
    set_list(list);
    names
}

/// Auswahl für den Dialog: die gemessene Liste, eine gespeicherte Schrift, die nicht dabei ist, steht vorn.
pub fn choices(current: &str) -> Vec<FontChoice> {
    let mut list = list();
    if list.is_empty() {
        list.push((DEFAULT_FONT_NAME.to_string(), BUNDLED_FAMILY.to_string()));
    }
    if !list.iter().any(|(v, _)| v == current) {
        list.insert(0, (current.to_string(), current.to_string()));
    }
    list
}

#[cfg(test)]
mod tests {
    use super::*;

    fn installed() -> Vec<String> {
        ["Consolas", "Cascadia Mono", BUNDLED_FAMILY].map(String::from).to_vec()
    }

    #[test]
    fn known_families_pass_through() {
        assert_eq!(resolve_family("Consolas", &installed()), "Consolas");
    }

    #[test]
    fn mac_postscript_names_map_to_a_family() {
        assert_eq!(resolve_family("JetBrainsMonoNF-Regular", &installed()), BUNDLED_FAMILY);
        assert_eq!(resolve_family("Consolas-Bold", &installed()), "Consolas");
        assert_eq!(resolve_family("Menlo-Regular", &installed()), BUNDLED_FAMILY, "unbekannt: mitgelieferte");
    }

    #[test]
    fn monospace_needs_equal_advances() {
        assert!(monospace_advances(Some(9.6), Some(9.6), Some(9.6)));
        assert!(!monospace_advances(Some(3.), Some(12.), Some(12.)));
        assert!(!monospace_advances(None, Some(9.6), Some(9.6)));
        assert!(!monospace_advances(Some(0.), Some(0.), Some(0.)));
    }

    #[test]
    fn choices_put_an_unknown_stored_font_first() {
        set_list(vec![(DEFAULT_FONT_NAME.into(), BUNDLED_FAMILY.into()), ("Consolas".into(), "Consolas".into())]);
        let c = choices("Menlo-Regular");
        assert_eq!(c[0].0, "Menlo-Regular");
        assert_eq!(choices("Consolas")[0].0, DEFAULT_FONT_NAME);
        assert_eq!(choices(DEFAULT_FONT_NAME).len(), 2, "die Standardschrift steht schon in der Liste");
    }
}
