//! Menüstruktur nach `buildMenu()` der Mac-App (Kadrell, Datei, Bearbeiten, Ansicht, Session, Fenster).
//! macOS setzt daraus die System-Menüleiste, Windows zeichnet sie selbst (`ui::menubar`), denn GPUI zeigt dort
//! kein App-Menü. Nicht belegte Einträge stehen ausgegraut da, damit die Struktur der Mac-App sichtbar bleibt.
//! Beschriftungen und Kürzel entstehen beim Bauen in der eingestellten Sprache; nach einem Sprachwechsel baut
//! `Shell::sync_language` die Menüs neu (`menus()` ist billig).

use gpui::{Action, App, Context, Div, InteractiveElement, KeyBinding, actions};
use kadrell_i18n::trd;

use crate::Shell;
use crate::lang::{de, shortcut, shortcut_mac_only, shortcut_os};
use crate::misc::{OpenAbout, OpenWhatsNew};
use crate::ui::ext::{FocusRightSidebar, OpenExtensions, ToggleRightSidebar};

actions!(kadrell, [NewSession, NewSessionHere, NewTerminal, CloseSession, StopSession, ResumeSession, RenameSession, RenameGroup, ZoomTile, NextSession, PrevSession, NextLayout, MoreColumns, FewerColumns, ToggleTheme, Minimize, Zoom, Quit, OpenPalette, FindAll, OpenSettings, OpenStats, QuickStart, SearchHistory, CloseWindow, NewWindow, NewGroup, ShowContextMenu, NewRemote]);

