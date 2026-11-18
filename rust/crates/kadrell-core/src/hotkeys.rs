//! Frei belegbare Kürzel: Aktionen, Standardbelegung, Bindungsmodell und Speicherung in den Einstellungen.
//! Quelle: `App/Hotkeys.swift` (ohne die Ereignisbehandlung und die lokalisierten Hilfetexte).
//!
//! Die Modifier stehen als `NSEvent.ModifierFlags`-Bits in der Datei (`"524288|←"` für ⌥←), damit eine
//! Einstellungsdatei auf beiden Seiten dasselbe heißt. Defaults nach Michaels tmux-Config (M-Pfeile, M-z, M-1…9,
//! M-Tab, M-Enter); F-Tasten nur mit ⌥: blanke F1…F4 brauchen Programme im Terminal selbst (htop, mc).

use crate::settings::Settings;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};

/// Modifier-Bits wie `NSEvent.ModifierFlags.rawValue`.
pub mod mods {
    pub const SHIFT: u64 = 1 << 17;
    pub const CONTROL: u64 = 1 << 18;
    pub const OPTION: u64 = 1 << 19;
    pub const COMMAND: u64 = 1 << 20;
    /// Nur diese vier zählen.
    pub const MASK: u64 = SHIFT | CONTROL | OPTION | COMMAND;
}

use mods::{COMMAND, CONTROL, OPTION, SHIFT};

macro_rules! hotkey_actions {
    ($($var:ident = $raw:literal),+ $(,)?) => {
        /// Eine Aktion, der ein Kürzel zugeordnet werden kann.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum HotkeyAction { $($var),+ }

        impl HotkeyAction {
            /// Alle Aktionen in der Reihenfolge der Mac-App.
            pub const ALL: &'static [HotkeyAction] = &[$(HotkeyAction::$var),+];

            /// Name in der Einstellungsdatei (`rawValue`).
            pub fn raw(self) -> &'static str {
                match self { $(HotkeyAction::$var => $raw),+ }
            }

            pub fn from_raw(raw: &str) -> Option<HotkeyAction> {
                match raw { $($raw => Some(HotkeyAction::$var),)+ _ => None }
            }
        }
    };
}

hotkey_actions! {
    FocusLeft = "focusLeft", FocusRight = "focusRight", FocusUp = "focusUp", FocusDown = "focusDown",
    SwapLeft = "swapLeft", SwapRight = "swapRight", SwapUp = "swapUp", SwapDown = "swapDown",
    ResizeLeft = "resizeLeft", ResizeRight = "resizeRight", ResizeUp = "resizeUp", ResizeDown = "resizeDown",
    SplitRight = "splitRight", SplitDown = "splitDown",
    NextSession = "nextSession", PrevSession = "prevSession", LastSession = "lastSession",
    PreviewNext = "previewNext", PreviewPrev = "previewPrev",
    NextWaiting = "nextWaiting",
    Focus1 = "focus1", Focus2 = "focus2", Focus3 = "focus3", Focus4 = "focus4", Focus5 = "focus5",
    Focus6 = "focus6", Focus7 = "focus7", Focus8 = "focus8", Focus9 = "focus9",
    Zoom = "zoom", NextLayout = "nextLayout", CloseFocused = "closeFocused", OpenEditor = "openEditor",
    RenameSession = "renameSession", SyncInput = "syncInput",
    FocusSidebar = "focusSidebar", FocusWorkspace = "focusWorkspace", CycleSort = "cycleSort", ToggleGrouping = "toggleGrouping",
    FocusRightSidebar = "focusRightSidebar", ToggleRightSidebar = "toggleRightSidebar",
    QuickStart = "quickStart", SearchHistory = "searchHistory",
    ShowHelp = "showHelp", ShowStats = "showStats", ShowExtensions = "showExtensions",
    Restart = "restart",
}

impl HotkeyAction {
    /// 0-basiert für `focus1` bis `focus9`.
    pub fn tile_index(self) -> Option<usize> {
        self.raw().strip_prefix("focus").and_then(|n| n.parse::<usize>().ok()).map(|n| n - 1)
    }

