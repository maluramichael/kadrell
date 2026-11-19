//! Tabelle der Kürzel der Arbeitsfläche, aus den Einstellungen gebaut (`kadrell-core::hotkeys`, im Einstellungen-Dialog
//! belegbar). Auf dem Mac wird ein Tastendruck zum `Hotkey` des Kerns und über `action_for` zur Aktion. Unter Windows
//! liegt ⌘ auf Alt (`crate::keys`): ⌘ und ⌥ sind dort beide Alt, die Tabelle vergleicht deshalb die Windows-Tasten
//! (Strg, Alt, Umschalt, Taste). Bei gleichem Kürzel gewinnt ein gespeichertes vor dem Standard, dann das ⌘-Kürzel.
//! Wo ein ⌥-Standard mit einem ⌘-Kürzel zusammenfällt oder das System die Taste frisst (Alt+Tab, Alt+Esc, Alt+F4),
//! gilt unter Windows ein eigener Standard (`windows_default`), solange der Nutzer nichts speichert.

use std::collections::{BTreeMap, HashMap};

use kadrell_core::Settings;
use kadrell_core::hotkeys::{self, Hotkey, HotkeyAction, HotkeyMap, SETTINGS_KEY, mods};

use crate::keys::{Physical, gpui_key, physical_of};
use crate::ui::settings::shortcut::{KeyInput, Platform, core_key, modifier_bits};

/// Standard unter Windows für Aktionen, deren Mac-Standard dort mit einem ⌘-Kürzel zusammenfällt (⌥N/⌥P gegen ⌘N/⌘P,
/// ⌥1 bis ⌥3 gegen ⌘1 bis ⌘3, ⌥O gegen ⌘O im Dialog) oder dort nie ankommt (Alt+Tab, Alt+Esc, Alt+F4). Alt+Umschalt
/// ist frei, Strg+Alt vermeiden wir wegen AltGr. Nicht auf einem Windows-Rechner verifiziert, dass Strg+Alt+Esc und
/// Alt+Umschalt+Ziffer ankommen (Annahme).
pub fn windows_default(action: HotkeyAction) -> Option<Hotkey> {
    let alt_shift = |key: &str| Some(Hotkey::new(mods::OPTION | mods::SHIFT, key));
    if let Some(i) = action.tile_index() {
        return alt_shift(&(i + 1).to_string());
    }
    match action {
        HotkeyAction::NextSession => alt_shift("j"),
        HotkeyAction::PrevSession => alt_shift("k"),
        HotkeyAction::LastSession => alt_shift("l"),
        HotkeyAction::CycleSort => alt_shift("o"),
        HotkeyAction::ShowExtensions => alt_shift("e"),
        HotkeyAction::CloseFocused => Some(Hotkey::new(mods::COMMAND | mods::OPTION, "Esc")),
        _ => None,
    }
}