pub enum Entry {
    Separator,
    /// `key` ist der deutsche Quelltext (stabil, die Palette sucht Einträge darüber), `label` der Text in der
    /// eingestellten Sprache.
    Item { key: &'static str, label: String, shortcut: String, action: Option<Box<dyn Action>> },
}

pub struct MenuDef {
    /// Deutscher Quelltext des Namens.
    pub key: &'static str,
    pub name: String,
    pub entries: Vec<Entry>,
}

fn menu(key: &'static str, entries: Vec<Entry>) -> MenuDef {
    MenuDef { key, name: trd!(key), entries }
}

fn item(key: &'static str, shortcut: String, action: impl Action) -> Entry {
    Entry::Item { key, label: trd!(key), shortcut, action: Some(Box::new(action)) }
}

fn off(key: &'static str, shortcut: String) -> Entry {
    Entry::Item { key, label: trd!(key), shortcut, action: None }
}

/// Eintrag mit eigener Beschriftung je Betriebssystem (Finder/Explorer, Dock/Minimieren).
fn item_os(mac: &'static str, win: &'static str, shortcut: String, action: impl Action) -> Entry {
    item(if cfg!(target_os = "macos") { mac } else { win }, shortcut, action)
}

fn none() -> String {
    String::new()
}

fn app_menu() -> MenuDef {
    // „Kadrell“ ist der Name der App und bleibt, wie er ist.
    MenuDef {
        key: "Kadrell",
        name: "Kadrell".to_string(),
        entries: vec![
            item(de!("Über Kadrell"), Shell::about_shortcut(), OpenAbout),
            item(de!("Was ist neu"), none(), OpenWhatsNew),
            item(de!("Statistik"), shortcut("⌥F3"), OpenStats),
            item(de!("Extensions …"), shortcut_os("⌥F4", "⇧⌘E"), OpenExtensions),
            Entry::Separator,
            item(de!("Einstellungen …"), shortcut("⌘,"), OpenSettings),
            Entry::Separator,
            item(de!("Kadrell beenden"), shortcut("⌘Q"), Quit),
        ],
    }
}

fn file_menu() -> MenuDef {
    menu(
        de!("Datei"),
        vec![
            item(de!("Neue Session"), shortcut("⌘N"), NewSession),
            item(de!("Neue Session im selben Ordner"), shortcut("⌘↩"), NewSessionHere),
            item(de!("Schnellstart: Idee rein, Session raus"), shortcut("⇧⌘I"), QuickStart),
            item(de!("Neues Terminal ohne Claude"), shortcut("⌘T"), NewTerminal),
            item(de!("Remote verbinden …"), shortcut("⇧⌘N"), NewRemote),
            item(de!("Neue Gruppe …"), shortcut("⌥⌘N"), NewGroup),
            Entry::Separator,
            item(de!("Neues Fenster"), shortcut("⇧⌘T"), NewWindow),
            item(de!("Fenster schließen"), shortcut("⇧⌘W"), CloseWindow),
            Entry::Separator,
            item(de!("Session schließen"), shortcut_mac_only("⌘W"), CloseSession),
        ],
    )
}

fn edit_menu() -> MenuDef {
    menu(
        de!("Bearbeiten"),
        vec![
            off(de!("Ausschneiden"), shortcut("⌘X")),
            off(de!("Kopieren"), shortcut("⌘C")),
            off(de!("Einsetzen"), shortcut("⌘V")),
            off(de!("Ganze Gruppen auswählen"), shortcut("⌘A")),
            off(de!("Alle Sessions auswählen"), shortcut("⇧⌘A")),
            Entry::Separator,
            off(de!("Im Terminal suchen …"), shortcut("⌘F")),
            item(de!("In allen Terminals suchen …"), shortcut("⇧⌘F"), FindAll),
            item(de!("Alle Claude-Sessions durchsuchen, auch beendete"), shortcut("⇧⌘H"), SearchHistory),
        ],
    )
}

fn view_menu() -> MenuDef {
    menu(
        de!("Ansicht"),
        vec![
            item(de!("Auto-Modus ein/aus"), none(), crate::windows::ToggleAuto),
            item(de!("Baum ein/aus"), shortcut("⌘B"), crate::windows::ToggleSidebar),
            item(de!("Rechte Sidebar ein/aus"), shortcut("⌥⌘B"), ToggleRightSidebar),
            item(de!("Rechte Sidebar: Tastatur hierher"), shortcut("⌘3"), FocusRightSidebar),
            item(de!("Alle Gruppen auf-/zuklappen"), shortcut("⇧⌘B"), crate::ui::tree_actions::ToggleAllGroups),
            Entry::Separator,
            item(de!("Terminal-Schrift größer"), shortcut("⌘+"), crate::appearance::zoom::FontBigger),
            item(de!("Terminal-Schrift kleiner"), shortcut("⌘-"), crate::appearance::zoom::FontSmaller),
            item(de!("Terminal-Schrift Standardgröße"), shortcut("⌘0"), crate::appearance::zoom::FontReset),
            Entry::Separator,
            item(de!("Kachel zoomen"), shortcut("⌘⇧↩"), ZoomTile),
            item(de!("Nächste Session"), shortcut("⌥N"), NextSession),
            item(de!("Vorherige Session"), shortcut("⌥P"), PrevSession),
            item(de!("Farbschema wechseln"), shortcut("⌥T"), ToggleTheme),
            Entry::Separator,
            item(de!("Suche"), shortcut("⌘P"), OpenPalette),
        ],
    )
}

fn session_menu() -> MenuDef {
    menu(
        de!("Session"),
        vec![
            item(de!("Fortsetzen"), none(), ResumeSession),
            item(de!("Stoppen"), none(), StopSession),
            item(de!("Umbenennen"), "F2".to_string(), RenameSession),
            item(de!("Gruppe umbenennen"), none(), RenameGroup),
            item(de!("Entfernen …"), none(), CloseSession),
            Entry::Separator,
            off(de!("Nur diese zeigen"), none()),
            off(de!("Aus Ansicht nehmen"), none()),
            off(de!("Verschieben nach"), none()),
            Entry::Separator,
            off(de!("Pfad kopieren"), none()),
            off(if cfg!(target_os = "macos") { de!("Im Finder zeigen") } else { de!("Im Explorer zeigen") }, none()),
        ],
    )
}

fn window_menu() -> MenuDef {
    menu(
        de!("Fenster"),
        vec![
            item_os(de!("Im Dock ablegen"), de!("Minimieren"), shortcut_mac_only("⌘M"), Minimize),
            item(de!("Zoomen"), none(), Zoom),
        ],
    )
}

pub fn menus() -> Vec<MenuDef> {
    vec![app_menu(), file_menu(), edit_menu(), view_menu(), session_menu(), window_menu()]
}

/// Tastenbelegungen und globale Aktionen. Alt+N/P/T bleiben in `Shell::on_key` (Strg+Alt ist AltGr und gehört
/// dem Terminal), hier steht nur, was als Aktion gebunden ist.
pub fn init(cx: &mut App) {
    let new_key = if cfg!(target_os = "macos") { "cmd-n" } else { "ctrl-n" };
    let remote_key = if cfg!(target_os = "macos") { "cmd-shift-n" } else { "ctrl-shift-n" };
    cx.bind_keys([KeyBinding::new(new_key, NewSession, None), KeyBinding::new(remote_key, NewRemote, None)]);
    let (here_key, term_key, zoom_key) = if cfg!(target_os = "macos") { ("cmd-enter", "cmd-t", "cmd-shift-enter") } else { ("ctrl-enter", "ctrl-t", "ctrl-shift-enter") };
    cx.bind_keys([
        KeyBinding::new(here_key, NewSessionHere, None),
        KeyBinding::new(term_key, NewTerminal, None),
        KeyBinding::new(zoom_key, ZoomTile, None),
        KeyBinding::new("f2", RenameSession, None),
        KeyBinding::new("shift-f10", ShowContextMenu, None),
    ]);
    // Strg+Alt+N: neue Gruppe (⌥⌘N der Mac-App).
    cx.bind_keys([KeyBinding::new(if cfg!(target_os = "macos") { "cmd-alt-n" } else { "ctrl-alt-n" }, NewGroup, None)]);
    let (search_key, find_all_key) = if cfg!(target_os = "macos") { ("cmd-p", "cmd-shift-f") } else { ("ctrl-p", "ctrl-shift-f") };
    cx.bind_keys([KeyBinding::new(search_key, OpenPalette, None), KeyBinding::new(find_all_key, FindAll, None)]);
    let (quick_key, history_key) = if cfg!(target_os = "macos") { ("cmd-shift-i", "cmd-shift-h") } else { ("ctrl-shift-i", "ctrl-shift-h") };
    cx.bind_keys([KeyBinding::new(quick_key, QuickStart, None), KeyBinding::new(history_key, SearchHistory, None)]);
    cx.bind_keys([KeyBinding::new(if cfg!(target_os = "macos") { "cmd-," } else { "ctrl-," }, OpenSettings, None), KeyBinding::new("alt-f3", OpenStats, None)]);
    // Alt+F4 und der rote Knopf verstecken nur das Fenster (`tray_wire`), beendet wird mit Strg+Q, über das Menü oder das Tray.
    let (quit_key, close_key) = if cfg!(target_os = "macos") { ("cmd-q", "cmd-shift-w") } else { ("ctrl-q", "ctrl-shift-w") };
    let new_window_key = if cfg!(target_os = "macos") { "cmd-shift-t" } else { "ctrl-shift-t" };
    cx.bind_keys([KeyBinding::new(quit_key, Quit, None), KeyBinding::new(close_key, CloseWindow, None), KeyBinding::new(new_window_key, NewWindow, None)]);
    cx.on_action(|_: &Quit, cx| {
        // Mit laufenden Sessions fragt das Hauptfenster erst nach (`Shell::request_quit`).
        // `defer`: der Fenster steckt gerade im Dispatch der Aktion und lässt sich dann nicht ein zweites Mal ändern.
        match cx.windows().into_iter().find_map(|w| w.downcast::<Shell>()) {
            Some(main) => cx.defer(move |cx| {
                let _ = main.update(cx, |shell, window, cx| shell.request_quit(window, cx));
            }),
            None => cx.quit(),
        }
    });
    cx.on_action(|_: &ToggleTheme, cx| Shell::toggle_theme(cx));
    crate::windows::bind_keys(cx);
    set_native_menus(cx);
}

/// Aktionen des Fensters: wirken auf die Shell und laufen über den Fokuspfad (Menü, Tastatur).
pub fn attach_actions(root: Div, cx: &mut Context<Shell>) -> Div {
    let root = crate::windows::attach_actions(crate::appearance::zoom::attach_actions(root, cx), cx);
    let root = crate::ui::tree_actions::attach_actions(root, cx);
    root.on_action(cx.listener(|this, _: &NewSession, window, cx| this.new_session_action(window, cx)))
        .on_action(cx.listener(|this, _: &OpenPalette, window, cx| this.open_palette("", window, cx)))
        .on_action(cx.listener(|this, _: &NewRemote, window, cx| this.open_palette("@", window, cx)))
        .on_action(cx.listener(|this, _: &FindAll, window, cx| this.open_palette("/", window, cx)))
        .on_action(cx.listener(|this, _: &SearchHistory, window, cx| this.open_palette("#", window, cx)))
        .on_action(cx.listener(|this, _: &QuickStart, window, cx| this.open_quick_start(window, cx)))
        .on_action(cx.listener(|this, _: &NewSessionHere, window, cx| this.new_session_here(window, cx)))
        .on_action(cx.listener(|this, _: &NewTerminal, window, cx| this.new_terminal(window, cx)))
        .on_action(cx.listener(|this, _: &CloseSession, window, cx| this.remove_focused(window, cx)))
        .on_action(cx.listener(|this, _: &StopSession, window, cx| this.stop_focused(window, cx)))
        .on_action(cx.listener(|this, _: &ResumeSession, window, cx| this.resume_focused(window, cx)))
        .on_action(cx.listener(|this, _: &RenameSession, window, cx| this.rename_focused(window, cx)))
        .on_action(cx.listener(|this, _: &RenameGroup, window, cx| this.edit_focused_group(window, cx)))
        .on_action(cx.listener(|this, _: &NewGroup, window, cx| this.open_new_group(window, cx)))
        .on_action(cx.listener(|this, _: &ShowContextMenu, window, cx| this.show_context_menu_for_focus(window, cx)))
        .on_action(cx.listener(|this, _: &ZoomTile, _, cx| this.toggle_zoom(cx)))
        .on_action(cx.listener(|this, _: &NextSession, window, cx| this.cycle_focus(1, window, cx)))
        .on_action(cx.listener(|this, _: &PrevSession, window, cx| this.cycle_focus(-1, window, cx)))
        .on_action(cx.listener(|this, _: &NextLayout, _, cx| this.cycle_layout(cx)))
        .on_action(cx.listener(|this, _: &MoreColumns, _, cx| this.step_cols(1, cx)))
        .on_action(cx.listener(|this, _: &FewerColumns, _, cx| this.step_cols(-1, cx)))
        .on_action(cx.listener(|this, _: &OpenSettings, window, cx| this.open_settings_action(window, cx)))
        .on_action(cx.listener(|this, _: &CloseWindow, window, cx| this.hide_main_window(window, cx)))
        .on_action(cx.listener(|this, _: &NewWindow, _, cx| this.new_window_action(cx)))
        .on_action(cx.listener(|this, _: &OpenStats, window, cx| this.toggle_stats(window, cx)))
        .on_action(cx.listener(|_, _: &Minimize, window, _| window.minimize_window()))
        .on_action(cx.listener(|_, _: &Zoom, window, _| window.zoom_window()))
}

#[cfg(target_os = "macos")]
pub(crate) fn set_native_menus(cx: &mut App) {
    use gpui::{Menu, MenuItem};
    let convert = |e: Entry| match e {
        Entry::Separator => MenuItem::separator(),
        Entry::Item { label, action: Some(action), .. } => {
            MenuItem::Action { name: label.into(), action, os_action: None, checked: false, disabled: false }
        }
        Entry::Item { label, action: None, .. } => MenuItem::action(label, gpui::NoAction).disabled(true),
    };
    cx.set_menus(menus().into_iter().map(|m| Menu::new(m.name).items(m.entries.into_iter().map(convert))).collect::<Vec<_>>());
}

/// Windows kennt in GPUI kein App-Menü, `ui::menubar` zeichnet es.
#[cfg(not(target_os = "macos"))]
pub(crate) fn set_native_menus(_cx: &mut App) {}