    /// Deutscher Titel, zugleich der Schlüssel in `Localizable.xcstrings`. Für „Kachel 1 bis 9“ der Text mit
    /// der Nummer (`Kachel 3`); die Übersetzung steht unter `Kachel %lld`.
    pub fn title_key(self) -> String {
        if let Some(i) = self.tile_index() {
            return format!("Kachel {}", i + 1);
        }
        self.title_text().to_string()
    }

    fn title_text(self) -> &'static str {
        use HotkeyAction::*;
        match self {
            FocusLeft => "Fokus links",
            FocusRight => "Fokus rechts",
            FocusUp => "Fokus oben",
            FocusDown => "Fokus unten",
            SwapLeft => "Kachel nach links tauschen",
            SwapRight => "Kachel nach rechts tauschen",
            SwapUp => "Kachel nach oben tauschen",
            SwapDown => "Kachel nach unten tauschen",
            ResizeLeft => "Trennlinie nach links",
            ResizeRight => "Trennlinie nach rechts",
            ResizeUp => "Trennlinie nach oben",
            ResizeDown => "Trennlinie nach unten",
            SplitRight => "Frei: nächste Kachel rechts",
            SplitDown => "Frei: nächste Kachel unten",
            NextSession => "Nächste Kachel",
            PrevSession => "Vorige Kachel",
            LastSession => "Zuletzt fokussierte Kachel",
            PreviewNext => "Vorschau: nächste Session im Baum",
            PreviewPrev => "Vorschau: vorige Session im Baum",
            NextWaiting => "Nächste wartende Session",
            Zoom => "Zoom: Fokus-Kachel allein",
            NextLayout => "Layout wechseln: Grid → Haupt + Spalte → Spirale → Frei → Scrollen → Stack",
            FocusSidebar => "Baum: Tastatur hierher, ↑↓ wählt Session",
            FocusWorkspace => "Arbeitsfläche: Tastatur an Claude",
            FocusRightSidebar => "Rechte Sidebar: Tastatur hierher, ↑↓ wählt Eintrag, ←→ Tab",
            ToggleRightSidebar => "Rechte Sidebar ein/aus",
            OpenEditor => "Ordner im externen Editor öffnen",
            RenameSession => "Session umbenennen",
            CycleSort => "Baum sortieren: aus → A–Z → Status",
            ToggleGrouping => "Baum nach Projekt gruppieren",
            SyncInput => "Sync: Eingabe an alle Kacheln",
            QuickStart => "Schnellstart: Idee rein, Session raus",
            SearchHistory => "Alle Claude-Sessions durchsuchen, auch beendete",
            ShowHelp => "Hilfe mit allen Kürzeln",
            ShowStats => "Statistik",
            ShowExtensions => "Extensions",
            Restart => "Kadrell neu starten",
            _ => "Fokus-Kachel schließen",
        }
    }

    /// Standardbelegung. `Restart` ist bewusst unbelegt (leerer Key), in den Einstellungen belegbar.
    pub fn default_key(self) -> Hotkey {
        use HotkeyAction::*;
        if let Some(i) = self.tile_index() {
            return Hotkey::new(OPTION, &(i + 1).to_string());
        }
        let (m, key) = match self {
            FocusLeft => (OPTION, "←"),
            FocusRight => (OPTION, "→"),
            FocusUp => (OPTION, "↑"),
            FocusDown => (OPTION, "↓"),
            SwapLeft => (OPTION | SHIFT, "←"),
            SwapRight => (OPTION | SHIFT, "→"),
            SwapUp => (OPTION | SHIFT, "↑"),
            SwapDown => (OPTION | SHIFT, "↓"),
            ResizeLeft => (CONTROL | OPTION, "←"),
            ResizeRight => (CONTROL | OPTION, "→"),
            ResizeUp => (CONTROL | OPTION, "↑"),
            ResizeDown => (CONTROL | OPTION, "↓"),
            SplitRight => (CONTROL | OPTION | SHIFT, "→"),
            SplitDown => (CONTROL | OPTION | SHIFT, "↓"),
            NextSession => (OPTION, "n"),
            PrevSession => (OPTION, "p"),
            LastSession => (OPTION, "⇥"),
            PreviewNext => (OPTION, "j"),
            PreviewPrev => (OPTION, "k"),
            NextWaiting => (OPTION, "w"),
            Zoom => (OPTION, "z"),
            NextLayout => (COMMAND, "l"),
            FocusSidebar => (COMMAND, "1"),
            FocusWorkspace => (COMMAND, "2"),
            FocusRightSidebar => (COMMAND, "3"),
            ToggleRightSidebar => (COMMAND | OPTION, "b"),
            OpenEditor => (OPTION, "e"),
            RenameSession => (OPTION, "F2"),
            CycleSort => (OPTION, "o"),
            ToggleGrouping => (OPTION, "g"),
            SyncInput => (OPTION, "i"),
            QuickStart => (COMMAND | SHIFT, "i"), // ⌘⇧N ist schon „Remote verbinden“
            SearchHistory => (COMMAND | SHIFT, "h"),
            ShowHelp => (OPTION, "F1"),
            ShowStats => (OPTION, "F3"),
            ShowExtensions => (OPTION, "F4"),
            Restart => (0, ""),
            _ => (OPTION, "Esc"),
        };
        Hotkey::new(m, key)
    }
}

