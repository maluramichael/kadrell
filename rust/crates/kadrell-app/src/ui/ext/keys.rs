//! Tasten der Extension-Oberfläche: ⌘3 (Tastatur in die rechte Seitenleiste), ⌘⌥B (Seitenleiste ein/aus) und das
//! Kürzel des Extensions-Dialogs.
//!
//! Der Dialog hat auf dem Mac ⌥F4 als Standard (`HotkeyAction::ShowExtensions`). Unter Windows schließt Alt+F4 das
//! Fenster, dort gilt deshalb Alt+Umschalt+E, solange der Nutzer nichts anderes eingestellt hat (die Einstellungen
//! speichern nur Abweichungen vom Standard, ein gespeichertes Kürzel gewinnt immer; die Windows-Standards stehen in
//! `workspace::keys::windows_default`). Die Wahl erfüllt, was die Einstellungen zulassen (`Hotkey::is_usable`),
//! kollidiert mit keinem anderen Windows-Kürzel (Test in `keys`) und vermeidet Strg+Alt, das unter Windows AltGr ist.
//! Unter Windows liegt ⌘ auf Alt (`crate::keys`), ⌘3 ist dort Alt+3 und ⌘⌥B Strg+Alt+B.

use gpui::{Action, App, KeyBinding};
use kadrell_core::Settings;
use kadrell_core::hotkeys::{Hotkey, HotkeyAction, mods};

use super::{FocusRightSidebar, OpenExtensions, ToggleRightSidebar};
use crate::keys::{Physical, gpui_key, stroke_of};
use crate::ui::settings::shortcut::Platform;
use crate::workspace::keys::{effective_map, platform_default};

fn platform_of(mac: bool) -> Platform {
    if mac { Platform::Mac } else { Platform::Windows }
}

/// Standard des Extensions-Dialogs je Plattform: Mac ⌥F4, Windows Alt+Umschalt+E.
pub fn dialog_default(mac: bool) -> Hotkey {
    platform_default(HotkeyAction::ShowExtensions, platform_of(mac))
}

/// Belegung einer Aktion: Einstellungen, sonst Standard der Plattform (unter Windows ersetzt der Windows-Standard den
/// Mac-Standard, solange der Nutzer nichts gespeichert hat). `None` ohne Kürzel.
pub fn binding(settings: &Settings, action: HotkeyAction, mac: bool) -> Option<Hotkey> {
    effective_map(settings, platform_of(mac)).remove(&action).filter(|k| !k.key.is_empty())
}

/// Tastenstring für `KeyBinding::new`: auf dem Mac `cmd`, `alt`, `ctrl`, unter Windows ⌘ als Alt (`crate::keys`).
pub fn keystroke(hotkey: &Hotkey, mac: bool) -> Option<String> {
    let key = gpui_key(&hotkey.key)?;
    if !mac {
        return Some(stroke_of(Physical::from_bits(hotkey.mods), &key));
    }
    let mut parts: Vec<&str> = Vec::new();
    for (flag, name) in [(mods::CONTROL, "ctrl"), (mods::OPTION, "alt"), (mods::SHIFT, "shift"), (mods::COMMAND, "cmd")] {
        if hotkey.has(flag) {
            parts.push(name);
        }
    }
    parts.push(&key);
    Some(parts.join("-"))
}

/// Anzeige für Hinweise und Menüs: auf dem Mac `⌥F4`, unter Windows `Alt+Umschalt+E`.
pub fn label(hotkey: &Hotkey, mac: bool) -> String {
    if mac {
        return hotkey.display();
    }
    let lang = kadrell_i18n::global().lang();
    let mut parts: Vec<String> = Physical::from_bits(hotkey.mods).names(lang).into_iter().map(String::from).collect();
    parts.push(if hotkey.key.chars().count() == 1 { hotkey.key.to_uppercase() } else { hotkey.key.clone() });
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
    use kadrell_core::hotkeys::{self, HotkeyMap};

    #[test]
    fn windows_default_is_allowed_unique_and_avoids_altgr_and_alt_f4() {
        let key = dialog_default(false);
        assert!(key.is_usable(), "die Einstellungen lassen es zu");
        assert!(key.has(mods::SHIFT) && !key.has(mods::CONTROL), "Alt+Umschalt: Alt+F4 und AltGr (Strg+Alt) bleiben frei");
        assert_ne!(key.key, "F4");
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
        assert_eq!(keystroke(&cmd3, false).as_deref(), Some("alt-3"));
        let toggle = HotkeyAction::ToggleRightSidebar.default_key();
        assert_eq!(keystroke(&toggle, true).as_deref(), Some("alt-cmd-b"));
        assert_eq!(keystroke(&toggle, false).as_deref(), Some("ctrl-alt-b"));
        assert_eq!(keystroke(&Hotkey::new(mods::OPTION, "F4"), true).as_deref(), Some("alt-f4"));
        assert_eq!(keystroke(&dialog_default(false), false).as_deref(), Some("alt-shift-e"));
        assert_eq!(keystroke(&Hotkey::new(mods::CONTROL, "k"), false).as_deref(), Some("ctrl-k"), "⌃ bleibt Strg");
        assert_eq!(keystroke(&Hotkey::new(mods::OPTION, "←"), false).as_deref(), Some("alt-left"));
        assert_eq!(keystroke(&Hotkey::new(mods::OPTION, "Esc"), false).as_deref(), Some("alt-escape"));
        assert_eq!(keystroke(&Hotkey::new(mods::OPTION, "??"), false), None);
    }

    #[test]
    fn labels_use_windows_names_there() {
        assert_eq!(label(&dialog_default(false), false), "Alt+Umschalt+E");
        assert_eq!(label(&HotkeyAction::FocusRightSidebar.default_key(), false), "Alt+3");
        assert_eq!(label(&HotkeyAction::ShowExtensions.default_key(), true), "⌥F4");
        assert_eq!(label(&HotkeyAction::ToggleRightSidebar.default_key(), false), "Strg+Alt+B");
        assert_eq!(close_hint(&Settings::default(), false).as_deref(), Some("Alt+Umschalt+E"));
    }
}
