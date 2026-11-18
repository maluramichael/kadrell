//! Sprache des Systems: Windows `GetUserDefaultLocaleName`, macOS `defaults read -g AppleLanguages`
//! (Reihenfolge der bevorzugten Sprachen), überall zuletzt die Umgebungsvariablen.

use crate::Lang;

/// Erste bevorzugte Sprache, die wir übersetzt haben (de oder en). Sonst Deutsch: das ist die
/// Entwicklungssprache, so wie das Bundle der Mac-App auf die Quellsprache zurückfällt.
pub fn system_language() -> Lang {
    pick(&preferred_tags())
}

/// Wählt aus Sprach-Tags (`de-DE`, `en_US.UTF-8`, `C`) die erste unterstützte Sprache.
pub fn pick(tags: &[String]) -> Lang {
    tags.iter().find_map(|t| Lang::from_tag(t)).unwrap_or(Lang::De)
}

fn preferred_tags() -> Vec<String> {
    let mut tags = platform_tags();
    tags.extend(env_tags());
    tags
}

/// `LC_ALL`, `LC_MESSAGES`, `LANGUAGE` (Doppelpunkt-Liste), `LANG`.
fn env_tags() -> Vec<String> {
    let mut tags = Vec::new();
    for name in ["LC_ALL", "LC_MESSAGES", "LANGUAGE", "LANG"] {
        if let Ok(v) = std::env::var(name) {
            tags.extend(v.split(':').filter(|s| !s.is_empty()).map(str::to_string));
        }
    }
    tags
}

#[cfg(windows)]
fn platform_tags() -> Vec<String> {
    use windows_sys::Win32::Globalization::GetUserDefaultLocaleName;
    const LOCALE_NAME_MAX_LENGTH: usize = 85;
    let mut buf = [0u16; LOCALE_NAME_MAX_LENGTH];
    // SAFETY: Puffer und Länge stimmen überein, die Funktion schreibt höchstens `len` Zeichen.
    let n = unsafe { GetUserDefaultLocaleName(buf.as_mut_ptr(), buf.len() as i32) };
    if n <= 1 {
        return Vec::new();
    }
    vec![String::from_utf16_lossy(&buf[..(n as usize - 1)])]
}

#[cfg(target_os = "macos")]
fn platform_tags() -> Vec<String> {
    let out = std::process::Command::new("defaults").args(["read", "-g", "AppleLanguages"]).output();
    match out {
        Ok(o) if o.status.success() => parse_apple_languages(&String::from_utf8_lossy(&o.stdout)),
        _ => Vec::new(),
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
fn platform_tags() -> Vec<String> {
    Vec::new()
}

/// Ausgabe von `defaults read -g AppleLanguages`: `(\n    "de-DE",\n    "en-DE"\n)`.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn parse_apple_languages(text: &str) -> Vec<String> {
    text.lines()
        .map(|l| l.trim().trim_matches(|c: char| c == ',' || c == '"' || c.is_whitespace()))
        .filter(|l| !l.is_empty() && *l != "(" && *l != ")")
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn picks_first_supported_language() {
        assert_eq!(pick(&v(&["fr-FR", "en-US", "de-DE"])), Lang::En);
        assert_eq!(pick(&v(&["de_DE.UTF-8"])), Lang::De);
        assert_eq!(pick(&v(&["DE"])), Lang::De);
    }

    #[test]
    fn unsupported_falls_back_to_german() {
        assert_eq!(pick(&v(&["fr-FR", "C"])), Lang::De);
        assert_eq!(pick(&[]), Lang::De);
    }

    #[test]
    fn parses_defaults_output() {
        let out = "(\n    \"en-DE\",\n    \"de-DE\"\n)\n";
        assert_eq!(parse_apple_languages(out), v(&["en-DE", "de-DE"]));
        assert_eq!(pick(&parse_apple_languages(out)), Lang::En);
    }

    #[test]
    fn system_language_never_panics() {
        let _ = system_language();
    }
}