/// Eine Sondertaste: Mac-Tastencode (US-Layout-Position), Name im Kürzel, Zeichen für ein Menü-Kürzel.
struct Special {
    code: u16,
    name: &'static str,
    menu: &'static str,
}

/// Pfeile, Return, Tab, Esc, Leertaste, Backspace und F1 bis F12. Die Menü-Zeichen der Pfeile und F-Tasten sind die
/// privaten Unicode-Zeichen von AppKit (`NSLeftArrowFunctionKey` ... `NSF1FunctionKey`).
const SPECIALS: [Special; 21] = [
    Special { code: 123, name: "←", menu: "\u{F702}" },
    Special { code: 124, name: "→", menu: "\u{F703}" },
    Special { code: 125, name: "↓", menu: "\u{F701}" },
    Special { code: 126, name: "↑", menu: "\u{F700}" },
    Special { code: 36, name: "⏎", menu: "\r" },
    Special { code: 48, name: "⇥", menu: "\t" },
    Special { code: 53, name: "Esc", menu: "\u{1b}" },
    Special { code: 49, name: "Space", menu: " " },
    Special { code: 51, name: "⌫", menu: "\u{8}" },
    Special { code: 122, name: "F1", menu: "\u{F704}" },
    Special { code: 120, name: "F2", menu: "\u{F705}" },
    Special { code: 99, name: "F3", menu: "\u{F706}" },
    Special { code: 118, name: "F4", menu: "\u{F707}" },
    Special { code: 96, name: "F5", menu: "\u{F708}" },
    Special { code: 97, name: "F6", menu: "\u{F709}" },
    Special { code: 98, name: "F7", menu: "\u{F70A}" },
    Special { code: 100, name: "F8", menu: "\u{F70B}" },
    Special { code: 101, name: "F9", menu: "\u{F70C}" },
    Special { code: 109, name: "F10", menu: "\u{F70D}" },
    Special { code: 103, name: "F11", menu: "\u{F70E}" },
    Special { code: 111, name: "F12", menu: "\u{F70F}" },
];

/// Ein Kürzel: Modifier und Taste.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Hotkey {
    /// `NSEvent.ModifierFlags.rawValue`, nur ⌃⌥⇧⌘.
    pub mods: u64,
    /// Zeichen ohne Modifier (klein) oder Name einer Sondertaste (`←`, `F5`). Zeichen statt Tastencode: ⌥Z liegt
    /// auf QWERTZ woanders als auf US.
    pub key: String,
}

impl Hotkey {
    pub fn new(mods: u64, key: &str) -> Hotkey {
        Hotkey { mods: mods & mods::MASK, key: key.to_string() }
    }

