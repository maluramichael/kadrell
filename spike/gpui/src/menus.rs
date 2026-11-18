//! Menüstruktur nach `buildMenu()` der Mac-App (Kadrell, Datei, Bearbeiten, Ansicht, Session, Fenster).
//! macOS setzt daraus die System-Menüleiste, Windows zeichnet sie selbst (`ui::menubar`), denn GPUI zeigt dort
//! kein App-Menü. Nicht belegte Einträge stehen ausgegraut da, damit die Struktur der Mac-App sichtbar bleibt.

use gpui::{Action, App, Context, Div, InteractiveElement, KeyBinding, actions};

use crate::Shell;

actions!(kadrell, [NewSession, NextSession, PrevSession, NextLayout, MoreColumns, FewerColumns, ToggleTheme, Minimize, Zoom, Quit]);

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
            off("Einstellungen …", keys("⌘,", "Strg+,")),
            Entry::Separator,
            item("Kadrell beenden", keys("⌘Q", "Alt+F4"), Quit),
        ],
    }
}

fn file_menu() -> MenuDef {
    MenuDef {
        name: "Datei",
        entries: vec![
            item("Neue Session", keys("⌘N", "Strg+N"), NewSession),
            off("Neue Session im selben Ordner", keys("⌘↩", "Strg+Enter")),
            off("Neues Terminal ohne Claude", keys("⌘T", "Strg+T")),
            off("Remote verbinden …", keys("⇧⌘N", "Strg+Umschalt+N")),
            off("Neue Gruppe …", keys("⌥⌘N", "Strg+Alt+N")),
            Entry::Separator,
            off("Neues Fenster", keys("⇧⌘T", "Strg+Umschalt+T")),
            off("Fenster schließen", keys("⇧⌘W", "Strg+Umschalt+W")),
            Entry::Separator,
            off("Session schließen", keys("⌘W", "Strg+W")),
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
            off("In allen Terminals suchen …", keys("⇧⌘F", "Strg+Umschalt+F")),
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
            item("Nächste Session", "Alt+N", NextSession),
            item("Vorherige Session", "Alt+P", PrevSession),
            item("Farbschema wechseln", "Alt+T", ToggleTheme),
            Entry::Separator,
            off("Suche", keys("⌘P", "Strg+P")),
        ],
    }
}

fn session_menu() -> MenuDef {
    MenuDef {
        name: "Session",
        entries: vec![
            off("Fortsetzen", ""),
            off("Stoppen", ""),
            off("Entfernen …", ""),
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
    if cfg!(target_os = "macos") {
        cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);
    }
    cx.on_action(|_: &Quit, cx| cx.quit());
    cx.on_action(|_: &ToggleTheme, cx| Shell::toggle_theme(cx));
    set_native_menus(cx);
}

/// Aktionen des Fensters: wirken auf die Shell und laufen über den Fokuspfad (Menü, Tastatur).
pub fn attach_actions(root: Div, cx: &mut Context<Shell>) -> Div {
    // TODO: NewSession gehört dem Overlay-Zweig (Strg+N / Cmd+N öffnet den Dialog), hier bewusst nicht belegt.
    root.on_action(cx.listener(|this, _: &NextSession, window, cx| this.step(1, window, cx)))
        .on_action(cx.listener(|this, _: &PrevSession, window, cx| this.step(-1, window, cx)))
        .on_action(cx.listener(|this, _: &NextLayout, _, cx| this.cycle_layout(cx)))
        .on_action(cx.listener(|this, _: &MoreColumns, _, cx| this.step_cols(1, cx)))
        .on_action(cx.listener(|this, _: &FewerColumns, _, cx| this.step_cols(-1, cx)))
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
