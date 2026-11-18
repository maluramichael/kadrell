//! Nutzertexte der Extension-Schicht, deutsch und englisch (Quelle: `Localizable.xcstrings`). Die Sprache gilt
//! prozessweit und lässt sich zur Laufzeit umstellen; die App ruft `set_locale` bei einem Sprachwechsel.

use std::sync::atomic::{AtomicBool, Ordering};

static ENGLISH: AtomicBool = AtomicBool::new(false);

/// `"de"` oder `"en"`; alles andere gilt als Deutsch.
pub fn set_locale(locale: &str) {
    ENGLISH.store(locale.starts_with("en"), Ordering::Relaxed);
}

/// Die eingestellte Sprache als `de` oder `en`.
pub fn locale() -> &'static str {
    if ENGLISH.load(Ordering::Relaxed) {
        "en"
    } else {
        "de"
    }
}

fn pick(english: bool, de: &str, en: &str, args: &[&str]) -> String {
    let mut text = if english { en } else { de }.to_string();
    for (i, arg) in args.iter().enumerate() {
        text = text.replace(&format!("{{{i}}}"), arg);
    }
    text
}

/// Wählt den Text der eingestellten Sprache.
pub fn tr(de: &str, en: &str) -> String {
    pick(ENGLISH.load(Ordering::Relaxed), de, en, &[])
}

/// Wie `tr` für Texte mit Platzhaltern: `{0}`, `{1}` werden durch die Argumente ersetzt.
pub fn tr_args(de: &str, en: &str, args: &[&str]) -> String {
    pick(ENGLISH.load(Ordering::Relaxed), de, en, args)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_by_language_and_fills_placeholders() {
        assert_eq!(pick(true, "Aus", "Off", &[]), "Off");
        assert_eq!(pick(false, "Aus", "Off", &[]), "Aus");
        assert_eq!(
            pick(true, "{0} von {1}", "{0} of {1}", &["a", "b"]),
            "a of b"
        );
    }
}
