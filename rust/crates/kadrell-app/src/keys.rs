//! App-Kürzel je Plattform. Die Mac-App bindet auf ⌘, ⌥, ⌃ und ⇧. Unter Windows liegt ⌘ auf Alt, damit Strg+B, Strg+R,
//! Strg+P usw. bei Claude Code im eingebetteten Terminal ankommen (Michael, 19.11.). Strg bleibt Strg (Mac ⌃) und
//! Textfelder behalten Strg+C/V/X/A/Z.
//!
//! ⌘ und ⌥ fallen unter Windows beide auf Alt. Bei Doppelbelegung gewinnt das ⌘-Kürzel, die ⌥-Aktion bekommt einen
//! eigenen Windows-Standard (`workspace::keys::windows_default`). ⌘⌥ zusammen (Mac) und ⌃⌥ ergeben Strg+Alt: dort
//! stehen nur Pfeile und Buchstaben, die auf deutscher Tastatur kein AltGr-Zeichen sind (nicht q e m 2 3 7 8 9 0 ß + <).
//!
//! Alle festen Bindungen laufen über `platform`: im Code steht der Mac-Tastenstring, Windows bekommt ihn umgesetzt.

use kadrell_core::hotkeys::{Hotkey, mods};

use crate::lang::{Modifier, modifier_name};

/// Die Tasten wie Windows sie sieht: Strg, Alt, Umschalt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Physical {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

impl Physical {
    /// Aus den Modifier-Bits des Kerns (Mac-Format): ⌘ und ⌥ sind Alt, ⌃ ist Strg, ⌘⌥ ist Strg+Alt.
    pub fn from_bits(bits: u64) -> Physical {
        let (cmd, opt) = (bits & mods::COMMAND != 0, bits & mods::OPTION != 0);
        Physical { ctrl: bits & mods::CONTROL != 0 || (cmd && opt), alt: cmd || opt, shift: bits & mods::SHIFT != 0 }
    }

    /// Aus den Modifiern eines Tastendrucks. Die Win-Taste zählt wie Strg (sie stand früher für ⌃).
    pub fn from_input(control: bool, platform: bool, alt: bool, shift: bool) -> Physical {
        Physical { ctrl: control || platform, alt, shift }
    }

    /// Alle Namen in der Reihenfolge Strg, Alt, Umschalt.
    pub fn names(self, lang: kadrell_i18n::Lang) -> Vec<&'static str> {
        [(self.ctrl, Modifier::Ctrl), (self.alt, Modifier::Alt), (self.shift, Modifier::Shift)]
            .into_iter()
            .filter(|(on, _)| *on)
            .map(|(_, m)| modifier_name(m, lang))
            .collect()
    }
}

