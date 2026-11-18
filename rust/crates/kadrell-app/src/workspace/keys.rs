//! Tabelle der Kürzel der Arbeitsfläche, aus den Einstellungen gebaut (`kadrell-core::hotkeys`, im Einstellungen-Dialog
//! belegbar). Ein Tastendruck wird zum `Hotkey` des Kerns (gleiche Zuordnung der Modifier wie beim Aufnehmen im
//! Dialog, siehe `ui::settings::shortcut`) und über `action_for` zur Aktion. Unter Windows kommen zwei Ausnahmen dazu:
//! Alt+Tab und Alt+Esc erreichen keine App, dort gelten andere Standardkürzel, solange der Nutzer nichts speichert,
//! und Strg+Alt+Pfeil gilt als ⌃⌥-Pfeil, weil die Win-Taste das System frisst.

use std::collections::BTreeMap;

use kadrell_core::Settings;
use kadrell_core::hotkeys::{self, Hotkey, HotkeyAction, HotkeyMap, SETTINGS_KEY, mods};

use crate::ui::settings::shortcut::{KeyInput, Platform, core_key, modifier_bits};

/// Standard unter Windows für Aktionen, deren Mac-Standard dort nie ankommt: Alt+Tab wechselt das Fenster,
/// Alt+Esc auch. Nicht auf einem Windows-Rechner verifiziert, dass Strg+Alt+Esc ankommt (Annahme).
pub fn windows_default(action: HotkeyAction) -> Option<Hotkey> {
    match action {
        HotkeyAction::LastSession => Some(Hotkey::new(mods::COMMAND, "⇥")),
        HotkeyAction::CloseFocused => Some(Hotkey::new(mods::COMMAND | mods::OPTION, "Esc")),
        _ => None,
    }
}

/// Belegung nach den Einstellungen, unter Windows mit den ersetzten Standards. Ein gespeichertes Kürzel (auch „ohne“)
/// gewinnt immer.
pub fn effective_map(settings: &Settings, platform: Platform) -> HotkeyMap {
    let saved = settings.string_map(SETTINGS_KEY);
    let mut map = hotkeys::current(settings);
    if platform == Platform::Windows {
        for &action in HotkeyAction::ALL {
            if let Some(key) = windows_default(action).filter(|_| !saved.contains_key(action.raw())) {
                map.insert(action, key);
            }
        }
    }
    map
}

/// Die Tabelle samt dem Stand der Einstellungen, aus dem sie gebaut ist.
pub struct Table {
    map: HotkeyMap,
    source: BTreeMap<String, String>,
    platform: Platform,
}

impl Table {
    pub fn new(settings: &Settings, platform: Platform) -> Table {
        Table { map: effective_map(settings, platform), source: settings.string_map(SETTINGS_KEY), platform }
    }

    /// Baut neu, wenn sich die Kürzel in den Einstellungen geändert haben. Gibt zurück, ob neu gebaut wurde.
    pub fn refresh(&mut self, settings: &Settings) -> bool {
        if self.source == settings.string_map(SETTINGS_KEY) {
            return false;
        }
        *self = Table::new(settings, self.platform);
        true
    }

    /// Die Aktion zu einem Tastendruck, `None` wenn nichts belegt ist.
    pub fn lookup(&self, input: &KeyInput) -> Option<HotkeyAction> {
        let key = core_key(&input.key)?;
        let bits = modifier_bits(input, self.platform);
        let direct = hotkeys::action_for(&self.map, &Hotkey::new(bits, &key));
        direct.or_else(|| self.altgr_alias(bits, &key, input.key.chars().count() > 1))
    }