    /// Kürzel aus einem Tastendruck: Sondertasten über den Mac-Tastencode, sonst das Zeichen ohne Modifier,
    /// kleingeschrieben. `None` ohne beides.
    pub fn from_event(mods: u64, mac_keycode: Option<u16>, chars_ignoring_modifiers: &str) -> Option<Hotkey> {
        if let Some(special) = mac_keycode.and_then(|c| SPECIALS.iter().find(|s| s.code == c)) {
            return Some(Hotkey::new(mods, special.name));
        }
        let c = chars_ignoring_modifiers.to_lowercase();
        if c.is_empty() { None } else { Some(Hotkey::new(mods, &c)) }
    }

    /// Aus `"<mods>|<key>"`. Wie `split(separator:maxSplits:1)` auf dem Mac, das leere Teile weglässt: ohne Taste
    /// oder mit unlesbarer Zahl ist das Ergebnis `None`.
    pub fn from_string(s: &str) -> Option<Hotkey> {
        let rest = s.trim_start_matches('|');
        let (head, tail) = rest.split_once('|')?;
        if head.is_empty() || tail.is_empty() {
            return None;
        }
        Some(Hotkey::new(head.parse::<u64>().ok()?, tail))
    }

    /// Gegenstück zu [`Hotkey::from_string`].
    pub fn to_string_raw(&self) -> String {
        format!("{}|{}", self.mods, self.key)
    }

    pub fn has(&self, flag: u64) -> bool {
        self.mods & flag != 0
    }

    /// Anzeige wie `⌥⇧←`.
    pub fn display(&self) -> String {
        let mut out = String::new();
        for (flag, sign) in [(CONTROL, "⌃"), (OPTION, "⌥"), (SHIFT, "⇧"), (COMMAND, "⌘")] {
            if self.has(flag) {
                out.push_str(sign);
            }
        }
        out + &self.key.to_uppercase()
    }

    /// Zeichen für ein Menü-Kürzel (Sondertasten in der Schreibweise von AppKit).
    pub fn menu_equivalent(&self) -> String {
        SPECIALS.iter().find(|s| s.name == self.key).map_or_else(|| self.key.clone(), |s| s.menu.to_string())
    }

    /// Ohne ⌘⌥⌃ würde das Kürzel normales Tippen im Terminal schlucken; F-Tasten sind die Ausnahme.
    pub fn is_usable(&self) -> bool {
        self.mods & (COMMAND | OPTION | CONTROL) != 0 || (self.key.starts_with('F') && self.key.chars().count() > 1)
    }
}

/// Belegung aller Aktionen. Eine fehlende Aktion hat bewusst kein Kürzel.
pub type HotkeyMap = HashMap<HotkeyAction, Hotkey>;

/// Schlüssel in den Einstellungen: ein Objekt Aktion zu `"<mods>|<key>"`, nur Abweichungen vom Standard.
pub const SETTINGS_KEY: &str = "hotkeys";

/// Aktuelle Belegung: Standard, überschrieben von dem, was in den Einstellungen steht. Ein leerer oder
/// unlesbarer Eintrag heißt „ohne Kürzel“.
pub fn current(settings: &Settings) -> HotkeyMap {
    let saved = settings.string_map(SETTINGS_KEY);
    let mut out = HotkeyMap::new();
    for &action in HotkeyAction::ALL {
        let key = match saved.get(action.raw()) {
            Some(s) => Hotkey::from_string(s),
            None => Some(action.default_key()),
        };
        if let Some(k) = key {
            out.insert(action, k);
        }
    }
    out
}

/// Speichert die Belegung. Gespeichert werden nur Abweichungen vom Standard; `""` heißt bewusst ohne Kürzel.
pub fn store(settings: &mut Settings, map: &HotkeyMap) {
    let mut saved: BTreeMap<&str, String> = BTreeMap::new();
    for &action in HotkeyAction::ALL {
        let value = map.get(&action);
        if value != Some(&action.default_key()) {
            saved.insert(action.raw(), value.map(Hotkey::to_string_raw).unwrap_or_default());
        }
    }
    settings.set(SETTINGS_KEY, serde_json::json!(saved));
}

/// Anzeige der Belegung einer Aktion, `None` ohne Kürzel (für Hinweistexte wie „⌥F1 Hilfe“).
pub fn display(map: &HotkeyMap, action: HotkeyAction) -> Option<String> {
    map.get(&action).filter(|k| !k.key.is_empty()).map(Hotkey::display)
}

