//! Tastatur im Baum (⌘1 / Strg+1): ↑↓ zeigt die nächste Session rechts, ohne Tasten an Claude zu schicken, ← klappt die
//! Gruppe zu, → auf, ⇧F10 öffnet das Kontextmenü, ⏎ und Leertaste übernehmen, Entf schließt. Das Kürzel für
//! „Taste an die Fokus-Session geben“ ist auf dem Mac ⌃⏎ (`keyDown` des Baums). Unter Windows ist Strg+⏎ schon
//! „Neue Session im selben Ordner“ und Win+⏎ startet den Sprachausgabe-Dialog des Systems: dort gilt Alt+⏎.
//! Port von `SidebarView.keyDown`.

use kadrell_core::hotkeys::mods;

use crate::ui::settings::shortcut::{KeyInput, Platform, modifier_bits};

/// Was eine Taste im Baum auslöst.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeKey {
    /// Eine Session weiter (+1) oder zurück (-1), zeigt sie rechts.
    Step(isize),
    Collapse,
    Expand,
    ContextMenu,
    /// Die Tastatur geht zurück an die Fokus-Session, ⏎ geht mit.
    PassToSession,
    /// ⏎ oder Leertaste: die Session übernehmen, Tastatur zu Claude.
    Open,
    /// Entf: Session schließen.
    Close,
    /// ⌥⌘↑/↓: die Session im Baum verschieben.
    Move(isize),
}

fn pass_modifier(platform: Platform) -> u64 {
    match platform {
        Platform::Mac => mods::CONTROL,
        Platform::Windows => mods::OPTION,
    }
}

pub fn classify(input: &KeyInput, platform: Platform) -> Option<TreeKey> {
    let bits = modifier_bits(input, platform);
    let key = input.key.as_str();
    if bits == mods::OPTION | mods::COMMAND {
        return match key {
            "down" => Some(TreeKey::Move(1)),
            "up" => Some(TreeKey::Move(-1)),
            _ => None,
        };
    }
    if key == "enter" && bits == pass_modifier(platform) {
        return Some(TreeKey::PassToSession);
    }
    if key == "f10" && bits == mods::SHIFT {
        return Some(TreeKey::ContextMenu);
    }
    if bits != 0 {
        return None;
    }
    match key {
        "down" => Some(TreeKey::Step(1)),
        "up" => Some(TreeKey::Step(-1)),
        "left" => Some(TreeKey::Collapse),
        "right" => Some(TreeKey::Expand),
        "enter" | "space" => Some(TreeKey::Open),
        "delete" | "backspace" => Some(TreeKey::Close),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(key: &str) -> KeyInput {
        KeyInput { control: false, alt: false, shift: false, platform: false, key: key.to_string() }
    }

    #[test]
    fn plain_keys_drive_the_tree() {
        let c = |k: &str| classify(&key(k), Platform::Windows);
        assert_eq!(c("down"), Some(TreeKey::Step(1)));
        assert_eq!(c("up"), Some(TreeKey::Step(-1)));
        assert_eq!(c("left"), Some(TreeKey::Collapse));
        assert_eq!(c("right"), Some(TreeKey::Expand));
        assert_eq!(c("enter"), Some(TreeKey::Open));
        assert_eq!(c("space"), Some(TreeKey::Open));
        assert_eq!(c("delete"), Some(TreeKey::Close));
        assert_eq!(c("a"), None, "Buchstaben gehen nirgends hin");
    }

    #[test]
    fn shift_f10_opens_the_context_menu() {
        let f10 = KeyInput { shift: true, ..key("f10") };
        assert_eq!(classify(&f10, Platform::Windows), Some(TreeKey::ContextMenu));
        assert_eq!(classify(&key("f10"), Platform::Windows), None);
    }

    #[test]
    fn passing_the_key_to_the_session_uses_ctrl_return_on_the_mac_and_alt_return_on_windows() {
        let ctrl = KeyInput { control: true, ..key("enter") };
        assert_eq!(classify(&ctrl, Platform::Mac), Some(TreeKey::PassToSession));
        assert_eq!(classify(&ctrl, Platform::Windows), None, "Strg+⏎ ist ⌘⏎: neue Session im selben Ordner");
        let alt = KeyInput { alt: true, ..key("enter") };
        assert_eq!(classify(&alt, Platform::Windows), Some(TreeKey::PassToSession));
        assert_eq!(classify(&alt, Platform::Mac), None);
    }

    #[test]
    fn option_command_arrows_move_the_session() {
        let win = KeyInput { control: true, alt: true, ..key("down") };
        assert_eq!(classify(&win, Platform::Windows), Some(TreeKey::Move(1)));
        let mac = KeyInput { platform: true, alt: true, ..key("up") };
        assert_eq!(classify(&mac, Platform::Mac), Some(TreeKey::Move(-1)));
    }

    #[test]
    fn modified_arrows_are_not_tree_steps() {
        let shift = KeyInput { shift: true, ..key("down") };
        assert_eq!(classify(&shift, Platform::Windows), None);
    }
}
