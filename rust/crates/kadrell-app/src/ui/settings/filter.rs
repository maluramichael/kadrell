//! Filter der Einstellungen, bedient wie `/` in tmux, vim und less (`ListFilter.swift`): alle Wörter der Eingabe
//! müssen vorkommen, Groß- und Kleinschreibung und Umlaute zählen nicht.

/// Kleinbuchstaben ohne Akzente und Umlaute (`caseInsensitive` und `diacriticInsensitive` der Mac-App).
pub fn fold(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars().flat_map(char::to_lowercase) {
        match c {
            'ä' | 'à' | 'á' | 'â' | 'ã' | 'å' => out.push('a'),
            'ö' | 'ò' | 'ó' | 'ô' | 'õ' => out.push('o'),
            'ü' | 'ù' | 'ú' | 'û' => out.push('u'),
            'é' | 'è' | 'ê' | 'ë' => out.push('e'),
            'í' | 'ì' | 'î' | 'ï' => out.push('i'),
            'ç' => out.push('c'),
            'ñ' => out.push('n'),
            'ß' => out.push_str("ss"),
            other => out.push(other),
        }
    }
    out
}

/// Kommen alle Wörter von `query` in `text` vor?
pub fn matches(text: &str, query: &str) -> bool {
    let hay = fold(text);
    fold(query).split_whitespace().all(|w| hay.contains(w))
}

/// Zustand des Filters: offen oder nicht, und der getippte Text.
#[derive(Debug, Default, Clone)]
pub struct Filter {
    pub open: bool,
    pub query: String,
}

impl Filter {
    pub fn needle(&self) -> &str {
        self.query.trim()
    }

    /// Filtert wirklich: Feld offen und etwas eingegeben.
    pub fn is_active(&self) -> bool {
        self.open && !self.needle().is_empty()
    }

    /// Bleibt der Eintrag sichtbar? Ohne aktiven Filter immer.
    pub fn keeps(&self, texts: &[&str]) -> bool {
        !self.is_active() || matches(&texts.join(" "), self.needle())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn active(q: &str) -> Filter {
        Filter { open: true, query: q.to_string() }
    }

    #[test]
    fn all_words_must_match() {
        assert!(matches("Terminal-Schriftgröße", "schrift größe"));
        assert!(!matches("Terminal-Schrift", "schrift farbe"));
        assert!(matches("irgendwas", ""));
    }

    #[test]
    fn ignores_case_and_umlauts() {
        assert!(matches("Größe der Kacheln", "GROSSE"));
        assert!(matches("Größe der Kacheln", "grosse"));
        assert!(matches("Menüleiste", "menuleiste"));
        assert!(matches("Menuleiste", "MENÜ"));
        assert!(matches("Fußzeile", "fusszeile"));
    }

    #[test]
    fn closed_or_empty_filter_keeps_everything() {
        assert!(Filter::default().keeps(&["egal"]));
        assert!(!Filter { open: false, query: "x".into() }.is_active());
        assert!(!active("   ").is_active());
        assert!(active("   ").keeps(&["egal"]));
    }

    #[test]
    fn active_filter_joins_the_texts() {
        let f = active("claude modell");
        assert!(f.keeps(&["Claude", "Modell"]));
        assert!(!f.keeps(&["Claude"]));
        assert!(f.is_active());
    }
}
