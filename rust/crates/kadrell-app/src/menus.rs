//! Menüstruktur nach `buildMenu()` der Mac-App (Kadrell, Datei, Bearbeiten, Ansicht, Session, Fenster).
//! macOS setzt daraus die System-Menüleiste, Windows zeichnet sie selbst (`ui::menubar`), denn GPUI zeigt dort
//! kein App-Menü. Nicht belegte Einträge stehen ausgegraut da, damit die Struktur der Mac-App sichtbar bleibt.

use gpui::{Action, App, Context, Div, InteractiveElement, KeyBinding, actions};

use crate::Shell;

actions!(kadrell, [NewSession, NewSessionHere, NewTerminal, CloseSession, StopSession, ResumeSession, RenameSession, RenameGroup, ZoomTile, NextSession, PrevSession, NextLayout, MoreColumns, FewerColumns, ToggleTheme, Minimize, Zoom, Quit, OpenPalette, FindAll, OpenSettings, CloseWindow]);

pub enum Entry {
    Separator,
    Item { label: &'static str, shortcut: &'static str, action: Option<Box<dyn Action>> },
}

pub struct MenuDef {
    pub name: &'static str,
    pub entries: Vec<Entry>,
}

/// Kürzel-Beschriftung: erster Wert Mac, zweiter Windows.
const fn keys(mac: &'static str, win: &'static str) -> &'static str {
    if cfg!(target_os = "macos") { mac } else { win }
}

fn item(label: &'static str, shortcut: &'static str, action: impl Action) -> Entry {
    Entry::Item { label, shortcut, action: Some(Box::new(action)) }
}

fn off(label: &'static str, shortcut: &'static str) -> Entry {
    Entry::Item { label, shortcut, action: None }
}

fn app_menu() -> MenuDef {
    MenuDef {
        name: "Kadrell",
        entries: vec![
            off("Über Kadrell", ""),
            off("Was ist neu", ""),
            off("Statistik", ""),
            off("Extensions …", ""),
            Entry::Separator,
            item("Einstellungen …", keys("⌘,", "Strg+,"), OpenSettings),
            Entry::Separator,
            item("Kadrell beenden", keys("⌘Q", "Strg+Q"), Quit),
        ],
    }
}

fn file_menu() -> MenuDef {
    MenuDef {
        name: "Datei",
        entries: vec![
            item("Neue Session", keys("⌘N", "Strg+N"), NewSession),
            item("Neue Session im selben Ordner", keys("⌘↩", "Strg+Enter"), NewSessionHere),
            item("Neues Terminal ohne Claude", keys("⌘T", "Strg+T"), NewTerminal),
            off("Remote verbinden …", keys("⇧⌘N", "Strg+Umschalt+N")),
            off("Neue Gruppe …", keys("⌥⌘N", "Strg+Alt+N")),
            Entry::Separator,
            off("Neues Fenster", keys("⇧⌘T", "Strg+Umschalt+T")),
            item("Fenster schließen", keys("⇧⌘W", "Strg+Umschalt+W"), CloseWindow),
            Entry::Separator,
            item("Session schließen", keys("⌘W", ""), CloseSession),
        ],
    }
}

fn edit_menu() -> MenuDef {
    MenuDef {
        name: "Bearbeiten",
        entries: vec![
            off("Ausschneiden", keys("⌘X", "Strg+X")),
            off("Kopieren", keys("⌘C", "Strg+C")),
            off("Einsetzen", keys("⌘V", "Strg+V")),
            off("Ganze Gruppen auswählen", keys("⌘A", "Strg+A")),
            off("Alle Sessions auswählen", keys("⇧⌘A", "Strg+Umschalt+A")),
            Entry::Separator,
            off("Im Terminal suchen …", keys("⌘F", "Strg+F")),
            item("In allen Terminals suchen …", keys("⇧⌘F", "Strg+Umschalt+F"), FindAll),
        ],
    }
}

fn view_menu() -> MenuDef {
    MenuDef {
        name: "Ansicht",
        entries: vec![
            off("Auto-Modus ein/aus", ""),
            off("Baum ein/aus", keys("⌘B", "Strg+B")),
            off("Alle Gruppen auf-/zuklappen", keys("⇧⌘B", "Strg+Umschalt+B")),
            Entry::Separator,
            off("Terminal-Schrift größer", keys("⌘+", "Strg++")),
            off("Terminal-Schrift kleiner", keys("⌘-", "Strg+-")),
            off("Terminal-Schrift Standardgröße", keys("⌘0", "Strg+0")),
            Entry::Separator,
            item("Kachel zoomen", keys("⌘⇧↩", "Strg+Umschalt+Enter"), ZoomTile),
            item("Nächste Session", "Alt+N", NextSession),
            item("Vorherige Session", "Alt+P", PrevSession),
            item("Farbschema wechseln", "Alt+T", ToggleTheme),
            Entry::Separator,
            item("Suche", keys("⌘P", "Strg+P"), OpenPalette),
        ],
    }
}

fn session_menu() -> MenuDef {
    MenuDef {
        name: "Session",
        entries: vec![
            item("Fortsetzen", "", ResumeSession),
            item("Stoppen", "", StopSession),
            item("Umbenennen", "F2", RenameSession),
            item("Gruppe umbenennen", "", RenameGroup),
            item("Entfernen …", "", CloseSession),
            Entry::Separator,
            off("Nur diese zeigen", ""),
            off("Aus Ansicht nehmen", ""),
            off("Verschieben nach", ""),
            Entry::Separator,
            off("Pfad kopieren", ""),
            off(keys("Im Finder zeigen", "Im Explorer zeigen"), ""),
        ],
    }
}