/// Standard einer Aktion auf dieser Plattform.
pub fn platform_default(action: HotkeyAction, platform: Platform) -> Hotkey {
    match platform {
        Platform::Windows => windows_default(action).unwrap_or_else(|| action.default_key()),
        Platform::Mac => action.default_key(),
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
    /// Nur Windows: Aktion je Windows-Taste.
    physical: HashMap<(Physical, String), HotkeyAction>,
}

impl Table {
    pub fn new(settings: &Settings, platform: Platform) -> Table {
        let map = effective_map(settings, platform);
        let source = settings.string_map(SETTINGS_KEY);
        let physical = if platform == Platform::Windows { physical_table(&map, &source) } else { HashMap::new() };
        Table { map, source, platform, physical }
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
        if self.platform == Platform::Windows {
            let p = Physical::from_input(input.control, input.platform, input.alt, input.shift);
            return self.physical.get(&(p, gpui_key(&key)?)).copied();
        }
        hotkeys::action_for(&self.map, &Hotkey::new(modifier_bits(input, self.platform), &key))
    }

    /// Die Belegung einer Aktion (für Hinweise).
    pub fn key_of(&self, action: HotkeyAction) -> Option<&Hotkey> {
        self.map.get(&action)
    }
}

/// Aktion je Windows-Taste. Bei gleicher Taste gewinnt, was später eingetragen wird: zuerst Standards, dann gespeicherte
/// Kürzel, innerhalb davon ⌘-Kürzel vor ⌥-Kürzeln (daher zuletzt), ganz am Ende die Reihenfolge der Aktionen.
fn physical_table(map: &HotkeyMap, saved: &BTreeMap<String, String>) -> HashMap<(Physical, String), HotkeyAction> {
    let mut entries: Vec<(&HotkeyAction, &Hotkey)> = map.iter().collect();
    entries.sort_by_key(|(a, h)| (saved.contains_key(a.raw()), h.has(mods::COMMAND) && !h.has(mods::OPTION), **a));
    entries.into_iter().filter_map(|(a, h)| Some((physical_of(h)?, *a))).collect()
}

/// Gehört die Windows-Taste von `h` schon einer anderen Aktion mit anderem gespeicherten Kürzel (⌘ gegen ⌥, beides
/// Alt)? Gleiche gespeicherte Kürzel regelt `shortcut::assign`.
pub fn clashes(settings: &Settings, action: HotkeyAction, h: &Hotkey) -> bool {
    let Some(mine) = physical_of(h) else { return false };
    effective_map(settings, Platform::Windows)
        .iter()
        .any(|(other, k)| *other != action && k != h && physical_of(k).as_ref() == Some(&mine))
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

    fn alt_shift(key: &str) -> KeyInput {
        KeyInput { alt: true, shift: true, ..input(key) }
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
        let swap = KeyInput { shift: true, ..alt("right") };
        assert_eq!(t.lookup(&swap), Some(HotkeyAction::SwapRight));
        // ⌘ ist unter Windows Alt.
        assert_eq!(t.lookup(&alt("l")), Some(HotkeyAction::NextLayout));
        assert_eq!(t.lookup(&alt("1")), Some(HotkeyAction::FocusSidebar));
        assert_eq!(t.lookup(&alt("2")), Some(HotkeyAction::FocusWorkspace));
        assert_eq!(t.lookup(&alt("3")), Some(HotkeyAction::FocusRightSidebar));
        // Strg allein gehört dem Terminal.
        assert_eq!(t.lookup(&KeyInput { control: true, ..input("l") }), None);
        // Auf dem Mac ist es ⌘.
        let mac = table(Platform::Mac);
        assert_eq!(mac.lookup(&KeyInput { platform: true, ..input("l") }), Some(HotkeyAction::NextLayout));
        assert_eq!(mac.lookup(&KeyInput { control: true, ..input("l") }), None);
        assert_eq!(mac.lookup(&alt("tab")), Some(HotkeyAction::LastSession));
        assert_eq!(mac.lookup(&alt("n")), Some(HotkeyAction::NextSession));
    }

    #[test]
    fn alt_actions_that_clash_with_command_shortcuts_have_their_own_windows_keys() {
        let t = table(Platform::Windows);
        // Alt+N und Alt+P gehören „Neue Session“ und „Suche“ (feste Bindungen), nicht dem Tabellen-Eintrag.
        assert_eq!(t.lookup(&alt("n")), None);
        assert_eq!(t.lookup(&alt("p")), None);
        assert_eq!(t.lookup(&alt_shift("j")), Some(HotkeyAction::NextSession));
        assert_eq!(t.lookup(&alt_shift("k")), Some(HotkeyAction::PrevSession));
        assert_eq!(t.lookup(&alt_shift("1")), Some(HotkeyAction::Focus1));
        assert_eq!(t.lookup(&alt_shift("9")), Some(HotkeyAction::Focus9));
        assert_eq!(t.lookup(&alt_shift("o")), Some(HotkeyAction::CycleSort));
        assert_eq!(t.lookup(&alt("9")), None, "Alt+9 gehört keiner Aktion mehr, alle Kacheln liegen auf Alt+Umschalt");
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
        // Die Win-Taste zählt wie Strg und bleibt gültig.
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
        assert_eq!(t.lookup(&alt_shift("l")), Some(HotkeyAction::LastSession));
        assert_eq!(t.lookup(&alt("tab")), None, "Alt+Tab erreicht keine App");
        let esc = KeyInput { control: true, alt: true, ..input("escape") };
        assert_eq!(t.lookup(&esc), Some(HotkeyAction::CloseFocused));
        assert_eq!(t.lookup(&alt("escape")), None);
        // Gespeichert gewinnt: ein eigenes Kürzel gilt, der Ersatz nicht mehr.
        let mut saved = Settings::default();
        assign(&mut saved, HotkeyAction::LastSession, Some(Hotkey::new(mods::OPTION, "y")));
        let t = Table::new(&saved, Platform::Windows);
        assert_eq!(t.lookup(&alt("y")), Some(HotkeyAction::LastSession));
        assert_eq!(t.lookup(&alt_shift("l")), None);
        // „Ohne Kürzel“ gewinnt auch.
        assign(&mut saved, HotkeyAction::LastSession, None);
        assert_eq!(Table::new(&saved, Platform::Windows).lookup(&alt_shift("l")), None);
    }

    #[test]
    fn a_saved_command_shortcut_beats_an_alt_one_on_the_same_key() {
        // ⌘Y für Zoom gespeichert und ⌥Y für Sync: beides ist Alt+Y, das ⌘-Kürzel gewinnt.
        let mut s = Settings::default();
        assign(&mut s, HotkeyAction::Zoom, Some(Hotkey::new(mods::COMMAND, "y")));
        assign(&mut s, HotkeyAction::SyncInput, Some(Hotkey::new(mods::OPTION, "y")));
        assert_eq!(Table::new(&s, Platform::Windows).lookup(&alt("y")), Some(HotkeyAction::Zoom));
    }

    #[test]
    fn clashes_name_alt_keys_that_two_actions_would_share() {
        let s = Settings::default();
        assert!(clashes(&s, HotkeyAction::Zoom, &Hotkey::new(mods::OPTION, "l")), "Alt+L ist ⌘L (Layout)");
        assert!(!clashes(&s, HotkeyAction::NextLayout, &Hotkey::new(mods::COMMAND, "l")), "die eigene Taste nicht");
        assert!(!clashes(&s, HotkeyAction::Zoom, &Hotkey::new(mods::OPTION, "y")));
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
        assert_eq!(t.lookup(&alt("f4")), None, "das Fenster behält Alt+F4");
        assert_eq!(t.lookup(&alt_shift("e")), Some(HotkeyAction::ShowExtensions));
        // Die Arbeitsfläche führt ShowExtensions nicht aus (`actions::handler_for` hat keinen Eintrag).
        assert!(crate::workspace::actions::handler_for(HotkeyAction::ShowExtensions).is_none());
        // Auf dem Mac bleibt ⌥F4.
        assert_eq!(table(Platform::Mac).lookup(&alt("f4")), Some(HotkeyAction::ShowExtensions));
    }

    #[test]
    fn windows_defaults_never_use_the_keys_of_other_defaults() {
        for &replaced in HotkeyAction::ALL {
            let Some(own) = windows_default(replaced) else { continue };
            for &a in HotkeyAction::ALL.iter().filter(|a| **a != replaced) {
                assert_ne!(platform_default(a, Platform::Windows), own, "{a:?} belegt es schon");
                assert_ne!(a.default_key(), own, "{a:?} belegt es schon (Mac)");
            }
        }
    }
}
