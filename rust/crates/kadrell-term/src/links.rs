//! Link unter dem Mauszeiger im Terminal: die Zeichenfolge ohne Leerraum um eine Spalte, ohne umgebende Klammern,
//! Anführungszeichen und Satzzeichen am Ende („https://x.y/z.“ endet im Satz, nicht im Link). Ob daraus ein Ziel wird
//! (URL oder vorhandene Datei), entscheidet die App (`kadrell_claude::link_target::resolve`).

/// Zeichen, die vorn nie zum Link gehören.
const LEADING: &[char] = &['(', '[', '{', '<', '"', '\'', '`'];
/// Zeichen, die hinten abfallen: Satzzeichen und schließende Klammern ohne öffnendes Gegenstück im Link.
const TRAILING: &[char] = &['.', ',', ';', ':', '!', '?', '"', '\'', '`', '>'];

fn opener(c: char) -> Option<char> {
    match c {
        ')' => Some('('),
        ']' => Some('['),
        '}' => Some('{'),
        _ => None,
    }
}

/// Fällt das letzte Zeichen ab? Satzzeichen immer, eine schließende Klammer nur, wenn der Link sie nicht selbst öffnet.
fn drops_last(link: &[char]) -> bool {
    let Some(&last) = link.last() else { return false };
    if TRAILING.contains(&last) {
        return true;
    }
    opener(last).is_some_and(|open| link.iter().filter(|c| **c == last).count() > link.iter().filter(|c| **c == open).count())
}

fn token_bounds(chars: &[char], at: usize) -> Option<(usize, usize)> {
    if chars.get(at).is_none_or(|c| c.is_whitespace()) {
        return None;
    }
    let start = chars[..at].iter().rposition(|c| c.is_whitespace()).map_or(0, |i| i + 1);
    let end = chars[at..].iter().position(|c| c.is_whitespace()).map_or(chars.len(), |i| at + i);
    Some((start, end))
}

/// Link an der Zeichenposition `at` (Index in `text`, nicht Spalte). `None` ohne Zeichen oder bei Leerraum.
pub fn link_at(text: &str, at: usize) -> Option<String> {
    let chars: Vec<char> = text.chars().collect();
    let (mut start, mut end) = token_bounds(&chars, at)?;
    while start < end && LEADING.contains(&chars[start]) {
        start += 1;
    }
    while end > start && drops_last(&chars[start..end]) {
        end -= 1;
    }
    (start < end).then(|| chars[start..end].iter().collect())
}

/// Zeichenindex zur Spalte: `cols[i]` ist die Spalte des i-ten Zeichens (breite Zeichen belegen zwei), ein Klick auf
/// die zweite Hälfte trifft das Zeichen davor. `None` hinter dem Text.
pub fn index_at_column(cols: &[usize], col: usize) -> Option<usize> {
    let i = cols.iter().rposition(|c| *c <= col)?;
    let width = if i + 1 < cols.len() { cols[i + 1] - cols[i] } else { 1 };
    (col < cols[i] + width.max(1)).then_some(i)
}

/// Schritt der Suchleiste per Kürzel: ⌘G bzw. Strg+G weiter (zum älteren Treffer wie ⏎), mit Umschalt zurück.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchStep {
    Older,
    Newer,
}

/// Modifier eines Tastendrucks, so wie `search_step` sie braucht (unabhängig von GPUI testbar).
#[derive(Debug, Clone, Copy, Default)]
pub struct Mods {
    pub platform: bool,
    pub control: bool,
    pub alt: bool,
    pub shift: bool,
}

/// ⌘G / ⌘⇧G auf dem Mac, Strg+G / Strg+Umschalt+G unter Windows. Alt gehört nie dazu.
pub fn search_step(m: Mods, key: &str, mac: bool) -> Option<SearchStep> {
    let primary = if mac { m.platform && !m.control } else { m.control && !m.platform };
    if key != "g" || !primary || m.alt {
        return None;
    }
    Some(if m.shift { SearchStep::Newer } else { SearchStep::Older })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(text: &str, needle: &str) -> Option<String> {
        link_at(text, text.find(needle).map(|b| text[..b].chars().count()).unwrap())
    }

    #[test]
    fn url_loses_trailing_dot_and_comma() {
        assert_eq!(at("siehe https://kadrell.malura.de.", "kadrell").as_deref(), Some("https://kadrell.malura.de"));
        assert_eq!(at("a https://x.y/z, b", "x.y").as_deref(), Some("https://x.y/z"));
    }

    #[test]
    fn parentheses_around_the_link_fall_away_but_balanced_ones_stay() {
        assert_eq!(at("(https://x.y/a)", "x.y").as_deref(), Some("https://x.y/a"));
        assert_eq!(at("https://de.wikipedia.org/wiki/Foo_(Bar)", "wiki").as_deref(), Some("https://de.wikipedia.org/wiki/Foo_(Bar)"));
        assert_eq!(at("[src/main.rs]", "main").as_deref(), Some("src/main.rs"));
    }

    #[test]
    fn position_suffix_stays_but_a_dangling_colon_goes() {
        assert_eq!(at("fehler in src/lib.rs:12:4: boom", "lib").as_deref(), Some("src/lib.rs:12:4"));
        assert_eq!(at("C:\\dev\\x.txt", "dev").as_deref(), Some("C:\\dev\\x.txt"));
    }

    #[test]
    fn quotes_and_whitespace() {
        assert_eq!(at("open \"a/b.txt\" now", "b.txt").as_deref(), Some("a/b.txt"));
        assert_eq!(link_at("a  b", 2), None, "Leerraum ist kein Link");
        assert_eq!(link_at("abc", 9), None, "hinter dem Text");
        assert_eq!(link_at("...", 1), None, "nur Satzzeichen");
    }

    #[test]
    fn columns_map_to_characters_including_wide_ones() {
        // „ab“, dann ein breites Zeichen auf Spalte 2 und 3, dann „c“ auf Spalte 4.
        let cols = [0, 1, 2, 4];
        assert_eq!(index_at_column(&cols, 1), Some(1));
        assert_eq!(index_at_column(&cols, 3), Some(2), "zweite Hälfte des breiten Zeichens");
        assert_eq!(index_at_column(&cols, 4), Some(3));
        assert_eq!(index_at_column(&cols, 9), None, "hinter dem Text");
        assert_eq!(index_at_column(&[], 0), None);
    }

    #[test]
    fn search_step_keys_per_platform() {
        let ctrl = Mods { control: true, ..Mods::default() };
        let cmd = Mods { platform: true, ..Mods::default() };
        assert_eq!(search_step(ctrl, "g", false), Some(SearchStep::Older));
        assert_eq!(search_step(Mods { shift: true, ..ctrl }, "g", false), Some(SearchStep::Newer));
        assert_eq!(search_step(cmd, "g", true), Some(SearchStep::Older));
        assert_eq!(search_step(Mods { shift: true, ..cmd }, "g", true), Some(SearchStep::Newer));
        assert_eq!(search_step(cmd, "g", false), None, "unter Windows ist die Windows-Taste kein Strg");
        assert_eq!(search_step(Mods { alt: true, ..ctrl }, "g", false), None, "Strg+Alt ist AltGr");
        assert_eq!(search_step(ctrl, "h", false), None);
    }
}
