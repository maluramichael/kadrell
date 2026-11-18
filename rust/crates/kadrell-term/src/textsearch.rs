//! Textsuche ohne Groß-/Kleinschreibung über Terminalzeilen (für die Suche in allen Terminals). Reine Funktionen,
//! damit sie ohne Terminal testbar bleiben.

use std::ops::Range;

fn fold(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

/// Alle nicht überlappenden Treffer von `query` in `text` als Bereiche in Zeichen (nicht Bytes).
pub fn find_char_ranges(text: &str, query: &str) -> Vec<Range<usize>> {
    let hay: Vec<char> = text.chars().map(fold).collect();
    let needle: Vec<char> = query.chars().map(fold).collect();
    let mut out = Vec::new();
    if needle.is_empty() || needle.len() > hay.len() {
        return out;
    }
    let mut i = 0;
    while i + needle.len() <= hay.len() {
        if hay[i..i + needle.len()] == needle[..] {
            out.push(i..i + needle.len());
            i += needle.len();
        } else {
            i += 1;
        }
    }
    out
}

/// Wie `find_char_ranges`, aber in Byte-Offsets von `text` (für gestylte Textabschnitte).
pub fn find_byte_ranges(text: &str, query: &str) -> Vec<Range<usize>> {
    let offsets: Vec<usize> = text.char_indices().map(|(b, _)| b).chain(std::iter::once(text.len())).collect();
    find_char_ranges(text, query).into_iter().map(|r| offsets[r.start]..offsets[r.end]).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_case_insensitive_non_overlapping() {
        assert_eq!(find_char_ranges("Claude claude CLAUDE", "claude"), vec![0..6, 7..13, 14..20]);
        assert_eq!(find_char_ranges("aaaa", "aa"), vec![0..2, 2..4]);
        assert!(find_char_ranges("abc", "").is_empty());
        assert!(find_char_ranges("ab", "abc").is_empty());
    }

    #[test]
    fn byte_ranges_follow_multibyte_text() {
        assert_eq!(find_byte_ranges("äö Claude", "claude"), vec![5..11]);
    }
}
