//! Kürzel anzeigen und aufnehmen. Der Kern speichert Modifier als `NSEvent`-Bits (⌃⌥⇧⌘), damit eine
//! Einstellungsdatei auf beiden Seiten dasselbe heißt. Auf dem Mac sind das die Tasten selbst. Unter Windows gilt
//! dieselbe Zuordnung wie in der Shell (`secondary`): Strg steht für ⌘, Alt für ⌥, Umschalt für ⇧, die Win-Taste
//! für ⌃. Die Zuordnung steht nur hier, `Platform` macht beide Seiten auf jedem Rechner testbar.

use kadrell_core::Settings;
use kadrell_core::hotkeys::{self, Hotkey, HotkeyAction, mods};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Mac,
    Windows,
}

/// Die Plattform, auf der die App läuft.
pub const fn current() -> Platform {
    if cfg!(target_os = "macos") { Platform::Mac } else { Platform::Windows }
}

/// Anzeige eines Kürzels: Mac `⌥⇧←` (wie `Hotkey.display`), Windows `Alt+Umschalt+←`.
pub fn format(h: &Hotkey, platform: Platform) -> String {
    match platform {
        Platform::Mac => h.display(),
        Platform::Windows => format_windows(h),
    }
}

fn format_windows(h: &Hotkey) -> String {
    let mut parts: Vec<String> = Vec::new();
    for (flag, name) in [(mods::COMMAND, "Strg"), (mods::CONTROL, "Win"), (mods::OPTION, "Alt"), (mods::SHIFT, "Umschalt")] {
        if h.has(flag) {
            parts.push(name.to_string());
        }
    }
    parts.push(h.key.to_uppercase());
    parts.join("+")
}

/// Ein Tastendruck, wie GPUI ihn liefert, ohne GPUI-Typen (testbar).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyInput {
    pub control: bool,
    pub alt: bool,
    pub shift: bool,
    pub platform: bool,
    /// `Keystroke::key`: Zeichen klein oder Name wie `left`, `enter`, `f5`.
    pub key: String,
}

/// Modifier des Tastendrucks als Bits des Kerns.
pub fn modifier_bits(input: &KeyInput, platform: Platform) -> u64 {
    let (command, control) = match platform {
        Platform::Mac => (input.platform, input.control),
        Platform::Windows => (input.control, input.platform),
    };
    [(command, mods::COMMAND), (control, mods::CONTROL), (input.alt, mods::OPTION), (input.shift, mods::SHIFT)]
        .into_iter()
        .filter(|(on, _)| *on)
        .fold(0, |bits, (_, flag)| bits | flag)
}

/// Name der Taste im Kürzel (`←`, `F5`, `n`), `None` für Tasten ohne Namen.
pub fn core_key(key: &str) -> Option<String> {
    let special = match key {
        "left" => "←",
        "right" => "→",
        "up" => "↑",
        "down" => "↓",
        "enter" => "⏎",
        "tab" => "⇥",
        "escape" => "Esc",
        "space" => "Space",
        "backspace" => "⌫",
        _ => "",
    };
    if !special.is_empty() {
        return Some(special.to_string());
    }
    if let Some(n) = key.strip_prefix('f').and_then(|n| n.parse::<u8>().ok()) {
        return (1..=12).contains(&n).then(|| format!("F{n}"));
    }
    (key.chars().count() == 1).then(|| key.to_lowercase())
}

/// Was ein Tastendruck während der Aufnahme bedeutet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Recorded {
    /// Esc ohne Modifier: Aufnahme abbrechen.
    Cancel,
    /// ⌫ ohne Modifier: Kürzel entfernen.
    Clear,
    /// Neues Kürzel.
    Set(Hotkey),
    /// Unbrauchbar (nur ein Zeichen ohne ⌘⌥⌃, unbekannte Taste): Aufnahme läuft weiter.
    Reject,
}

pub fn classify(input: &KeyInput, platform: Platform) -> Recorded {
    let bits = modifier_bits(input, platform);
    let plain = bits == 0;
    if plain && input.key == "escape" {
        return Recorded::Cancel;
    }
    if plain && input.key == "backspace" {
        return Recorded::Clear;
    }
    match core_key(&input.key).map(|k| Hotkey::new(bits, &k)) {
        Some(h) if h.is_usable() => Recorded::Set(h),
        _ => Recorded::Reject,
    }
}

