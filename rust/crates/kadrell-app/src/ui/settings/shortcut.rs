//! Kürzel anzeigen und aufnehmen. Der Kern speichert Modifier als `NSEvent`-Bits (⌃⌥⇧⌘), damit eine
//! Einstellungsdatei auf beiden Seiten dasselbe heißt. Auf dem Mac sind das die Tasten selbst. Unter Windows liegen
//! ⌘ und ⌥ beide auf Alt (`crate::keys`), ⌃ ist Strg. Der Rekorder speichert deshalb ein gedrücktes Alt als ⌘, bei
//! einer Aktion mit ⌥-Standard als ⌥: so bleibt die Einstellungsdatei mit dem Mac-Format vereinbar, und die Aktion
//! behält ihre Art. Strg+Alt gilt als ⌘⌥ bzw. ⌃⌥. `Platform` macht beide Seiten auf jedem Rechner testbar.

use crate::keys::{Physical, is_reserved, physical_of};
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
    let mut parts: Vec<String> = Physical::from_bits(h.mods).names(kadrell_i18n::global().lang()).into_iter().map(String::from).collect();
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

/// Modifier des Tastendrucks als Bits des Kerns. Mac: die Tasten selbst. Windows: Strg und Win sind ⌃, Alt ist ⌥
/// (nicht ⌘: ob Alt ⌘ oder ⌥ meint, hängt von der Aktion ab, siehe `recorded_bits`).
pub fn modifier_bits(input: &KeyInput, platform: Platform) -> u64 {
    let (command, control) = match platform {
        Platform::Mac => (input.platform, input.control),
        Platform::Windows => (false, input.control || input.platform),
    };
    [(command, mods::COMMAND), (control, mods::CONTROL), (input.alt, mods::OPTION), (input.shift, mods::SHIFT)]
        .into_iter()
        .filter(|(on, _)| *on)
        .fold(0, |bits, (_, flag)| bits | flag)
}

/// Hat die Aktion einen ⌥-Standard (ohne ⌘)? Dann speichert der Rekorder Alt unter Windows als ⌥, sonst als ⌘.
fn option_style(action: Option<HotkeyAction>) -> bool {
    action.is_some_and(|a| {
        let d = a.default_key();
        d.has(mods::OPTION) && !d.has(mods::COMMAND)
    })
}

/// Bits für den Rekorder. Mac wie `modifier_bits`. Windows: Alt wird ⌘ (Aktion mit ⌥-Standard: ⌥), Strg bleibt ⌃,
/// Strg+Alt wird ⌘⌥ (Aktion mit ⌥-Standard: ⌃⌥).
fn recorded_bits(input: &KeyInput, platform: Platform, action: Option<HotkeyAction>) -> u64 {
    let bits = modifier_bits(input, platform);
    if platform == Platform::Mac || !input.alt {
        return bits;
    }
    if option_style(action) {
        return bits;
    }
    if bits & mods::CONTROL != 0 { (bits & !mods::CONTROL) | mods::COMMAND | mods::OPTION } else { (bits & !mods::OPTION) | mods::COMMAND }
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
    classify_for(input, platform, None)
}

/// Wie `classify` für eine bestimmte Aktion (bestimmt unter Windows, ob Alt als ⌘ oder ⌥ gespeichert wird). Unter
/// Windows nimmt der Rekorder keine Taste auf, die der App oder dem System gehört (feste Bindungen, Alt+F4 u. ä.).
pub fn classify_for(input: &KeyInput, platform: Platform, action: Option<HotkeyAction>) -> Recorded {
    let plain = modifier_bits(input, platform) == 0;
    if plain && input.key == "escape" {
        return Recorded::Cancel;
    }
    if plain && input.key == "backspace" {
        return Recorded::Clear;
    }
    let bits = recorded_bits(input, platform, action);
    match core_key(&input.key).map(|k| Hotkey::new(bits, &k)) {
        Some(h) if h.is_usable() && !reserved(&h, platform) => Recorded::Set(h),
        _ => Recorded::Reject,
    }
}

