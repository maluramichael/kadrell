//! Inhalt der Hilfe (⌥F1) nach `UI/AboutView.swift`: Abschnitte mit Kürzel und Text. Die Kürzel der belegbaren Aktionen
//! kommen aus den Einstellungen (`hotkeys::current`, unter Windows in der Schreibweise des Systems), alles andere
//! sind feste Bedienhinweise. Reine Daten, ohne GPUI.

use kadrell_core::Settings;
use kadrell_core::hotkeys::HotkeyAction::{self, *};
use kadrell_i18n::trd;

use super::os_text;
use crate::lang::de;
use crate::ui::ext::keys;
use crate::ui::settings::filter;

/// Eine Zeile: Kürzel (oder Geste) und was sie tut.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub keys: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    pub title: String,
    pub rows: Vec<Row>,
}

/// Hilfetext einer Aktion (deutscher Quelltext, `HotkeyAction.helpText` der Mac-App).
pub fn help_text(action: HotkeyAction) -> String {
    if action.tile_index().is_some() {
        return trd!("Kachel 1–9 fokussieren");
    }
    match action {
        FocusLeft | FocusRight | FocusUp | FocusDown => trd!(de!("Fokus bewegen · im Stack auf- und zuklappen")),
        SwapLeft | SwapRight | SwapUp | SwapDown => trd!(de!("Fokus-Kachel mit Nachbar tauschen")),
        ResizeLeft | ResizeRight | ResizeUp | ResizeDown => trd!(de!("Trennlinie an der Fokus-Kachel um 5 % verschieben · Scrollen: Spalte ⅓ ½ ⅔ breit")),
        SplitRight | SplitDown => trd!(de!("Layout Frei (wie i3): die Kachel nach der fokussierten entsteht rechts bzw. unten")),
        NextSession | PrevSession => trd!(de!("Nächste / vorige Kachel")),
        PreviewNext | PreviewPrev => trd!(de!("Baum als Vorschau durchblättern, Auswahl bleibt · ⏎ übernimmt, Esc zurück")),
        NextWaiting => trd!(de!("Springt zur nächsten Session, die auf dich wartet")),
        CloseFocused => trd!(de!("Fokus-Kachel schließen (Esc selbst geht an Claude)")),
        OpenEditor => trd!(de!("Ordner der Fokus-Session im Editor aus den Einstellungen öffnen")),
        RenameSession => trd!(de!("Fokus-Session umbenennen, auch im Baum · Claude überschreibt den Namen danach nicht mehr")),
        SyncInput => trd!(de!("Sync: Tippen und ⌘V gehen an alle offenen Kacheln gleichzeitig · Badge „SYNC“")),
        ToggleGrouping => trd!(de!("Gruppen aus: alle Sessions in einer flachen Liste, mit eigener Handreihenfolge")),
        QuickStart => trd!(de!("Idee tippen, Kadrell erkennt das Projekt und startet dort eine Session mit der Idee als erstem Prompt")),
        SearchHistory => trd!(de!("Sucht in Titeln und eigenen Prompts aller bisherigen Claude-Sessions · ⏎ setzt sie im alten Ordner fort oder springt zur laufenden")),
        ShowHelp => trd!(de!("diese Hilfe")),
        ShowStats => trd!(de!("Statistik: Limits, Prompts und Zähler")),
        ShowExtensions => trd!(de!("Extensions: an- und abschalten, neu laden, Einstellungen, Log")),
        FocusRightSidebar => trd!(de!("Rechte Sidebar der Extensions: ↑↓ Eintrag, ←→ Tab, ⏎ erste Aktion, ⌥⏎ alle Aktionen, Esc zurück")),
        other => trd!(&other.title_key()),
    }
}

/// Zeilen für belegbare Aktionen: eine je Hilfetext, die Kürzel aller Aktionen mit diesem Text nebeneinander, lange
/// Reihen (Kachel 1–9) nur „erstes … letztes“. Aktionen ohne Kürzel fehlen.
pub fn action_rows(settings: &Settings, actions: &[HotkeyAction], mac: bool) -> Vec<Row> {
    let mut out: Vec<Row> = Vec::new();
    for &action in actions {
        let text = help_text(action);
        if out.iter().any(|r| r.text == text) {
            continue;
        }
        let labels: Vec<String> = actions
            .iter()
            .filter(|a| help_text(**a) == text)
            .filter_map(|a| keys::binding(settings, *a, mac).map(|k| keys::label(&k, mac)))
            .collect();
        let (Some(first), Some(last)) = (labels.first(), labels.last()) else { continue };
        out.push(Row { keys: if labels.len() > 4 { format!("{first} … {last}") } else { labels.join(" ") }, text });
    }
    out
}

/// Feste Zeile: Kürzel (Mac-Schreibweise, unter Windows übersetzt) und Text, beides in der eingestellten Sprache.
fn fixed(keys: &str, text: &str) -> Row {
    Row { keys: os_text(keys), text: os_text(text) }
}

fn extra(rows: &[(&str, &str)]) -> Vec<Row> {
    rows.iter().map(|(k, t)| fixed(k, t)).collect()
}