/// Die Aktion zu einem Tastendruck. Bei doppelter Belegung gewinnt die erste in der Reihenfolge von
/// [`HotkeyAction::ALL`].
pub fn action_for(map: &HotkeyMap, key: &Hotkey) -> Option<HotkeyAction> {
    HotkeyAction::ALL.iter().copied().find(|a| map.get(a) == Some(key))
}

/// Ist der Wert in den Einstellungen ein Objekt? Für Aufrufer, die rohe Einstellungen prüfen.
pub fn has_overrides(settings: &Settings) -> bool {
    matches!(settings.get(SETTINGS_KEY), Some(Value::Object(m)) if !m.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn defaults_are_unique_and_usable() {
        // Absichtlich unbelegte Defaults (leerer Key, zum Beispiel Neu starten) zählen nicht.
        let keys: Vec<Hotkey> = HotkeyAction::ALL.iter().map(|a| a.default_key()).filter(|k| !k.key.is_empty()).collect();
        assert_eq!(keys.iter().collect::<HashSet<_>>().len(), keys.len());
        for k in &keys {
            assert!(k.is_usable(), "{}", k.display());
        }
        assert_eq!(HotkeyAction::ALL.len(), 47);
    }

    #[test]
    fn raw_names_roundtrip() {
        for &a in HotkeyAction::ALL {
            assert_eq!(HotkeyAction::from_raw(a.raw()), Some(a));
        }
        assert_eq!(HotkeyAction::from_raw("nope"), None);
    }

    #[test]
    fn string_roundtrip_and_display() {
        let k = Hotkey::new(OPTION | SHIFT, "←");
        assert_eq!(Hotkey::from_string(&k.to_string_raw()), Some(k.clone()));
        assert_eq!(Hotkey::from_string(&format!("{OPTION}||")), Some(Hotkey::new(OPTION, "|")));
        assert_eq!(Hotkey::from_string(""), None);
        assert_eq!(Hotkey::from_string("0|"), None, "ohne Taste");
        assert_eq!(Hotkey::from_string("abc|x"), None);
        assert_eq!(Hotkey::from_string("524288"), None);
        assert_eq!(k.display(), "⌥⇧←");
        assert_eq!(Hotkey::new(COMMAND, "Esc").menu_equivalent(), "\u{1b}");
        assert_eq!(Hotkey::new(COMMAND, "a").menu_equivalent(), "a");
        assert_eq!(Hotkey::new(OPTION, "←").to_string_raw(), "524288|←", "Bits wie NSEvent.ModifierFlags");
        assert_eq!(Hotkey::new(1 << 30 | OPTION, "a").mods, OPTION, "fremde Bits fallen weg");
    }

    #[test]
    fn usable() {
        assert!(!Hotkey::new(0, "a").is_usable());
        assert!(!Hotkey::new(SHIFT, "a").is_usable());
        assert!(Hotkey::new(0, "F5").is_usable());
        assert!(Hotkey::new(CONTROL, "a").is_usable());
        assert!(!Hotkey::new(0, "F").is_usable());
    }

    #[test]
    fn event_matching() {
        // ⌥← mit numericPad und function: nur die vier Modifier zählen.
        let left = Hotkey::from_event(OPTION | (1 << 21) | (1 << 23), Some(123), "").unwrap();
        assert_eq!(left, Hotkey::new(OPTION, "←"));
        // ⌥Z liefert "Ω" als Zeichen, `charactersIgnoringModifiers` ist "z".
        let z = Hotkey::from_event(OPTION, Some(6), "z").unwrap();
        assert_eq!(z, HotkeyAction::Zoom.default_key());
        assert_eq!(Hotkey::from_event(OPTION, Some(6), "Z").unwrap(), z, "klein geschrieben");
        assert_eq!(Hotkey::from_event(OPTION, None, ""), None);
    }

    #[test]
    fn close_focused_avoids_command_escape() {
        // macOS fängt ⌘Esc systemweit ab, der Event-Monitor sieht es nie. Deshalb darf der Standard das nicht sein.
        assert_ne!(HotkeyAction::CloseFocused.default_key(), Hotkey::new(COMMAND, "Esc"));
        assert_eq!(Hotkey::from_event(OPTION, Some(53), "\u{1b}"), Some(HotkeyAction::CloseFocused.default_key()));
    }

    /// Blanke F-Tasten gehören dem Programm im Terminal, Kadrell nimmt sie nur mit ⌥.
    #[test]
    fn panels_default_to_option_function_keys() {
        assert_eq!(HotkeyAction::ShowHelp.default_key(), Hotkey::new(OPTION, "F1"));
        assert_eq!(HotkeyAction::RenameSession.default_key(), Hotkey::new(OPTION, "F2"));
        assert_eq!(HotkeyAction::ShowStats.default_key(), Hotkey::new(OPTION, "F3"));
        assert_eq!(HotkeyAction::ShowExtensions.default_key(), Hotkey::new(OPTION, "F4"));
        for &a in HotkeyAction::ALL {
            let k = a.default_key();
            if k.key.starts_with('F') && k.key.chars().count() > 1 {
                assert_ne!(k.mods, 0, "{}", a.raw());
            }
        }
    }

    #[test]
    fn function_key_needs_exact_modifiers() {
        let f1 = |m| Hotkey::from_event(m, Some(122), "\u{F704}");
        assert_eq!(f1(OPTION), Some(HotkeyAction::ShowHelp.default_key()));
        assert_ne!(f1(0), Some(HotkeyAction::ShowHelp.default_key()));
        assert_ne!(f1(SHIFT), Some(HotkeyAction::ShowHelp.default_key()));
    }

    #[test]
    fn tile_index() {
        assert_eq!(HotkeyAction::Focus1.tile_index(), Some(0));
        assert_eq!(HotkeyAction::Focus9.tile_index(), Some(8));
        assert_eq!(HotkeyAction::FocusLeft.tile_index(), None);
        assert_eq!(HotkeyAction::FocusSidebar.tile_index(), None);
        assert_eq!(HotkeyAction::FocusSidebar.default_key(), Hotkey::new(COMMAND, "1"));
        assert_eq!(HotkeyAction::NextLayout.default_key(), Hotkey::new(COMMAND, "l"));
        assert_eq!(HotkeyAction::Focus3.default_key(), Hotkey::new(OPTION, "3"));
        assert_eq!(HotkeyAction::Focus3.title_key(), "Kachel 3");
        assert_eq!(HotkeyAction::Zoom.title_key(), "Zoom: Fokus-Kachel allein");
        assert_eq!(HotkeyAction::CloseFocused.title_key(), "Fokus-Kachel schließen");
    }

    #[test]
    fn only_deviations_are_stored_and_empty_means_unbound() {
        let mut settings = Settings::default();
        let mut map = current(&settings);
        assert_eq!(map.len(), HotkeyAction::ALL.len());
        assert!(!has_overrides(&settings));
        store(&mut settings, &map);
        assert!(!has_overrides(&settings), "nichts weicht ab");

        map.insert(HotkeyAction::Zoom, Hotkey::new(COMMAND, "y"));
        map.remove(&HotkeyAction::NextWaiting);
        store(&mut settings, &map);
        let saved = settings.string_map(SETTINGS_KEY);
        assert_eq!(saved.len(), 2);
        assert_eq!(saved["zoom"], format!("{COMMAND}|y"));
        assert_eq!(saved["nextWaiting"], "");

        let back = current(&settings);
        assert_eq!(back[&HotkeyAction::Zoom], Hotkey::new(COMMAND, "y"));
        assert!(!back.contains_key(&HotkeyAction::NextWaiting));
        assert_eq!(display(&back, HotkeyAction::Zoom).as_deref(), Some("⌘Y"));
        assert_eq!(display(&back, HotkeyAction::NextWaiting), None);
        assert_eq!(display(&back, HotkeyAction::Restart), None, "leerer Default ist ohne Kürzel");
        assert_eq!(action_for(&back, &Hotkey::new(COMMAND, "y")), Some(HotkeyAction::Zoom));
        assert_eq!(action_for(&back, &Hotkey::new(OPTION, "z")), None, "alte Belegung ist frei");
    }
}