fn window_menu() -> MenuDef {
    MenuDef {
        name: "Fenster",
        entries: vec![
            item(keys("Im Dock ablegen", "Minimieren"), keys("⌘M", ""), Minimize),
            item("Zoomen", "", Zoom),
        ],
    }
}

pub fn menus() -> Vec<MenuDef> {
    vec![app_menu(), file_menu(), edit_menu(), view_menu(), session_menu(), window_menu()]
}

/// Tastenbelegungen und globale Aktionen. Alt+N/P/T bleiben in `Shell::on_key` (Strg+Alt ist AltGr und gehört
/// dem Terminal), hier steht nur, was als Aktion gebunden ist.
pub fn init(cx: &mut App) {
    let new_key = if cfg!(target_os = "macos") { "cmd-n" } else { "ctrl-n" };
    cx.bind_keys([KeyBinding::new(new_key, NewSession, None)]);
    let (here_key, term_key, zoom_key) = if cfg!(target_os = "macos") { ("cmd-enter", "cmd-t", "cmd-shift-enter") } else { ("ctrl-enter", "ctrl-t", "ctrl-shift-enter") };
    cx.bind_keys([
        KeyBinding::new(here_key, NewSessionHere, None),
        KeyBinding::new(term_key, NewTerminal, None),
        KeyBinding::new(zoom_key, ZoomTile, None),
        KeyBinding::new("f2", RenameSession, None),
    ]);
    let (search_key, find_all_key) = if cfg!(target_os = "macos") { ("cmd-p", "cmd-shift-f") } else { ("ctrl-p", "ctrl-shift-f") };
    cx.bind_keys([KeyBinding::new(search_key, OpenPalette, None), KeyBinding::new(find_all_key, FindAll, None)]);
    cx.bind_keys([KeyBinding::new(if cfg!(target_os = "macos") { "cmd-," } else { "ctrl-," }, OpenSettings, None)]);
    // Alt+F4 und der rote Knopf verstecken nur das Fenster (`tray_wire`), beendet wird mit Strg+Q, über das Menü oder das Tray.
    let (quit_key, close_key) = if cfg!(target_os = "macos") { ("cmd-q", "cmd-shift-w") } else { ("ctrl-q", "ctrl-shift-w") };
    cx.bind_keys([KeyBinding::new(quit_key, Quit, None), KeyBinding::new(close_key, CloseWindow, None)]);
    cx.on_action(|_: &Quit, cx| cx.quit());
    cx.on_action(|_: &ToggleTheme, cx| Shell::toggle_theme(cx));
    set_native_menus(cx);
}

/// Aktionen des Fensters: wirken auf die Shell und laufen über den Fokuspfad (Menü, Tastatur).
pub fn attach_actions(root: Div, cx: &mut Context<Shell>) -> Div {
    root.on_action(cx.listener(|this, _: &NewSession, window, cx| this.new_session_action(window, cx)))
        .on_action(cx.listener(|this, _: &OpenPalette, window, cx| this.open_palette("", window, cx)))
        .on_action(cx.listener(|this, _: &FindAll, window, cx| this.open_palette("/", window, cx)))
        .on_action(cx.listener(|this, _: &NewSessionHere, window, cx| this.new_session_here(window, cx)))
        .on_action(cx.listener(|this, _: &NewTerminal, window, cx| this.new_terminal(window, cx)))
        .on_action(cx.listener(|this, _: &CloseSession, window, cx| this.remove_focused(window, cx)))
        .on_action(cx.listener(|this, _: &StopSession, window, cx| this.stop_focused(window, cx)))
        .on_action(cx.listener(|this, _: &ResumeSession, window, cx| this.resume_focused(window, cx)))
        .on_action(cx.listener(|this, _: &RenameSession, window, cx| this.rename_focused(window, cx)))
        .on_action(cx.listener(|this, _: &RenameGroup, window, cx| this.rename_focused_group(window, cx)))
        .on_action(cx.listener(|this, _: &ZoomTile, _, cx| this.toggle_zoom(cx)))
        .on_action(cx.listener(|this, _: &NextSession, window, cx| this.step(1, window, cx)))
        .on_action(cx.listener(|this, _: &PrevSession, window, cx| this.step(-1, window, cx)))
        .on_action(cx.listener(|this, _: &NextLayout, _, cx| this.cycle_layout(cx)))
        .on_action(cx.listener(|this, _: &MoreColumns, _, cx| this.step_cols(1, cx)))
        .on_action(cx.listener(|this, _: &FewerColumns, _, cx| this.step_cols(-1, cx)))
        .on_action(cx.listener(|this, _: &OpenSettings, window, cx| this.open_settings_action(window, cx)))
        .on_action(cx.listener(|this, _: &CloseWindow, window, cx| this.hide_main_window(window, cx)))
        .on_action(cx.listener(|_, _: &Minimize, window, _| window.minimize_window()))
        .on_action(cx.listener(|_, _: &Zoom, window, _| window.zoom_window()))
}

#[cfg(target_os = "macos")]
fn set_native_menus(cx: &mut App) {
    use gpui::{Menu, MenuItem};
    let convert = |e: Entry| match e {
        Entry::Separator => MenuItem::separator(),
        Entry::Item { label, action: Some(action), .. } => {
            MenuItem::Action { name: label.into(), action, os_action: None, checked: false, disabled: false }
        }
        Entry::Item { label, action: None, .. } => MenuItem::action(label, gpui::NoAction).disabled(true),
    };
    cx.set_menus(menus().into_iter().map(|m| Menu::new(m.name).items(m.entries.into_iter().map(convert))));
}

/// Windows kennt in GPUI kein App-Menü, `ui::menubar` zeichnet es.
#[cfg(not(target_os = "macos"))]
fn set_native_menus(_cx: &mut App) {}
