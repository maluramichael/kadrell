//! Mac-Begriffe in Texten, die unter Windows anders heißen. Die Quelltexte stammen aus den xcstrings der Mac-App; unter
//! Windows ersetzt `adapt` sie beim Übersetzen (Finder wird Explorer, Dock wird Taskleiste, Menüleiste wird Infobereich).
//! Kürzelzeichen (⌘⌥⌃⇧) wandelt die App (`kadrell-app/src/lang.rs`), nur ⌫ steht hier. Auf dem Mac bleibt alles
//! unverändert.

use crate::Lang;

/// Ersetzungen je Sprache, die längere Wendung vor der kürzeren. Reihenfolge ist Teil der Regel.
const DE: &[(&str, &str)] = &[
    ("Menüleisten-Icon / Dock", "Infobereich-Icon / Taskleiste"),
    ("Im Dock ablegen", "Minimieren"),
    ("Menüleiste", "Infobereich"),
    ("Finder", "Explorer"),
    ("⌫", "Rücktaste"),
];

const EN: &[(&str, &str)] = &[
    ("Menu bar icon / Dock", "Notification area icon / Taskbar"),
    ("Menu bar", "Notification area"),
    ("Finder", "Explorer"),
    ("⌫", "Backspace"),
];

/// Text mit den Windows-Begriffen statt der Mac-Begriffe.
pub fn adapt(text: &str, lang: Lang) -> String {
    let table = match lang {
        Lang::De => DE,
        Lang::En => EN,
    };
    table.iter().fold(text.to_string(), |acc, (mac, win)| if acc.contains(mac) { acc.replace(mac, win) } else { acc })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_mac_words() {
        assert_eq!(adapt("Im Finder zeigen", Lang::De), "Im Explorer zeigen");
        assert_eq!(adapt("Show in Finder", Lang::En), "Show in Explorer");
        assert_eq!(adapt("Menüleisten-Icon / Dock", Lang::De), "Infobereich-Icon / Taskleiste");
        assert_eq!(adapt("Menu bar icon / Dock", Lang::En), "Notification area icon / Taskbar");
        assert_eq!(adapt("Menüleiste", Lang::De), "Infobereich");
        assert_eq!(adapt("Im Dock ablegen", Lang::De), "Minimieren");
        assert_eq!(adapt("⌫ alle", Lang::De), "Rücktaste alle");
        assert_eq!(adapt("Papierkorb", Lang::De), "Papierkorb");
    }

    #[test]
    fn catalog_has_no_mac_words_left() {
        for lang in Lang::ALL {
            for line in crate::ftl_source(lang).lines().filter(|l| !l.starts_with('#')) {
                let text = adapt(line, lang);
                assert!(!text.contains("Finder") && !text.contains('⌫'), "{text}");
            }
        }
    }
}