    /// Windows: Strg+Alt+Pfeil (Tab, Esc, F-Taste) steht für ⌃⌥ + Taste. Nur für benannte Tasten (`named`): Strg+Alt+
    /// Zeichen ist AltGr und gehört dem Terminal.
    fn altgr_alias(&self, bits: u64, key: &str, named: bool) -> Option<HotkeyAction> {
        let both = mods::COMMAND | mods::OPTION;
        if self.platform != Platform::Windows || bits & both != both || !named {
            return None;
        }
        let aliased = (bits & !mods::COMMAND) | mods::CONTROL;
        hotkeys::action_for(&self.map, &Hotkey::new(aliased, key))
    }

    /// Die Belegung einer Aktion (für Hinweise).
    pub fn key_of(&self, action: HotkeyAction) -> Option<&Hotkey> {
        self.map.get(&action)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::settings::shortcut::assign;

    fn input(key: &str) -> KeyInput {
        KeyInput { control: false, alt: false, shift: false, platform: false, key: key.to_string() }
    }

    fn alt(key: &str) -> KeyInput {
        KeyInput { alt: true, ..input(key) }
    }

    fn table(platform: Platform) -> Table {
        Table::new(&Settings::default(), platform)
    }

    #[test]
    fn defaults_dispatch_to_their_actions() {
        let t = table(Platform::Windows);
        assert_eq!(t.lookup(&alt("left")), Some(HotkeyAction::FocusLeft));
        assert_eq!(t.lookup(&alt("z")), Some(HotkeyAction::Zoom));
        assert_eq!(t.lookup(&alt("i")), Some(HotkeyAction::SyncInput));
        assert_eq!(t.lookup(&alt("j")), Some(HotkeyAction::PreviewNext));
        assert_eq!(t.lookup(&alt("k")), Some(HotkeyAction::PreviewPrev));
        assert_eq!(t.lookup(&alt("1")), Some(HotkeyAction::Focus1));
        assert_eq!(t.lookup(&alt("9")), Some(HotkeyAction::Focus9));
        assert_eq!(t.lookup(&alt("n")), Some(HotkeyAction::NextSession));
        assert_eq!(t.lookup(&alt("p")), Some(HotkeyAction::PrevSession));
        let swap = KeyInput { shift: true, ..alt("right") };
        assert_eq!(t.lookup(&swap), Some(HotkeyAction::SwapRight));
        // ⌘ ist unter Windows Strg.
        let ctrl = |key: &str| KeyInput { control: true, ..input(key) };
        assert_eq!(t.lookup(&ctrl("l")), Some(HotkeyAction::NextLayout));
        assert_eq!(t.lookup(&ctrl("1")), Some(HotkeyAction::FocusSidebar));
        assert_eq!(t.lookup(&ctrl("2")), Some(HotkeyAction::FocusWorkspace));
        // Auf dem Mac ist es ⌘.
        let mac = table(Platform::Mac);
        assert_eq!(mac.lookup(&KeyInput { platform: true, ..input("l") }), Some(HotkeyAction::NextLayout));
        assert_eq!(mac.lookup(&ctrl("l")), None);
        assert_eq!(mac.lookup(&alt("tab")), Some(HotkeyAction::LastSession));
    }

    #[test]
    fn plain_keys_and_unknown_combinations_are_not_ours() {
        let t = table(Platform::Windows);
        assert_eq!(t.lookup(&input("z")), None);
        assert_eq!(t.lookup(&input("left")), None);
        assert_eq!(t.lookup(&alt("q")), None);
        assert_eq!(t.lookup(&alt("home")), None);
    }

    #[test]
    fn resize_arrows_come_in_as_ctrl_alt_on_windows() {
        let t = table(Platform::Windows);
        let ctrl_alt = KeyInput { control: true, alt: true, ..input("left") };
        assert_eq!(t.lookup(&ctrl_alt), Some(HotkeyAction::ResizeLeft));
        let split = KeyInput { control: true, alt: true, shift: true, ..input("right") };
        assert_eq!(t.lookup(&split), Some(HotkeyAction::SplitRight));
        // Die Win-Taste (⌃ in der Zuordnung der Einstellungen) bleibt gültig.
        let win = KeyInput { platform: true, alt: true, ..input("up") };
        assert_eq!(t.lookup(&win), Some(HotkeyAction::ResizeUp));
        // Strg+Alt+Zeichen ist AltGr und kein Kürzel.
        let altgr = KeyInput { control: true, alt: true, ..input("n") };
        assert_eq!(t.lookup(&altgr), None);
        // Auf dem Mac ist Ctrl+Alt+Pfeil ohnehin ⌃⌥ + Pfeil, ⌘⌥ + Pfeil gibt es nicht (kein Alias).
        let mac = table(Platform::Mac);
        assert_eq!(mac.lookup(&ctrl_alt), Some(HotkeyAction::ResizeLeft));
        assert_eq!(mac.lookup(&KeyInput { platform: true, alt: true, ..input("left") }), None);
    }

    #[test]
    fn windows_replaces_only_untouched_defaults_that_the_system_eats() {
        let t = table(Platform::Windows);
        let ctrl_tab = KeyInput { control: true, ..input("tab") };
        assert_eq!(t.lookup(&ctrl_tab), Some(HotkeyAction::LastSession));
        assert_eq!(t.lookup(&alt("tab")), None, "Alt+Tab erreicht keine App");
        let esc = KeyInput { control: true, alt: true, ..input("escape") };
        assert_eq!(t.lookup(&esc), Some(HotkeyAction::CloseFocused));
        assert_eq!(t.lookup(&alt("escape")), None);
        // Gespeichert gewinnt: ein eigenes Kürzel gilt, der Ersatz nicht mehr.
        let mut saved = Settings::default();
        assign(&mut saved, HotkeyAction::LastSession, Some(Hotkey::new(mods::OPTION, "y")));
        let t = Table::new(&saved, Platform::Windows);
        assert_eq!(t.lookup(&alt("y")), Some(HotkeyAction::LastSession));
        assert_eq!(t.lookup(&ctrl_tab), None);
        // „Ohne Kürzel“ gewinnt auch.
        assign(&mut saved, HotkeyAction::LastSession, None);
        assert_eq!(Table::new(&saved, Platform::Windows).lookup(&ctrl_tab), None);
        // Das Ersatzkürzel kollidiert mit keinem Standard.
        for &a in HotkeyAction::ALL {
            for replaced in [HotkeyAction::LastSession, HotkeyAction::CloseFocused] {
                assert_ne!(a.default_key(), windows_default(replaced).unwrap(), "{a:?} belegt es schon");
            }
        }
    }

    #[test]
    fn rebinding_in_the_settings_changes_the_dispatch_without_restart() {
        let mut settings = Settings::default();
        let mut t = Table::new(&settings, Platform::Windows);
        assert!(!t.refresh(&settings), "nichts geändert, nichts neu gebaut");
        assign(&mut settings, HotkeyAction::Zoom, Some(Hotkey::new(mods::OPTION, "y")));
        assert!(t.refresh(&settings));
        assert_eq!(t.lookup(&alt("y")), Some(HotkeyAction::Zoom));
        assert_eq!(t.lookup(&alt("z")), None);
        assign(&mut settings, HotkeyAction::Zoom, None);
        assert!(t.refresh(&settings));
        assert_eq!(t.lookup(&alt("y")), None);
        assert_eq!(t.key_of(HotkeyAction::Zoom), None);
    }

    #[test]
    fn alt_f4_is_never_a_workspace_key_unless_the_user_binds_it() {
        let t = table(Platform::Windows);
        assert_eq!(t.lookup(&alt("f4")), Some(HotkeyAction::ShowExtensions));
        // Der Standard von ShowExtensions ist ⌥F4: die Tabelle kennt ihn, die Arbeitsfläche führt ihn nicht aus
        // (`actions::handler_for` hat keinen Eintrag), das Fenster behält Alt+F4.
        assert!(crate::workspace::actions::handler_for(HotkeyAction::ShowExtensions).is_none());
    }
}