fn open_section(settings: &Settings, mac: bool) -> Section {
    let mut rows = extra(&[
        (de!("Klick"), de!("nur diese Session zeigen · auf Gruppe: alle ihre Sessions")),
        (de!("⌘ Klick"), de!("Session dazu oder weg")),
        (de!("⇧ Klick"), de!("Bereich seit dem letzten Klick dazu")),
        ("⌘A", de!("ganze Gruppen der Auswahl, nochmal: zurück")),
        ("⌘⇧A", de!("alle Sessions, nochmal: zurück")),
        ("⌘N", de!("Neue Session: Projekt suchen, ~/d/p abkürzen, ⌘O Finder, Ordner hineinziehen")),
        ("⌘⏎", de!("Neue Session im Ordner der fokussierten")),
        ("⌘T", de!("Terminal ohne Claude im Ordner der fokussierten")),
        ("⌘⌥N", de!("Neue leere Gruppe mit Name, Ordner und Farbe")),
        (de!("+ an Gruppe"), de!("neue Session in der Gruppe · mit ⌘: Terminal ohne Claude")),
    ]);
    rows.extend(action_rows(settings, &[OpenEditor, RenameSession], mac));
    rows.extend(extra(&[(de!("Stift"), de!("an Kachel und Baum-Zeile: umbenennen"))]));
    Section { title: trd!("Sessions öffnen"), rows }
}

fn navigation_section(settings: &Settings, mac: bool) -> Section {
    let tiles: Vec<HotkeyAction> = HotkeyAction::ALL.iter().copied().filter(|a| a.tile_index().is_some()).collect();
    let mut actions = vec![FocusLeft, FocusRight, FocusUp, FocusDown, NextSession, PrevSession, LastSession, PreviewNext, PreviewPrev, NextWaiting];
    actions.extend(tiles);
    actions.extend([FocusSidebar, FocusWorkspace, FocusRightSidebar]);
    let mut rows = action_rows(settings, &actions, mac);
    rows.extend(extra(&[
        ("⇧F10", de!("Im Baum: Menü der Session, bei zugeklappter Gruppe das der Gruppe · ⌃⏎ geht an die Session")),
        ("⌘P", de!("Suche, mit > Kommandos")),
        ("⌘F", de!("Im Terminal suchen, ⌘G weiter")),
        ("⌘⇧F", de!("In allen Terminals suchen")),
    ]));
    rows.extend(action_rows(settings, &[SearchHistory], mac));
    Section { title: trd!("Navigation"), rows }
}

fn tiles_section(settings: &Settings, mac: bool) -> Section {
    let first = [SwapLeft, SwapRight, SwapUp, SwapDown, ResizeLeft, ResizeRight, ResizeUp, ResizeDown, SplitRight, SplitDown, Zoom, NextLayout, SyncInput, CloseFocused];
    let mut rows = action_rows(settings, &first, mac);
    rows.extend(extra(&[
        (de!("Trennlinie ziehen"), de!("Größen des Layouts ändern, Doppelklick verteilt gleich · Terminals füllen die Felder der Reihe nach")),
        (de!("‹ SP › in der Leiste"), de!("Spalten im Grid, unter 1 wieder automatisch")),
        (de!("TEILT in der Leiste"), de!("Frei: Teilung an der Fokus-Kachel, Klick schaltet → ↓ AUTO (längere Seite)")),
        (de!("Seitlich wischen"), de!("Scrollen: Spalten verschieben (Maus: ⇧ + Rad), der Fokus rückt von selbst ins Bild")),
        (de!("Ziehen"), de!("Session oder Gruppe umsortieren, Baum und Kacheln gleich · Session im Baum auf eine andere Gruppe: dorthin verschieben")),
        (de!("Rechtsklick im Baum"), de!("Menü der Session oder Gruppe: Verschieben nach, Bearbeiten, Ordner, Schließen")),
        ("⌘W", de!("Fokus-Session beenden und entfernen, mit Rückfrage")),
        (de!("⌥ + Klick auf X"), de!("Schließen ohne Rückfrage")),
        (de!("Mittelklick im Baum"), de!("Session schließen · auf Gruppe: die ganze Gruppe · mit ⌥ ohne Rückfrage")),
        ("⌘B", de!("Baum ein/aus")),
        ("⌘⇧B", de!("Alle Gruppen auf- oder zuklappen")),
    ]));
    rows.extend(action_rows(settings, &[ToggleRightSidebar, CycleSort], mac));
    Section { title: trd!("Kacheln verwalten"), rows }
}

fn window_section(settings: &Settings, mac: bool) -> Section {
    let mut rows = extra(&[
        (de!("Rundes X"), de!("Fenster nur verstecken, Sessions laufen weiter")),
        (de!("Menüleisten-Icon / Dock"), de!("Fenster zurückholen")),
        ("⌘⇧T", de!("Neues Fenster: eigene Auswahl und eigenes Layout, dieselben Sessions")),
        ("⌘⇧W", de!("Fenster schließen, Sessions laufen weiter")),
        ("⌘M", de!("Im Dock ablegen")),
        (de!("⌘ Mausrad"), de!("Terminal-Schrift aller Sessions größer/kleiner")),
        ("⌘,", de!("Einstellungen: Projektordner, Darstellung, Tastenkürzel")),
    ]);
    rows.extend(action_rows(settings, &[ShowHelp, ShowStats, ShowExtensions], mac));
    Section { title: trd!("Fenster und App"), rows }
}

