//! Kleine Textwerkzeuge, die mehrere Module brauchen.

use serde_json::{Map, Value};
use unicode_normalization::UnicodeNormalization;

/// Wie `folding(options: [.caseInsensitive, .diacriticInsensitive])`: ohne Akzente, kleingeschrieben, `ß` als `ss`.
pub fn fold(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.nfd().filter(|c| !is_combining(*c)) {
        match c {
            'ß' => out.push_str("ss"),
            c => out.extend(c.to_lowercase()),
        }
    }
    out
}

/// Zeichen für Zeichen gefaltet (jedes Eingabezeichen wird zu genau einem Ausgabezeichen), für Fundstellen, die in
/// den Originaltext zurückgerechnet werden müssen. Ersetzt `ß` durch `s`, damit die Positionen stimmen.
pub fn fold_chars(s: &str) -> Vec<char> {
    s.chars()
        .map(|c| {
            let base = c.to_string().nfd().find(|d| !is_combining(*d)).unwrap_or(c);
            base.to_lowercase().next().unwrap_or(base)
        })
        .collect()
}

fn is_combining(c: char) -> bool {
    matches!(c as u32, 0x0300..=0x036F | 0x1AB0..=0x1AFF | 0x1DC0..=0x1DFF | 0x20D0..=0x20FF | 0xFE20..=0xFE2F)
}

/// Kleinbuchstaben, ohne Akzente, nur Buchstaben und Ziffern: „Robo-Thomas“ gibt „robothomas“.
pub fn normalize_name(s: &str) -> String {
    fold(s).to_lowercase().chars().filter(|c| c.is_alphanumeric()).collect()
}

/// Erstes `{ … }` im Text als JSON-Objekt (Modelle packen es gern in Code-Zäune).
pub fn first_json_object(text: &str) -> Option<Map<String, Value>> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    if start >= end {
        return None;
    }
    match serde_json::from_str::<Value>(&text[start..=end]).ok()? {
        Value::Object(m) => Some(m),
        _ => None,
    }
}

/// Teilfolge `needle` in `hay` (Bytes), wie `memmem`.
pub fn contains_bytes(hay: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() {
        return true;
    }
    let first = needle[0];
    let mut i = 0;
    while i + needle.len() <= hay.len() {
        match hay[i..hay.len() - needle.len() + 1].iter().position(|b| *b == first) {
            None => return false,
            Some(p) => {
                i += p;
                if &hay[i..i + needle.len()] == needle {
                    return true;
                }
                i += 1;
            }
        }
    }
    false
}

/// Beliebig viele Leerzeichen und Zeilenumbrüche zu einem Leerzeichen.
pub fn squash_whitespace(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Unscharf wie ⌘P: die Buchstaben von `q` in Reihenfolge irgendwo in `s`, beides kleingeschrieben.
pub fn fuzzy(q: &str, s: &str) -> bool {
    let q: Vec<char> = q.to_lowercase().chars().collect();
    let mut i = 0;
    for c in s.to_lowercase().chars() {
        if i < q.len() && c == q[i] {
            i += 1;
        }
    }
    i == q.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fold_and_names() {
        assert_eq!(fold("Für Ärger"), "fur arger");
        assert_eq!(fold("Straße"), "strasse");
        assert_eq!(normalize_name("Robo-Thomas"), "robothomas");
        assert_eq!(fold_chars("Éa").iter().collect::<String>(), "ea");
    }

    #[test]
    fn json_and_bytes() {
        assert!(first_json_object("```json\n{\"a\": 1}\n```").is_some());
        assert!(first_json_object("nix").is_none());
        assert!(contains_bytes(b"hello world", b"o w"));
        assert!(!contains_bytes(b"hello", b"xyz"));
        assert!(fuzzy("kdr", "Kadrell"));
    }
}
