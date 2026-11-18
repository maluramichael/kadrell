//! Tasten der Extension-Oberfläche: ⌘3 (Tastatur in die rechte Seitenleiste), ⌘⌥B (Seitenleiste ein/aus) und das
//! Kürzel des Extensions-Dialogs.
//!
//! Der Dialog hat auf dem Mac ⌥F4 als Standard (`HotkeyAction::ShowExtensions`). Unter Windows schließt Alt+F4 das
//! Fenster, dort gilt deshalb Strg+Umschalt+E, solange der Nutzer nichts anderes eingestellt hat (die Einstellungen
//! speichern nur Abweichungen vom Standard, ein gespeichertes Kürzel gewinnt immer). Die Wahl erfüllt, was die
//! Einstellungen zulassen (`Hotkey::is_usable`: Strg, Alt oder Cmd dabei), kollidiert mit keinem Standardkürzel von
//! `HotkeyAction::ALL` und vermeidet Strg+Alt, das unter Windows AltGr ist und dem Terminal gehört.

use gpui::{Action, App, KeyBinding};
use kadrell_core::Settings;
use kadrell_core::hotkeys::{self, Hotkey, HotkeyAction, mods};

use crate::lang::{Modifier, modifier_name};
use super::{FocusRightSidebar, OpenExtensions, ToggleRightSidebar};

/// Standard des Extensions-Dialogs je Plattform: Mac ⌥F4, Windows Strg+Umschalt+E.
pub fn dialog_default(mac: bool) -> Hotkey {
    if mac { HotkeyAction::ShowExtensions.default_key() } else { Hotkey::new(mods::CONTROL | mods::SHIFT, "e") }
}

/// Belegung einer Aktion: Einstellungen, sonst Standard; unter Windows ersetzt der Windows-Standard das ⌥F4, solange
/// der Nutzer nichts gespeichert hat (die gespeicherte Belegung wäre sonst nicht vom Standard zu unterscheiden).
pub fn binding(settings: &Settings, action: HotkeyAction, mac: bool) -> Option<Hotkey> {
    let key = hotkeys::current(settings).get(&action)?.clone();
    if action == HotkeyAction::ShowExtensions && key == action.default_key() && !mac {
        return Some(dialog_default(false));
    }
    (!key.key.is_empty()).then_some(key)
}

