//! Vorschau (⌥J / ⌥K): den Baum durchblättern und die Session dort allein auf der Arbeitsfläche zeigen, ohne die
//! Auswahl zu ändern und ohne Tastatur. ⏎ übernimmt sie, Esc und jede andere Aktion gehen zurück.
//! Port von `stepPreview` und `endPreview` (AppDelegate) und `sessionId(after:step:)` des Baums.

/// Wohin die Tastatur nach der Vorschau zurückgeht.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Keyboard {
    /// Zur Fokus-Kachel (Claude).
    Tile,
    /// In den Baum (⌘1).
    Tree,
}

/// Was nach dem Ende der Vorschau zu tun ist.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ended {
    /// Die Session wird die Auswahl.
    Commit(String),
    /// Zurück zur alten Auswahl, die Tastatur geht an den vorigen Besitzer.
    Cancel(Keyboard),
}

#[derive(Default, Debug)]
pub struct Preview {
    shown: Option<String>,
    /// Besitzer der Tastatur vor der Vorschau.
    owner: Option<Keyboard>,
}

/// Nachbar in Baumreihenfolge über die sichtbaren Sessions, am Rand bleibt es stehen. Ohne Start (oder wenn
/// `current` nicht sichtbar ist) die erste.
pub fn neighbor_key(ids: &[String], current: Option<&str>, step: isize) -> Option<String> {
    let last = ids.len().checked_sub(1)?;
    let at = current.and_then(|c| ids.iter().position(|k| k == c));
    let index = at.map_or(0, |i| (i as isize + step).clamp(0, last as isize) as usize);
    ids.get(index).cloned()
}

impl Preview {
    pub fn shown(&self) -> Option<&str> {
        self.shown.as_deref()
    }

    pub fn is_active(&self) -> bool {
        self.shown.is_some()
    }

    /// Einen Schritt im Baum: die erste Vorschau geht von der Fokus-Kachel aus und merkt sich, wer die Tastatur hatte.
    /// Gibt die Session zurück, die jetzt zu zeigen ist, `None` ohne Sessions.
    pub fn step(&mut self, ids: &[String], focus: Option<&str>, keyboard: Keyboard, delta: isize) -> Option<String> {
        let current = self.shown.as_deref().or(focus);
        let next = neighbor_key(ids, current, delta)?;
        if self.shown.is_none() {
            self.owner = Some(keyboard);
        }
        self.shown = Some(next.clone());
        Some(next)
    }

    /// Beendet die Vorschau. `None`, wenn keine lief.
    pub fn end(&mut self, commit: bool) -> Option<Ended> {
        let key = self.shown.take()?;
        let owner = self.owner.take().unwrap_or(Keyboard::Tile);
        Some(if commit { Ended::Commit(key) } else { Ended::Cancel(owner) })
    }

    /// Wirft die Vorschau weg, ohne etwas zu tun (die Auswahl hat sich geändert).
    pub fn clear(&mut self) {
        self.shown = None;
        self.owner = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids() -> Vec<String> {
        ["a", "b", "c"].iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn stepping_moves_through_the_tree_and_stops_at_the_edges() {
        let ids = ids();
        assert_eq!(neighbor_key(&ids, Some("a"), 1).as_deref(), Some("b"));
        assert_eq!(neighbor_key(&ids, Some("c"), 1).as_deref(), Some("c"), "am Rand bleibt es stehen");
        assert_eq!(neighbor_key(&ids, Some("a"), -1).as_deref(), Some("a"));
        assert_eq!(neighbor_key(&ids, None, 1).as_deref(), Some("a"), "ohne Start die erste");
        assert_eq!(neighbor_key(&ids, Some("zugeklappt"), -1).as_deref(), Some("a"));
        assert_eq!(neighbor_key(&[], Some("a"), 1), None);
    }

    #[test]
    fn preview_starts_at_the_focus_and_continues_from_the_shown_session() {
        let ids = ids();
        let mut p = Preview::default();
        assert!(!p.is_active());
        assert_eq!(p.step(&ids, Some("a"), Keyboard::Tile, 1).as_deref(), Some("b"));
        assert_eq!(p.shown(), Some("b"));
        assert_eq!(p.step(&ids, Some("a"), Keyboard::Tree, 1).as_deref(), Some("c"), "weiter ab der gezeigten, nicht vom Fokus");
        assert_eq!(p.step(&ids, Some("a"), Keyboard::Tree, -1).as_deref(), Some("b"));
        assert!(p.is_active());
    }

    #[test]
    fn enter_commits_and_escape_returns_the_keyboard_to_its_owner() {
        let ids = ids();
        let mut p = Preview::default();
        p.step(&ids, Some("a"), Keyboard::Tree, 1);
        p.step(&ids, Some("a"), Keyboard::Tile, 1);
        assert_eq!(p.end(false), Some(Ended::Cancel(Keyboard::Tree)), "der erste Besitzer zählt");
        assert!(!p.is_active());
        assert_eq!(p.end(false), None);
        p.step(&ids, Some("a"), Keyboard::Tile, 1);
        assert_eq!(p.end(true), Some(Ended::Commit("b".into())));
        assert_eq!(p.end(true), None);
    }

    #[test]
    fn clear_forgets_the_preview_without_an_outcome() {
        let ids = ids();
        let mut p = Preview::default();
        p.step(&ids, None, Keyboard::Tile, 1);
        p.clear();
        assert!(!p.is_active());
        assert_eq!(p.end(true), None);
    }

    #[test]
    fn no_sessions_means_no_preview() {
        let mut p = Preview::default();
        assert_eq!(p.step(&[], None, Keyboard::Tile, 1), None);
        assert!(!p.is_active());
    }
}