/// Alle Abschnitte der Hilfe in der Reihenfolge der Mac-App.
pub fn sections(settings: &Settings, mac: bool) -> Vec<Section> {
    vec![open_section(settings, mac), navigation_section(settings, mac), tiles_section(settings, mac), window_section(settings, mac)]
}

/// Mit offenem Filter nur Zeilen, deren Kürzel oder Text passt (alle Zeilen eines Abschnitts, wenn sein Name passt).
pub fn filtered(sections: &[Section], query: &str) -> Vec<Section> {
    let query = query.trim();
    if query.is_empty() {
        return sections.to_vec();
    }
    sections
        .iter()
        .filter_map(|s| {
            let rows: Vec<Row> = if filter::matches(&s.title, query) { s.rows.clone() } else { s.rows.iter().filter(|r| filter::matches(&format!("{} {}", r.keys, r.text), query)).cloned().collect() };
            (!rows.is_empty()).then(|| Section { title: s.title.clone(), rows })
        })
        .collect()
}

/// Zahl der Zeilen, für „N Treffer“ unter dem Filter.
pub fn row_count(sections: &[Section]) -> usize {
    sections.iter().map(|s| s.rows.len()).sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use kadrell_core::hotkeys::{self, Hotkey, mods};

    fn defaults() -> Settings {
        Settings::default()
    }

    fn row<'a>(s: &'a [Section], needle: &str) -> &'a Row {
        s.iter().flat_map(|s| &s.rows).find(|r| r.text.contains(needle) || r.keys.contains(needle)).unwrap_or_else(|| panic!("keine Zeile mit {needle}"))
    }

    #[test]
    fn there_are_four_sections_in_the_order_of_the_mac_app() {
        let s = sections(&defaults(), true);
        let titles: Vec<&str> = s.iter().map(|s| s.title.as_str()).collect();
        assert_eq!(titles, ["Sessions öffnen", "Navigation", "Kacheln verwalten", "Fenster und App"]);
        assert!(s.iter().all(|s| s.rows.len() > 5));
    }

    #[test]
    fn tile_shortcuts_collapse_to_first_and_last() {
        let s = sections(&defaults(), true);
        assert_eq!(row(&s, "Kachel 1–9").keys, "⌥1 … ⌥9");
    }

    #[test]
    fn four_arrows_stay_side_by_side() {
        let s = sections(&defaults(), true);
        assert_eq!(row(&s, "Fokus bewegen").keys, "⌥← ⌥→ ⌥↑ ⌥↓");
    }

    #[test]
    fn rebinding_changes_the_help() {
        let mut settings = defaults();
        let mut map = hotkeys::current(&settings);
        map.insert(ShowHelp, Hotkey::new(mods::COMMAND | mods::SHIFT, "k"));
        hotkeys::store(&mut settings, &map);
        let s = sections(&settings, true);
        assert_eq!(row(&s, "diese Hilfe").keys, "⇧⌘K");
    }

    #[test]
    fn an_action_without_shortcut_has_no_row() {
        let mut settings = defaults();
        let mut map = hotkeys::current(&settings);
        map.remove(&ShowStats);
        hotkeys::store(&mut settings, &map);
        let rows = action_rows(&settings, &[ShowStats, ShowHelp], true);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].text, "diese Hilfe");
    }

    #[cfg(windows)]
    #[test]
    fn windows_spells_the_keys_out() {
        let s = sections(&defaults(), false);
        assert_eq!(row(&s, "diese Hilfe").keys, "Alt+F1");
        assert!(row(&s, "Neue Session: Projekt").keys.contains("Strg"), "⌘N wird zu Strg+N");
    }

    #[test]
    fn filter_keeps_matching_rows_or_whole_sections() {
        let s = sections(&defaults(), true);
        let hits = filtered(&s, "suche");
        assert!(row_count(&hits) < row_count(&s) && row_count(&hits) > 0);
        let by_title = filtered(&s, "navigation");
        assert_eq!(by_title.len(), 1);
        assert_eq!(by_title[0].rows.len(), s[1].rows.len(), "Abschnittsname passt: alle Zeilen");
        assert_eq!(filtered(&s, "   "), s, "ohne Eingabe nichts filtern");
        assert!(filtered(&s, "xyzzy-nichts").is_empty());
    }

    #[test]
    fn filter_ignores_umlauts_and_case() {
        let s = sections(&defaults(), true);
        assert!(!filtered(&s, "SCHLIESSEN").is_empty());
    }

    #[test]
    fn every_action_has_a_help_text_in_both_languages() {
        for action in HotkeyAction::ALL {
            assert!(!help_text(*action).is_empty());
        }
    }
}