fn reserved(h: &Hotkey, platform: Platform) -> bool {
    platform == Platform::Windows && physical_of(h).is_some_and(|(p, key)| is_reserved(p, &key))
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
        assert_eq!(format(&HotkeyAction::NextLayout.default_key(), Platform::Windows), "Alt+L");
        assert_eq!(format(&HotkeyAction::ResizeUp.default_key(), Platform::Windows), "Strg+Alt+↑");
        assert_eq!(format(&Hotkey::new(mods::COMMAND | mods::SHIFT, "n"), Platform::Windows), "Alt+Umschalt+N");
        assert_eq!(format(&HotkeyAction::ToggleRightSidebar.default_key(), Platform::Windows), "Strg+Alt+B");
        assert_eq!(format(&HotkeyAction::Focus3.default_key(), Platform::Windows), "Alt+3");
        assert_eq!(format(&HotkeyAction::ShowHelp.default_key(), Platform::Windows), "Alt+F1");
    }

    #[test]
    fn modifiers_map_per_platform() {
        let ctrl = KeyInput { control: true, ..key("l") };
        assert_eq!(modifier_bits(&ctrl, Platform::Windows), mods::CONTROL, "Strg bleibt Strg");
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
    fn alt_is_recorded_as_command_unless_the_action_is_an_option_one() {
        let alt_y = KeyInput { alt: true, ..key("y") };
        assert_eq!(classify(&alt_y, Platform::Windows), Recorded::Set(Hotkey::new(mods::COMMAND, "y")));
        let for_zoom = classify_for(&alt_y, Platform::Windows, Some(HotkeyAction::Zoom));
        assert_eq!(for_zoom, Recorded::Set(Hotkey::new(mods::OPTION, "y")), "Zoom hat den ⌥-Standard ⌥Z");
        let for_layout = classify_for(&alt_y, Platform::Windows, Some(HotkeyAction::NextLayout));
        assert_eq!(for_layout, Recorded::Set(Hotkey::new(mods::COMMAND, "y")), "Layout hat den ⌘-Standard ⌘L");
        let alt_shift_y = KeyInput { alt: true, shift: true, ..key("y") };
        assert_eq!(classify(&alt_shift_y, Platform::Windows), Recorded::Set(Hotkey::new(mods::COMMAND | mods::SHIFT, "y")));
    }

    #[test]
    fn alt_arrow_is_recorded() {
        let alt_left = KeyInput { alt: true, ..key("left") };
        assert_eq!(classify_for(&alt_left, Platform::Windows, Some(HotkeyAction::FocusLeft)), Recorded::Set(Hotkey::new(mods::OPTION, "←")));
        assert_eq!(classify(&alt_left, Platform::Windows), Recorded::Set(Hotkey::new(mods::COMMAND, "←")));
    }

    #[test]
    fn ctrl_alt_is_recorded_as_a_combination_and_ctrl_alone_as_control() {
        let ctrl_alt = KeyInput { control: true, alt: true, ..key("up") };
        assert_eq!(classify_for(&ctrl_alt, Platform::Windows, Some(HotkeyAction::ResizeUp)), Recorded::Set(Hotkey::new(mods::CONTROL | mods::OPTION, "↑")));
        assert_eq!(classify_for(&ctrl_alt, Platform::Windows, Some(HotkeyAction::ToggleRightSidebar)), Recorded::Set(Hotkey::new(mods::COMMAND | mods::OPTION, "↑")));
        let ctrl_k = KeyInput { control: true, ..key("k") };
        assert_eq!(classify(&ctrl_k, Platform::Windows), Recorded::Set(Hotkey::new(mods::CONTROL, "k")));
    }

    #[test]
    fn windows_recorder_refuses_keys_of_the_app_and_the_system() {
        let alt = |k: &str| KeyInput { alt: true, ..key(k) };
        assert_eq!(classify(&alt("n"), Platform::Windows), Recorded::Reject, "Alt+N ist „Neue Session“");
        assert_eq!(classify(&alt("p"), Platform::Windows), Recorded::Reject, "Alt+P ist die Suche");
        assert_eq!(classify(&alt("f4"), Platform::Windows), Recorded::Reject, "Alt+F4 schließt das Fenster");
        assert_eq!(classify(&alt("tab"), Platform::Windows), Recorded::Reject);
        // Auf dem Mac ist ⌥N frei.
        assert!(matches!(classify(&alt("n"), Platform::Mac), Recorded::Set(_)));
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

    /// Tooltip der Taste „Standard“: „Zurück auf <Standardkürzel>“ (`.help("Zurück auf \(action.defaultKey.display)")`).
    #[test]
    fn reset_tooltip_names_the_default_shortcut() {
        let default = HotkeyAction::FocusLeft.default_key();
        let tip = kadrell_i18n::trd!("Zurück auf %@", format(&default, Platform::Windows));
        assert_eq!(tip, "Zurück auf Alt+←");
    }

    #[test]
    fn unknown_keys_are_rejected() {
        let alt_home = KeyInput { alt: true, ..key("home") };
        assert_eq!(classify(&alt_home, Platform::Mac), Recorded::Reject);
    }
}
