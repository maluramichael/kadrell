//! Sync (tmux `synchronize-panes`): Tippen und Einfügen im Fokus-Terminal gehen an alle offenen Kacheln. Bewusst nicht
//! gespeichert. Hier steht, wer die Eingabe bekommt; was weitergeht, entscheidet `kadrell_term::forward_of`.

/// Die Kacheln, die zusätzlich zur Quelle die Eingabe bekommen: bei eingeschaltetem Sync alle Kacheln der Arbeitsfläche
/// (auch die, die ein Zoom gerade verdeckt) mit laufendem Terminal, ohne die Quelle selbst.
pub fn targets(sync: bool, tiles: &[String], source: &str, running: impl Fn(&str) -> bool) -> Vec<String> {
    if !sync {
        return Vec::new();
    }
    tiles.iter().filter(|k| k.as_str() != source && running(k)).cloned().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tiles() -> Vec<String> {
        ["a", "b", "c", "d"].iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn off_means_nobody_else() {
        assert!(targets(false, &tiles(), "a", |_| true).is_empty());
    }

    #[test]
    fn on_means_every_other_running_tile() {
        assert_eq!(targets(true, &tiles(), "b", |k| k != "d"), ["a", "c"]);
    }

    #[test]
    fn a_single_tile_has_no_targets() {
        assert!(targets(true, &["a".to_string()], "a", |_| true).is_empty());
    }

    #[test]
    fn the_source_is_never_a_target_even_if_it_is_not_a_tile() {
        assert_eq!(targets(true, &tiles(), "zz", |_| true).len(), 4);
    }
}