/// Belegt `action` neu (`None` = ohne Kürzel). Ein Kürzel gehört nur einer Aktion: eine andere, die es hatte,
/// verliert es (Mac-App: „doppelt vergeben geht nicht“).
pub fn assign(settings: &mut Settings, action: HotkeyAction, key: Option<Hotkey>) {
    let mut map = hotkeys::current(settings);
    match key {
        Some(h) => {
            map.retain(|other, v| *other == action || *v != h);
            map.insert(action, h);
        }
        None => {
            map.remove(&action);
        }
    }
    hotkeys::store(settings, &map);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(key: &str) -> KeyInput {
        KeyInput { control: false, alt: false, shift: false, platform: false, key: key.to_string() }
    }

    #[test]
    fn mac_format_is_the_core_display() {
        let h = HotkeyAction::SwapLeft.default_key();
        assert_eq!(format(&h, Platform::Mac), "⌥⇧←");
        assert_eq!(format(&HotkeyAction::NextLayout.default_key(), Platform::Mac), "⌘L");
    }

    #[test]
    fn windows_format_spells_out_the_modifiers() {
        assert_eq!(format(&HotkeyAction::FocusLeft.default_key(), Platform::Windows), "Alt+←");
        assert_eq!(format(&HotkeyAction::SwapRight.default_key(), Platform::Windows), "Alt+Umschalt+→");
        assert_eq!(format(&HotkeyAction::NextLayout.default_key(), Platform::Windows), "Strg+L");
        assert_eq!(format(&HotkeyAction::ResizeUp.default_key(), Platform::Windows), "Win+Alt+↑");
        assert_eq!(format(&HotkeyAction::ToggleRightSidebar.default_key(), Platform::Windows), "Strg+Alt+B");
        assert_eq!(format(&HotkeyAction::Focus3.default_key(), Platform::Windows), "Alt+3");
        assert_eq!(format(&HotkeyAction::ShowHelp.default_key(), Platform::Windows), "Alt+F1");
    }

    #[test]
    fn modifiers_map_per_platform() {
        let ctrl = KeyInput { control: true, ..key("l") };
        assert_eq!(modifier_bits(&ctrl, Platform::Windows), mods::COMMAND);
        assert_eq!(modifier_bits(&ctrl, Platform::Mac), mods::CONTROL);
        let win = KeyInput { platform: true, alt: true, ..key("left") };
        assert_eq!(modifier_bits(&win, Platform::Windows), mods::CONTROL | mods::OPTION);
        assert_eq!(modifier_bits(&win, Platform::Mac), mods::COMMAND | mods::OPTION);
    }

    #[test]
    fn key_names_follow_the_core() {
        assert_eq!(core_key("left").as_deref(), Some("←"));
        assert_eq!(core_key("f5").as_deref(), Some("F5"));
        assert_eq!(core_key("f13"), None);
        assert_eq!(core_key("Z").as_deref(), Some("z"));
        assert_eq!(core_key("home"), None);
        assert_eq!(core_key("tab").as_deref(), Some("⇥"));
    }

    #[test]
    fn escape_and_backspace_without_modifier_cancel_and_clear() {
        assert_eq!(classify(&key("escape"), Platform::Windows), Recorded::Cancel);
        assert_eq!(classify(&key("backspace"), Platform::Windows), Recorded::Clear);
    }

    #[test]
    fn alt_arrow_is_recorded() {
        let alt_left = KeyInput { alt: true, ..key("left") };
        assert_eq!(classify(&alt_left, Platform::Windows), Recorded::Set(Hotkey::new(mods::OPTION, "←")));
    }

    #[test]
    fn plain_characters_are_rejected_but_function_keys_pass() {
        assert_eq!(classify(&key("a"), Platform::Windows), Recorded::Reject);
        assert_eq!(classify(&key("f5"), Platform::Windows), Recorded::Set(Hotkey::new(0, "F5")));
        let shift_a = KeyInput { shift: true, ..key("a") };
        assert_eq!(classify(&shift_a, Platform::Mac), Recorded::Reject);
        let alt_esc = KeyInput { alt: true, ..key("escape") };
        assert_eq!(classify(&alt_esc, Platform::Mac), Recorded::Set(Hotkey::new(mods::OPTION, "Esc")));
    }

    #[test]
    fn assigning_a_taken_shortcut_removes_it_from_the_other_action() {
        let mut s = Settings::default();
        assign(&mut s, HotkeyAction::FocusRight, Some(HotkeyAction::FocusLeft.default_key()));
        let map = hotkeys::current(&s);
        assert_eq!(map.get(&HotkeyAction::FocusRight), Some(&HotkeyAction::FocusLeft.default_key()));
        assert_eq!(map.get(&HotkeyAction::FocusLeft), None);
    }

    #[test]
    fn clearing_and_resetting_a_shortcut() {
        let mut s = Settings::default();
        assign(&mut s, HotkeyAction::Zoom, None);
        assert_eq!(hotkeys::current(&s).get(&HotkeyAction::Zoom), None);
        assign(&mut s, HotkeyAction::Zoom, Some(HotkeyAction::Zoom.default_key()));
        assert_eq!(hotkeys::current(&s).get(&HotkeyAction::Zoom), Some(&HotkeyAction::Zoom.default_key()));
        assert_eq!(hotkeys::current(&s).len(), 47);
    }

    #[test]
    fn unknown_keys_are_rejected() {
        let alt_home = KeyInput { alt: true, ..key("home") };
        assert_eq!(classify(&alt_home, Platform::Mac), Recorded::Reject);
    }
}