/// Name einer Taste im Tastenstring von GPUI (`left`, `f4`, `escape`), `None` für Unbekanntes.
fn gpui_key(key: &str) -> Option<String> {
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

/// Tastenstring für `KeyBinding::new`: ⌘ ist auf dem Mac `cmd`, unter Windows Strg (wie überall in der App).
pub fn keystroke(hotkey: &Hotkey, mac: bool) -> Option<String> {
    let mut parts: Vec<&str> = Vec::new();
    let command_is_ctrl = !mac && hotkey.has(mods::COMMAND);
    if hotkey.has(mods::CONTROL) || command_is_ctrl {
        parts.push("ctrl");
    }
    if hotkey.has(mods::OPTION) {
        parts.push("alt");
    }
    if hotkey.has(mods::SHIFT) {
        parts.push("shift");
    }
    if mac && hotkey.has(mods::COMMAND) {
        parts.push("cmd");
    }
    let key = gpui_key(&hotkey.key)?;
    parts.push(&key);
    Some(parts.join("-"))
}

/// Anzeige für Hinweise und Menüs: auf dem Mac `⌥F4`, unter Windows `Strg+Umschalt+E`.
pub fn label(hotkey: &Hotkey, mac: bool) -> String {
    if mac {
        return hotkey.display();
    }
    let lang = kadrell_i18n::global().lang();
    let mut parts: Vec<String> = Vec::new();
    if hotkey.has(mods::CONTROL) || hotkey.has(mods::COMMAND) {
        parts.push(modifier_name(Modifier::Ctrl, lang).into());
    }
    if hotkey.has(mods::OPTION) {
        parts.push(modifier_name(Modifier::Alt, lang).into());
    }
    if hotkey.has(mods::SHIFT) {
        parts.push(modifier_name(Modifier::Shift, lang).into());
    }
    let key = if hotkey.key.chars().count() == 1 { hotkey.key.to_uppercase() } else { hotkey.key.clone() };
    parts.push(key);
    parts.join("+")
}

fn bind_one(cx: &App, settings: &Settings, action: HotkeyAction, command: impl Action, mac: bool) -> Option<KeyBinding> {
    let stroke = keystroke(&binding(settings, action, mac)?, mac)?;
    KeyBinding::load(&stroke, Box::new(command), None, false, None, cx).ok()
}

/// Trägt die drei Kürzel ein (einmal beim Start). Eine unlesbare oder gelöschte Belegung bleibt aus.
pub fn bind_keys(cx: &mut App, settings: &Settings) {
    let mac = cfg!(target_os = "macos");
    let bindings = [
        bind_one(cx, settings, HotkeyAction::ShowExtensions, OpenExtensions, mac),
        bind_one(cx, settings, HotkeyAction::FocusRightSidebar, FocusRightSidebar, mac),
        bind_one(cx, settings, HotkeyAction::ToggleRightSidebar, ToggleRightSidebar, mac),
    ];
    cx.bind_keys(bindings.into_iter().flatten());
}

/// Hinweis in der Fußzeile des Dialogs: „Esc oder ⌥F4 schließen“ bzw. „Esc schließt“ ohne Kürzel.
pub fn close_hint(settings: &Settings, mac: bool) -> Option<String> {
    binding(settings, HotkeyAction::ShowExtensions, mac).map(|k| label(&k, mac))
}

#[cfg(test)]
mod tests {
    use super::*;
    use kadrell_core::hotkeys::HotkeyMap;

    #[test]
    fn windows_default_is_allowed_unique_and_avoids_altgr_and_alt_f4() {
        let key = dialog_default(false);
        assert!(key.is_usable(), "die Einstellungen lassen es zu");
        assert!(!key.has(mods::OPTION), "kein Alt: Alt+F4 und AltGr bleiben frei");
        for a in HotkeyAction::ALL {
            assert_ne!(a.default_key(), key, "{a:?} belegt es schon");
        }
        assert_eq!(dialog_default(true), Hotkey::new(mods::OPTION, "F4"));
    }

    #[test]
    fn windows_replaces_only_the_untouched_default() {
        let none = Settings::default();
        assert_eq!(binding(&none, HotkeyAction::ShowExtensions, true), Some(Hotkey::new(mods::OPTION, "F4")));
        assert_eq!(binding(&none, HotkeyAction::ShowExtensions, false), Some(dialog_default(false)));
        let mut saved = Settings::default();
        let mut map: HotkeyMap = hotkeys::current(&saved);
        map.insert(HotkeyAction::ShowExtensions, Hotkey::new(mods::COMMAND | mods::SHIFT, "x"));
        hotkeys::store(&mut saved, &map);
        assert_eq!(binding(&saved, HotkeyAction::ShowExtensions, false), Some(Hotkey::new(mods::COMMAND | mods::SHIFT, "x")), "gespeichert gewinnt");
        map.insert(HotkeyAction::ShowExtensions, Hotkey::new(0, ""));
        hotkeys::store(&mut saved, &map);
        assert_eq!(binding(&saved, HotkeyAction::ShowExtensions, false), None, "bewusst ohne Kürzel");
    }

    #[test]
    fn keystrokes_follow_the_platform() {
        let cmd3 = HotkeyAction::FocusRightSidebar.default_key();
        assert_eq!(keystroke(&cmd3, true).as_deref(), Some("cmd-3"));
        assert_eq!(keystroke(&cmd3, false).as_deref(), Some("ctrl-3"));
        let toggle = HotkeyAction::ToggleRightSidebar.default_key();
        assert_eq!(keystroke(&toggle, true).as_deref(), Some("alt-cmd-b"));
        assert_eq!(keystroke(&toggle, false).as_deref(), Some("ctrl-alt-b"));
        assert_eq!(keystroke(&Hotkey::new(mods::OPTION, "F4"), true).as_deref(), Some("alt-f4"));
        assert_eq!(keystroke(&dialog_default(false), false).as_deref(), Some("ctrl-shift-e"));
        assert_eq!(keystroke(&Hotkey::new(mods::OPTION, "←"), false).as_deref(), Some("alt-left"));
        assert_eq!(keystroke(&Hotkey::new(mods::OPTION, "Esc"), false).as_deref(), Some("alt-escape"));
        assert_eq!(keystroke(&Hotkey::new(mods::OPTION, "??"), false), None);
    }

    #[test]
    fn labels_use_windows_names_there() {
        assert_eq!(label(&dialog_default(false), false), "Strg+Umschalt+E");
        assert_eq!(label(&HotkeyAction::FocusRightSidebar.default_key(), false), "Strg+3");
        assert_eq!(label(&HotkeyAction::ShowExtensions.default_key(), true), "⌥F4");
        assert_eq!(close_hint(&Settings::default(), false).as_deref(), Some("Strg+Umschalt+E"));
    }
}
