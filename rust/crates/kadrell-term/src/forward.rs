//! Was von einer Eingabe an die anderen Kacheln mitgeht, wenn die App Eingaben an mehrere Terminals verteilt (Sync,
//! wie tmux `synchronize-panes`). Dieselbe Übersetzung wie im Terminal selbst (`key_bytes`, Einfügen), damit alle
//! Kacheln dasselbe bekommen wie die mit der Tastatur.

use gpui::Keystroke;

use crate::terminal::{is_cmd, key_bytes};

/// Eingabe für die übrigen Kacheln.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Forward {
    /// Einfügen (⌘V bzw. Strg+Umschalt+V): jede Kachel liest die Zwischenablage selbst, mit ihrem eigenen Bracketed-Paste.
    Paste,
    /// Rohe Bytes für den Prozess.
    Bytes(Vec<u8>),
    /// Bleibt lokal (Kopieren, Suchen, Programmkürzel mit ⌘).
    Ignore,
}

/// Was `ks` an die anderen Kacheln schickt. Pfeiltasten gehen im Standardmodus (ohne Anwendungs-Cursor) hinaus.
pub fn forward_of(ks: &Keystroke) -> Forward {
    if is_cmd(ks, "v") {
        return Forward::Paste;
    }
    if is_cmd(ks, "c") || is_cmd(ks, "f") {
        return Forward::Ignore;
    }
    if let Some(bytes) = key_bytes(ks, false) {
        return Forward::Bytes(bytes);
    }
    let m = ks.modifiers;
    match &ks.key_char {
        Some(text) if !text.is_empty() && !m.platform && (!m.control || m.alt) => Forward::Bytes(text.clone().into_bytes()),
        _ => Forward::Ignore,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::Modifiers;

    fn stroke(key: &str, key_char: Option<&str>, modifiers: Modifiers) -> Keystroke {
        Keystroke { modifiers, key: key.to_string(), key_char: key_char.map(str::to_string) }
    }

    fn plain(key: &str, key_char: Option<&str>) -> Keystroke {
        stroke(key, key_char, Modifiers::default())
    }

    #[test]
    fn typed_text_goes_out_as_utf8() {
        assert_eq!(forward_of(&plain("a", Some("a"))), Forward::Bytes(b"a".to_vec()));
        assert_eq!(forward_of(&plain("a", Some("ä"))), Forward::Bytes("ä".as_bytes().to_vec()));
        assert_eq!(forward_of(&plain("a", None)), Forward::Ignore);
    }

    #[test]
    fn special_keys_use_the_terminal_encoding() {
        assert_eq!(forward_of(&plain("enter", None)), Forward::Bytes(b"\r".to_vec()));
        assert_eq!(forward_of(&plain("left", None)), Forward::Bytes(b"\x1b[D".to_vec()));
        assert_eq!(forward_of(&plain("backspace", None)), Forward::Bytes(vec![0x7f]));
        let ctrl_c = stroke("c", None, Modifiers { control: true, ..Modifiers::default() });
        if cfg!(target_os = "macos") {
            assert_eq!(forward_of(&ctrl_c), Forward::Bytes(vec![3]));
        }
    }

    #[test]
    fn paste_copy_and_find_follow_the_platform_shortcut() {
        let primary = if cfg!(target_os = "macos") {
            Modifiers { platform: true, ..Modifiers::default() }
        } else {
            Modifiers { control: true, shift: true, ..Modifiers::default() }
        };
        assert_eq!(forward_of(&stroke("v", None, primary)), Forward::Paste);
        assert_eq!(forward_of(&stroke("c", None, primary)), Forward::Ignore);
        assert_eq!(forward_of(&stroke("f", None, primary)), Forward::Ignore);
    }

    #[test]
    fn command_combinations_stay_local() {
        let cmd = Modifiers { platform: true, ..Modifiers::default() };
        assert_eq!(forward_of(&stroke("l", Some("l"), cmd)), Forward::Ignore);
    }

    #[test]
    fn altgr_text_is_forwarded() {
        let altgr = Modifiers { control: true, alt: true, ..Modifiers::default() };
        assert_eq!(forward_of(&stroke("q", Some("@"), altgr)), Forward::Bytes(b"@".to_vec()));
    }
}