/// Tastenstring für `KeyBinding` auf dieser Plattform aus dem Mac-Tastenstring (`cmd-shift-n`).
/// `KeyBinding::new` verlangt `&'static str`, der umgesetzte Text wird deshalb einmal beim Start angelegt und bleibt.
pub fn platform(stroke: &'static str) -> &'static str {
    if cfg!(target_os = "macos") { stroke } else { Box::leak(windows_stroke(stroke).into_boxed_str()) }
}

/// „Neue Session“ ist die bewusste Ausnahme von „Strg gehört dem Terminal“: Mac ⌘N, Windows Strg+N (Michael, 20.11.).
/// Strg+N kommt dadurch nicht mehr im Terminal an, Alt+N bleibt frei. Beschriftung: `lang::windows_run`.
const NEW_SESSION_MAC: &str = "cmd-n";
const NEW_SESSION_WINDOWS: &str = "ctrl-n";

/// Mac-Tastenstring unter Windows: `cmd` wird `alt` (außer ⌘N, siehe oben), `cmd` mit `alt` und `ctrl` mit `alt`
/// werden `ctrl-alt`.
pub fn windows_stroke(stroke: &str) -> String {
    if stroke == NEW_SESSION_MAC {
        return NEW_SESSION_WINDOWS.to_string();
    }
    let mut bits = 0;
    let mut rest = stroke;
    while let Some((head, tail)) = rest.split_once('-').filter(|(_, tail)| !tail.is_empty()) {
        bits |= match head {
            "cmd" => mods::COMMAND,
            "ctrl" => mods::CONTROL,
            "alt" => mods::OPTION,
            "shift" => mods::SHIFT,
            _ => break,
        };
        rest = tail;
    }
    stroke_of(Physical::from_bits(bits), rest)
}

/// Tastenstring aus den Windows-Tasten und dem Namen der Taste.
pub fn stroke_of(p: Physical, key: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for (on, name) in [(p.ctrl, "ctrl"), (p.alt, "alt"), (p.shift, "shift")] {
        if on {
            parts.push(name);
        }
    }
    parts.push(key);
    parts.join("-")
}

/// Name einer Taste im Tastenstring von GPUI (`left`, `f4`, `escape`), `None` für Unbekanntes.
pub fn gpui_key(key: &str) -> Option<String> {
    let named = match key {
        "←" => "left",
        "→" => "right",
        "↑" => "up",
        "↓" => "down",
        "⏎" => "enter",
        "⇥" => "tab",
        "Esc" => "escape",
        "Space" => "space",
        "⌫" => "backspace",
        k if k.len() > 1 && k.starts_with('F') && k[1..].chars().all(|c| c.is_ascii_digit()) => return Some(k.to_lowercase()),
        k if k.chars().count() == 1 => return Some(k.to_lowercase()),
        _ => return None,
    };
    Some(named.to_string())
}

/// Die Windows-Tasten eines Kürzels samt GPUI-Name der Taste, `None` für eine Taste ohne Namen.
pub fn physical_of(h: &Hotkey) -> Option<(Physical, String)> {
    Some((Physical::from_bits(h.mods), gpui_key(&h.key)?))
}

/// Feste Bindungen der App als Mac-Tastenstring, mit der Stelle, die sie bindet. Der Rekorder der Einstellungen lehnt
/// diese Tasten ab (sie gehören der App), der Test prüft, dass unter Windows keine zwei dasselbe sind. Wer eine feste
/// Bindung dazunimmt, trägt sie hier ein.
pub const FIXED: &[(&str, &str)] = &[
    ("menus: Neue Session", "cmd-n"),
    ("menus: Remote verbinden", "cmd-shift-n"),
    ("menus: Neue Session im selben Ordner", "cmd-enter"),
    ("menus: Neues Terminal", "cmd-t"),
    ("menus: Kachel zoomen", "cmd-shift-enter"),
    ("menus: Neue Gruppe", "cmd-alt-n"),
    ("menus: Suche", "cmd-p"),
    ("menus: In allen Terminals suchen", "cmd-shift-f"),
    ("menus: Einstellungen", "cmd-,"),
    ("menus: Beenden", "cmd-q"),
    ("menus: Fenster schließen", "cmd-shift-w"),
    ("menus: Neues Fenster", "cmd-shift-t"),
    ("menus: Alle Sessions auswählen", "cmd-shift-a"),
    ("windows: Baum ein/aus", "cmd-b"),
    ("tree_actions: Alle Gruppen auf-/zuklappen", "cmd-shift-b"),
    ("zoom: Schrift größer =", "cmd-="),
    ("zoom: Schrift größer +", "cmd-+"),
    ("zoom: Schrift größer Umschalt+=", "cmd-shift-="),
    ("zoom: Schrift kleiner", "cmd--"),
    ("zoom: Schrift Standard", "cmd-0"),
    ("lib: Farbschema wechseln", "cmd-alt-t"),
    ("tile_context: Im Terminal suchen", "cmd-f"),
    ("tile_context: Weitersuchen", "cmd-g"),
    ("tile_context: Rückwärts suchen", "cmd-shift-g"),
    ("tile_context: Kopieren", "cmd-c"),
    ("tile_context: Einsetzen", "cmd-v"),
];

/// Feste Bindungen mit Kontext: im Dialog gilt dieselbe Taste wie sonst für „Neue Session im selben Ordner“, das ist
/// gewollt (Dialoge sind modal) und kein Konflikt.
pub const FIXED_IN_DIALOGS: &[&str] = &["cmd-enter", "cmd-o"];

/// Gehört die Taste der App (feste Bindung) oder dem System? Dann nimmt der Rekorder sie nicht auf. Nur Windows.
pub fn is_reserved(p: Physical, key: &str) -> bool {
    let taken = |stroke: &str| split(&windows_stroke(stroke)) == (p, key.to_string());
    FIXED.iter().any(|(_, s)| taken(s)) || FIXED_IN_DIALOGS.iter().any(|s| taken(s)) || system_key(p, key)
}

/// Alt+F4, Alt+Tab, Alt+Leertaste und Alt+Esc gehören dem System und erreichen die App nicht (oder schließen sie).
fn system_key(p: Physical, key: &str) -> bool {
    p.alt && !p.ctrl && matches!(key, "f4" | "tab" | "space" | "escape")
}

/// Tastenstring in Windows-Tasten und Taste zerlegen (Gegenstück zu `stroke_of`).
fn split(stroke: &str) -> (Physical, String) {
    let mut p = Physical { ctrl: false, alt: false, shift: false };
    let mut rest = stroke;
    while let Some((head, tail)) = rest.split_once('-').filter(|(_, tail)| !tail.is_empty()) {
        match head {
            "ctrl" => p.ctrl = true,
            "alt" => p.alt = true,
            "shift" => p.shift = true,
            _ => break,
        }
        rest = tail;
    }
    (p, rest.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use kadrell_core::hotkeys::HotkeyAction;
    use std::collections::HashMap;

    #[test]
    fn windows_turns_cmd_into_alt_and_keeps_ctrl() {
        assert_eq!(windows_stroke("cmd-n"), "ctrl-n", "Ausnahme: Neue Session liegt auf Strg+N");
        assert_eq!(windows_stroke("cmd-p"), "alt-p");
        assert_eq!(windows_stroke("cmd-shift-n"), "alt-shift-n");
        assert_eq!(windows_stroke("cmd-enter"), "alt-enter");
        assert_eq!(windows_stroke("cmd-alt-n"), "ctrl-alt-n");
        assert_eq!(windows_stroke("alt-left"), "alt-left");
        assert_eq!(windows_stroke("ctrl-alt-left"), "ctrl-alt-left");
        assert_eq!(windows_stroke("ctrl-enter"), "ctrl-enter");
        assert_eq!(windows_stroke("f2"), "f2");
    }

    #[test]
    fn new_session_reserves_ctrl_n_and_frees_alt_n() {
        let ctrl_n = Physical { ctrl: true, alt: false, shift: false };
        let alt_n = Physical { ctrl: false, alt: true, shift: false };
        assert!(is_reserved(ctrl_n, "n"));
        assert!(!is_reserved(alt_n, "n"));
    }

    #[test]
    fn symbol_keys_survive_the_conversion() {
        assert_eq!(windows_stroke("cmd-,"), "alt-,");
        assert_eq!(windows_stroke("cmd--"), "alt--");
        assert_eq!(windows_stroke("cmd-="), "alt-=");
        assert_eq!(windows_stroke("cmd-shift-="), "alt-shift-=");
        assert_eq!(windows_stroke("cmd-0"), "alt-0");
    }

    #[test]
    fn platform_is_the_windows_stroke_here() {
        if cfg!(target_os = "macos") {
            assert_eq!(platform("cmd-n"), "cmd-n");
        } else {
            assert_eq!(platform("cmd-n"), "ctrl-n");
            assert_eq!(platform("cmd-p"), "alt-p");
        }
    }

    #[test]
    fn physical_keys_merge_command_and_option() {
        let alt = Physical { ctrl: false, alt: true, shift: false };
        assert_eq!(Physical::from_bits(mods::COMMAND), alt);
        assert_eq!(Physical::from_bits(mods::OPTION), alt);
        let ctrl_alt = Physical { ctrl: true, alt: true, shift: false };
        assert_eq!(Physical::from_bits(mods::COMMAND | mods::OPTION), ctrl_alt);
        assert_eq!(Physical::from_bits(mods::CONTROL | mods::OPTION), ctrl_alt);
        assert_eq!(Physical::from_input(false, true, true, false), ctrl_alt, "die Win-Taste zählt wie Strg");
    }

    /// Kein Alt+Strg mit Tasten, die auf deutscher Tastatur AltGr-Zeichen sind.
    const ALTGR: &str = "qem237890ß+<";

    fn all_windows_bindings() -> Vec<(String, Physical, String)> {
        let mut out: Vec<(String, Physical, String)> = Vec::new();
        for (name, stroke) in FIXED {
            let (p, k) = split(&windows_stroke(stroke));
            out.push((name.to_string(), p, k));
        }
        for &a in HotkeyAction::ALL {
            let h = crate::workspace::keys::windows_default(a).unwrap_or_else(|| a.default_key());
            if let Some((p, k)) = physical_of(&h) {
                out.push((format!("{a:?}"), p, k));
            }
        }
        out
    }

    #[test]
    fn no_two_windows_bindings_are_the_same() {
        let mut seen: HashMap<(Physical, String), String> = HashMap::new();
        for (name, p, k) in all_windows_bindings() {
            if let Some(other) = seen.insert((p, k.clone()), name.clone()) {
                panic!("{name} und {other} sind beide {p:?} {k}");
            }
        }
    }

    #[test]
    fn no_windows_binding_is_ctrl_alone_altgr_or_a_system_key() {
        for (name, p, k) in all_windows_bindings() {
            // Einzige Ausnahme: „Neue Session“ auf Strg+N (bewusste Entscheidung des Nutzers).
            let new_session = name == "menus: Neue Session" && p == Physical { ctrl: true, alt: false, shift: false } && k == "n";
            assert!(
                p.alt || p.shift || k.starts_with('f') && k.len() > 1 || new_session,
                "{name}: Strg allein gehört dem Terminal (einzige Ausnahme: Neue Session auf Strg+N)"
            );
            if p.ctrl && p.alt {
                assert!(!ALTGR.contains(k.as_str()) || k.chars().count() != 1, "{name}: Strg+Alt+{k} ist AltGr");
            }
            assert!(!system_key(p, &k), "{name} gehört dem System");
        }
    }
}
