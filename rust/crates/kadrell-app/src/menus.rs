//! Menüstruktur nach `buildMenu()` der Mac-App (Kadrell, Datei, Bearbeiten, Ansicht, Session, Fenster).
//! macOS setzt daraus die System-Menüleiste, Windows zeichnet sie selbst (`ui::menubar`), denn GPUI zeigt dort
//! kein App-Menü. Nicht belegte Einträge stehen ausgegraut da, damit die Struktur der Mac-App sichtbar bleibt.
//! Beschriftungen und Kürzel entstehen beim Bauen in der eingestellten Sprache; nach einem Sprachwechsel baut
//! `Shell::sync_language` die Menüs neu (`menus()` ist billig).
//!
//! Zwei Fassungen: `menus_for(&MenuContext::live())` ist das echte Menü (Ansicht mit Layouts und allen belegbaren
//! Kürzeln, Session-Menü mit Zustand, Untermenüs); `menus()` ist die flache Fassung ohne Zustand, aus der die Palette
//! ihre Kommandos holt.

use gpui::{Action, App, Context, Div, InteractiveElement, KeyBinding, actions};
use kadrell_core::hotkeys::HotkeyAction;
use kadrell_i18n::trd;

use crate::Shell;
use crate::lang::{de, shortcut, shortcut_mac_only, shortcut_os};
use crate::menu_ctx::{MenuContext, MoveToGroup, PerformHotkey, SelectLayout, hotkey_actions, hotkey_title, layout_title};
use crate::misc::{OpenAbout, OpenWhatsNew};
use crate::ui::context_menu::Cmd;
use crate::ui::ext::{FocusRightSidebar, OpenExtensions, ToggleRightSidebar};
use crate::ui::layout::LayoutMode;
use crate::ui::text_input::{Copy, Cut, Paste};
use crate::ui::tile_context::{FindInTerminal, FindNext, FindPrevious};

actions!(kadrell, [NewSession, NewSessionHere, NewTerminal, CloseSession, StopSession, ResumeSession, RenameSession, RenameGroup, ZoomTile, NextSession, PrevSession, NextLayout, MoreColumns, FewerColumns, ToggleTheme, Minimize, Zoom, Quit, OpenPalette, FindAll, OpenSettings, OpenStats, QuickStart, SearchHistory, CloseWindow, NewWindow, NewGroup, ShowContextMenu, NewRemote]);

// Aktionen der App-Hülle: Neustart, Kommandozeile, Wegwerf-Instanz, Ausblenden, Auswahl, Auto-Modus, Session-Menü.
actions!(kadrell_shell, [RestartApp, InstallCli, TemporaryInstance, HideApp, SelectGroups, SelectEverything, IsolateSession, RemoveFromView, NewSessionBeside, CopyPath, ShowInFolder]);

pub enum Entry {
    Separator,
    /// `key` ist der deutsche Quelltext (stabil, die Palette sucht Einträge darüber), `label` der Text in der
    /// eingestellten Sprache.
    Item { key: &'static str, label: String, shortcut: String, action: Option<Box<dyn Action>> },
    /// Untermenü („Verschieben nach“, „Kachel wählen“). Die Palette übernimmt nur `Item`.
    Sub { label: String, entries: Vec<Entry> },
    /// Wahlpunkt mit Farbpunkt und Haken (Gruppen in „Verschieben nach“); ohne Aktion ausgegraut.
    Choice { label: String, dot: Option<u32>, checked: bool, action: Option<Box<dyn Action>> },
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

/// Eintrag, der nur wirkt, wenn `enabled` gilt (im echten Menü; die flache Fassung für die Palette ist immer an).
fn gated(ctx: &MenuContext, enabled: bool, key: &'static str, shortcut: String, action: impl Action) -> Entry {
    if !ctx.live || enabled { item(key, shortcut, action) } else { off(key, shortcut) }
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
            item(de!("Über Kadrell"), crate::Shell::about_shortcut(), OpenAbout),
            item(de!("Was ist neu"), none(), OpenWhatsNew),
            item(de!("Statistik"), shortcut("⌥F3"), OpenStats),
            item(de!("Extensions …"), shortcut_os("⌥F4", "⇧⌥E"), OpenExtensions),
            Entry::Separator,
            item(de!("Einstellungen …"), shortcut("⌘,"), OpenSettings),
            item(de!("Kommandozeilen-Tool installieren …"), none(), InstallCli),
            item(de!("Neue Instanz mit temporärem Profil"), none(), TemporaryInstance),
            Entry::Separator,
            item(de!("Kadrell ausblenden"), shortcut_mac_only("⌘H"), HideApp),
            Entry::Separator,
            item(de!("Kadrell neu starten"), none(), RestartApp),
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
            // Textfelder behalten unter Windows Strg (Strg+X im Textfeld), im Terminal ist Strg+C der Abbruch: Kopieren
            // und Einsetzen im Terminal liegen dort auf Alt+C und Alt+V (⌘ ist Alt).
            item(de!("Ausschneiden"), shortcut_os("⌘X", "⌃X"), Cut),
            item(de!("Kopieren"), shortcut("⌘C"), Copy),
            item(de!("Einsetzen"), shortcut("⌘V"), Paste),
            item(de!("Ganze Gruppen auswählen"), shortcut_mac_only("⌘A"), SelectGroups),
            item(de!("Alle Sessions auswählen"), shortcut("⇧⌘A"), SelectEverything),
            Entry::Separator,
            item(de!("Im Terminal suchen …"), shortcut("⌘F"), FindInTerminal),
            item(de!("Weitersuchen"), shortcut("⌘G"), FindNext),
            item(de!("Rückwärts suchen"), shortcut("⇧⌘G"), FindPrevious),
            item(de!("In allen Terminals suchen …"), shortcut("⇧⌘F"), FindAll),
            item(de!("Alle Claude-Sessions durchsuchen, auch beendete"), shortcut("⇧⌘H"), SearchHistory),
        ],
    )
}

/// Menü „Ansicht“ der flachen Fassung (Palette): wie vor dem Umbau, mit eigenen Einträgen für Kürzel.
fn flat_view_menu() -> MenuDef {
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
            item(de!("Farbschema wechseln"), shortcut_os("⌥T", "⌥⌘T"), ToggleTheme),
            Entry::Separator,
            item(de!("Suche"), shortcut("⌘P"), OpenPalette),
        ],
    )
}

/// Ein Eintrag je Layout-Modus (Swift: `LayoutMode.allCases`), ohne Kürzel.
fn layout_entries() -> Vec<Entry> {
    LayoutMode::ALL.iter().map(|m| item(layout_title(*m), none(), SelectLayout { mode: m.key().to_string() })).collect()
}

/// Die belegbaren Kürzel als Einträge mit der aktuellen Belegung; die Kacheln 1 bis 9 stehen im Untermenü
/// „Kachel wählen“ hinter `lastSession` (Swift: Schleife über `HotkeyAction.allCases` ohne Panels).
fn hotkey_entries(ctx: &MenuContext) -> Vec<Entry> {
    let one = |a: HotkeyAction| Entry::Item {
        key: a.raw(),
        label: hotkey_title(a),
        shortcut: ctx.binding(a),
        action: Some(Box::new(PerformHotkey { action: a.raw().to_string() })),
    };
    let tiles: Vec<Entry> = hotkey_actions().into_iter().filter(|(_, tile)| *tile).map(|(a, _)| one(a)).collect();
    let mut out = Vec::new();
    for (action, tile) in hotkey_actions() {
        if tile {
            continue;
        }
        out.push(one(action));
        if action == HotkeyAction::LastSession {
            out.push(Entry::Sub { label: trd!("Kachel wählen"), entries: Vec::new() });
        }
    }
    if let Some(Entry::Sub { entries, .. }) = out.iter_mut().find(|e| matches!(e, Entry::Sub { .. })) {
        *entries = tiles;
    }
    out
}

fn live_view_menu(ctx: &MenuContext) -> MenuDef {
    let mut entries = layout_entries();
    entries.extend([
        item(de!("Auto-Modus ein/aus"), none(), crate::windows::ToggleAuto),
        item(de!("Baum ein/aus"), shortcut("⌘B"), crate::windows::ToggleSidebar),
        item(de!("Alle Gruppen auf-/zuklappen"), shortcut("⇧⌘B"), crate::ui::tree_actions::ToggleAllGroups),
        Entry::Separator,
        item(de!("Terminal-Schrift größer"), shortcut("⌘+"), crate::appearance::zoom::FontBigger),
        item(de!("Terminal-Schrift kleiner"), shortcut("⌘-"), crate::appearance::zoom::FontSmaller),
        item(de!("Terminal-Schrift Standardgröße"), shortcut("⌘0"), crate::appearance::zoom::FontReset),
        Entry::Separator,
    ]);
    entries.extend(hotkey_entries(ctx));
    entries.extend([Entry::Separator, item(de!("Suche"), shortcut("⌘P"), OpenPalette)]);
    menu(de!("Ansicht"), entries)
}

fn view_menu(ctx: &MenuContext) -> MenuDef {
    if ctx.live { live_view_menu(ctx) } else { flat_view_menu() }
}

/// „Verschieben nach ▸“: alle Gruppen mit Farbpunkt, die aktuelle abgehakt und aus; ohne Session ein grauer Hinweis.
fn move_entries(ctx: &MenuContext) -> Vec<Entry> {
    if ctx.live && ctx.session.is_none() {
        return vec![off(de!("Keine Session fokussiert"), none())];
    }
    ctx.moves
        .iter()
        .map(|t| Entry::Choice {
            label: t.label.clone(),
            dot: Some(t.color),
            checked: t.here,
            action: (!t.here).then(|| Box::new(MoveToGroup { group: t.id.clone() }) as Box<dyn Action>),
        })
        .collect()
}

fn session_menu(ctx: &MenuContext) -> MenuDef {
    let flags = ctx.session.clone().unwrap_or_default();
    let focused = ctx.session.is_some();
    let reveal = if cfg!(target_os = "macos") { de!("Im Finder zeigen") } else { de!("Im Explorer zeigen") };
    menu(
        de!("Session"),
        vec![
            gated(ctx, focused && !flags.attached, de!("Fortsetzen"), shortcut_mac_only("⌘R"), ResumeSession),
            gated(ctx, flags.attached, de!("Stoppen"), shortcut_mac_only("⌘."), StopSession),
            gated(ctx, focused, de!("Umbenennen"), "F2".to_string(), RenameSession),
            gated(ctx, focused, de!("Gruppe umbenennen"), none(), RenameGroup),
            gated(ctx, focused, de!("Entfernen …"), none(), CloseSession),
            Entry::Separator,
            gated(ctx, focused, de!("Nur diese zeigen"), none(), IsolateSession),
            gated(ctx, flags.shown, de!("Aus Ansicht nehmen"), none(), RemoveFromView),
            gated(ctx, flags.has_cwd, de!("Neue Session hier"), none(), NewSessionBeside),
            Entry::Sub { label: trd!("Verschieben nach"), entries: move_entries(ctx) },
            Entry::Separator,
            gated(ctx, flags.has_cwd, de!("Pfad kopieren"), none(), CopyPath),
            gated(ctx, flags.has_cwd, reveal, none(), ShowInFolder),
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

/// Das Menü mit Zustand (Fokus, Belegung der Kürzel, Gruppen).
pub fn menus_for(ctx: &MenuContext) -> Vec<MenuDef> {
    vec![app_menu(), file_menu(), edit_menu(), view_menu(ctx), session_menu(ctx), window_menu()]
}

/// Die flache Fassung ohne Zustand: alle Einträge an, für die Kommandos der Palette.
pub fn menus() -> Vec<MenuDef> {
    menus_for(&MenuContext::default())
}

/// Tastenbelegungen und globale Aktionen. Hier stehen die Mac-Tastenstrings, `keys::platform` macht daraus unter
/// Windows Alt (⌘) bzw. Strg+Alt (⌘⌥, AltGr-freie Buchstaben). Wer eine Bindung dazunimmt, trägt sie in `keys::FIXED` ein.
/// Alt+N/P (Mac ⌥N/⌥P) laufen über die Belegung aus den Einstellungen (`workspace::keys`), nicht hier. Schnellstart,
/// Verlaufssuche, Statistik und Neustart folgen ebenfalls den Einstellungen (`Shell::open_hotkey_key`).
pub fn init(cx: &mut App) {
    use crate::keys::platform as key;
    cx.bind_keys([KeyBinding::new(key("cmd-n"), NewSession, None), KeyBinding::new(key("cmd-shift-n"), NewRemote, None)]);
    cx.bind_keys([
        KeyBinding::new(key("cmd-enter"), NewSessionHere, None),
        KeyBinding::new(key("cmd-t"), NewTerminal, None),
        KeyBinding::new(key("cmd-shift-enter"), ZoomTile, None),
        KeyBinding::new("f2", RenameSession, None),
        KeyBinding::new("shift-f10", ShowContextMenu, None),
    ]);
    // ⌥⌘N der Mac-App: neue Gruppe.
    cx.bind_keys([KeyBinding::new(key("cmd-alt-n"), NewGroup, None)]);
    cx.bind_keys([KeyBinding::new(key("cmd-p"), OpenPalette, None), KeyBinding::new(key("cmd-shift-f"), FindAll, None)]);
    cx.bind_keys([KeyBinding::new(key("cmd-,"), OpenSettings, None)]);
    bind_selection_keys(cx);
    crate::ui::tile_context::bind_keys(cx);
    // Alt+F4 und der rote Knopf verstecken nur das Fenster (`tray_wire`), beendet wird mit Alt+Q, über das Menü oder das Tray.
    cx.bind_keys([
        KeyBinding::new(key("cmd-q"), Quit, None),
        KeyBinding::new(key("cmd-shift-w"), CloseWindow, None),
        KeyBinding::new(key("cmd-shift-t"), NewWindow, None),
    ]);
    cx.on_action(|_: &Quit, cx| crate::quit::request(cx));
    cx.on_action(|_: &RestartApp, cx| crate::quit::request_restart(cx));
    cx.on_action(|_: &ToggleTheme, cx| Shell::toggle_theme(cx));
    crate::windows::bind_keys(cx);
    set_native_menus(cx);
}

/// Kürzel der Auswahl und des Session-Menüs. Auf dem Mac ⌘A, ⇧⌘A, ⌘H, ⌘R und ⌘. wie in der Mac-App; unter Windows
/// gilt nur Alt+Umschalt+A (Alles auswählen), ⌘A, ⌘H, ⌘R und ⌘. bleiben ohne Belegung (Strg+A/R sind Textfeld bzw.
/// Verlaufssuche in Claude, Alt+H/R/. gibt es in der Windows-Fassung nicht).
fn bind_selection_keys(cx: &mut App) {
    if cfg!(target_os = "macos") {
        cx.bind_keys([
            KeyBinding::new("cmd-a", SelectGroups, None),
            KeyBinding::new("cmd-shift-a", SelectEverything, None),
            KeyBinding::new("cmd-h", HideApp, None),
            KeyBinding::new("cmd-r", ResumeSession, None),
            KeyBinding::new("cmd-.", StopSession, None),
        ]);
    } else {
        cx.bind_keys([KeyBinding::new(crate::keys::platform("cmd-shift-a"), SelectEverything, None)]);
    }
}

/// Aktionen des Fensters: wirken auf die Shell und laufen über den Fokuspfad (Menü, Tastatur).
pub fn attach_actions(root: Div, cx: &mut Context<Shell>) -> Div {
    let root = crate::windows::attach_actions(crate::appearance::zoom::attach_actions(root, cx), cx);
    let root = crate::ui::tree_actions::attach_actions(crate::ui::tile_context::attach_actions(root, cx), cx);
    let root = attach_session_actions(attach_shell_actions(root, cx), cx);
    root.on_action(cx.listener(|this, _: &NewSession, window, cx| this.new_session_action(window, cx)))
        .on_action(cx.listener(|this, _: &OpenPalette, window, cx| this.open_palette("", window, cx)))
        .on_action(cx.listener(|this, _: &NewRemote, window, cx| this.open_palette("@", window, cx)))
        .on_action(cx.listener(|this, _: &FindAll, window, cx| this.open_palette("/", window, cx)))
        .on_action(cx.listener(|this, _: &SearchHistory, window, cx| this.open_palette("#", window, cx)))
        .on_action(cx.listener(|this, _: &QuickStart, window, cx| this.open_quick_start(window, cx)))
        .on_action(cx.listener(|this, _: &NewSessionHere, window, cx| this.new_session_here(window, cx)))
        .on_action(cx.listener(|this, _: &NewTerminal, window, cx| this.new_terminal(window, cx)))
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

/// App-Menü, Auswahl, Layouts, Kürzel und Einsetzen.
fn attach_shell_actions(root: Div, cx: &mut Context<Shell>) -> Div {
    root.on_action(cx.listener(|this, _: &InstallCli, window, cx| this.install_cli_action(window, cx)))
        .on_action(cx.listener(|this, _: &TemporaryInstance, _, _| this.temporary_instance_action()))
        .on_action(cx.listener(|this, _: &HideApp, window, cx| this.hide_app(window, cx)))
        .on_action(cx.listener(|this, _: &SelectGroups, window, cx| this.select_groups_action(window, cx)))
        .on_action(cx.listener(|this, _: &SelectEverything, window, cx| this.select_everything_action(window, cx)))
        .on_action(cx.listener(|this, a: &SelectLayout, _, cx| this.select_layout_action(&a.mode, cx)))
        .on_action(cx.listener(|this, a: &PerformHotkey, window, cx| this.perform_hotkey_action(&a.action, window, cx)))
        .on_action(cx.listener(|this, _: &Paste, _, cx| this.paste_into_focused(cx)))
}

/// Das Menü „Session“: wirkt auf die fokussierte Session.
fn attach_session_actions(root: Div, cx: &mut Context<Shell>) -> Div {
    root.on_action(cx.listener(|this, _: &CloseSession, window, cx| this.remove_focused(window, cx)))
        .on_action(cx.listener(|this, _: &StopSession, window, cx| this.stop_focused(window, cx)))
        .on_action(cx.listener(|this, _: &ResumeSession, window, cx| this.resume_focused(window, cx)))
        .on_action(cx.listener(|this, _: &RenameSession, window, cx| this.rename_focused(window, cx)))
        .on_action(cx.listener(|this, _: &RenameGroup, window, cx| this.edit_focused_group(window, cx)))
        .on_action(cx.listener(|this, _: &IsolateSession, window, cx| this.focused_cmd(|key, _| Cmd::ShowOnly(key), window, cx)))
        .on_action(cx.listener(|this, _: &RemoveFromView, window, cx| this.focused_cmd(|key, _| Cmd::HideFromView(key), window, cx)))
        .on_action(cx.listener(|this, _: &NewSessionBeside, window, cx| this.focused_cmd(|key, _| Cmd::NewSessionHere(key), window, cx)))
        .on_action(cx.listener(|this, _: &CopyPath, window, cx| this.focused_cmd(|_, cwd| Cmd::CopyPath(cwd), window, cx)))
        .on_action(cx.listener(|this, _: &ShowInFolder, window, cx| this.focused_cmd(|_, cwd| Cmd::Reveal(cwd), window, cx)))
        .on_action(cx.listener(|this, a: &MoveToGroup, window, cx| this.move_focused_to_group(&a.group, window, cx)))
}

#[cfg(target_os = "macos")]
fn native_items(entries: Vec<Entry>) -> Vec<gpui::MenuItem> {
    use gpui::{Menu, MenuItem};
    entries
        .into_iter()
        .map(|e| match e {
            Entry::Separator => MenuItem::separator(),
            Entry::Item { label, action: Some(action), .. } => {
                MenuItem::Action { name: label.into(), action, os_action: None, checked: false, disabled: false }
            }
            Entry::Item { label, action: None, .. } => MenuItem::action(label, gpui::NoAction).disabled(true),
            Entry::Sub { label, entries } => MenuItem::submenu(Menu::new(label).items(native_items(entries))),
            // Der Farbpunkt der Gruppe fehlt im System-Menü (GPUI kennt keine Bilder in Menüs).
            Entry::Choice { label, checked, action: Some(action), .. } => {
                MenuItem::Action { name: label.into(), action, os_action: None, checked, disabled: false }
            }
            Entry::Choice { label, checked, action: None, .. } => MenuItem::action(label, gpui::NoAction).checked(checked).disabled(true),
        })
        .collect()
}

#[cfg(target_os = "macos")]
pub(crate) fn set_native_menus_for(cx: &mut App, ctx: &MenuContext) {
    use gpui::Menu;
    cx.set_menus(menus_for(ctx).into_iter().map(|m| Menu::new(m.name).items(native_items(m.entries))).collect::<Vec<_>>());
}

/// Windows kennt in GPUI kein App-Menü, `ui::menubar` zeichnet es.
#[cfg(not(target_os = "macos"))]
pub(crate) fn set_native_menus_for(_cx: &mut App, _ctx: &MenuContext) {}

/// System-Menü mit der Grundfassung (Start, Sprachwechsel); `Shell::sync_menus` ersetzt sie bei jeder Änderung.
pub(crate) fn set_native_menus(cx: &mut App) {
    set_native_menus_for(cx, &MenuContext::live());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(entries: &[Entry]) -> Vec<String> {
        entries
            .iter()
            .map(|e| match e {
                Entry::Separator => "-".to_string(),
                Entry::Item { key, action, .. } => format!("{key}{}", if action.is_some() { "" } else { " (aus)" }),
                Entry::Sub { label, .. } => format!("{label} >"),
                Entry::Choice { label, .. } => label.clone(),
            })
            .collect()
    }

    fn menu_named(ctx: &MenuContext, key: &str) -> Vec<Entry> {
        menus_for(ctx).into_iter().find(|m| m.key == key).unwrap().entries
    }

    #[test]
    fn app_menu_has_the_mac_apps_entries_in_order() {
        let entries = menu_named(&MenuContext::live(), "Kadrell");
        assert_eq!(
            labels(&entries),
            [
                "Über Kadrell", "Was ist neu", "Statistik", "Extensions …", "-", "Einstellungen …", "Kommandozeilen-Tool installieren …",
                "Neue Instanz mit temporärem Profil", "-", "Kadrell ausblenden", "-", "Kadrell neu starten", "Kadrell beenden",
            ]
        );
    }

    #[test]
    fn session_items_follow_the_focused_session() {
        let ctx = MenuContext { live: true, ..MenuContext::default() };
        let all_off = labels(&menu_named(&ctx, "Session"));
        assert!(all_off.iter().filter(|l| l.ends_with("(aus)")).count() >= 9, "ohne Fokus ist alles aus: {all_off:?}");
        let stopped = MenuContext { session: Some(Default::default()), ..ctx.clone() };
        let l = labels(&menu_named(&stopped, "Session"));
        assert!(!l.contains(&"Fortsetzen (aus)".to_string()) && l.contains(&"Stoppen (aus)".to_string()), "{l:?}");
        let running = MenuContext { session: Some(crate::menu_ctx::SessionFlags { attached: true, shown: true, has_cwd: true }), ..ctx };
        let l = labels(&menu_named(&running, "Session"));
        assert!(l.contains(&"Fortsetzen (aus)".to_string()) && !l.contains(&"Stoppen (aus)".to_string()), "{l:?}");
        assert!(!l.contains(&"Aus Ansicht nehmen (aus)".to_string()) && !l.contains(&"Pfad kopieren (aus)".to_string()), "{l:?}");
    }

    #[test]
    fn the_flat_palette_version_has_everything_on() {
        let l = labels(&menu_named(&MenuContext::default(), "Session"));
        assert!(l.iter().all(|e| !e.ends_with("(aus)")), "{l:?}");
        assert!(labels(&menu_named(&MenuContext::default(), "Ansicht")).contains(&"Kachel zoomen".to_string()));
    }

    #[test]
    fn move_submenu_lists_groups_with_a_check_and_a_hint_without_session() {
        let none = MenuContext::live();
        let Some(Entry::Sub { entries, .. }) = menu_named(&none, "Session").into_iter().find(|e| matches!(e, Entry::Sub { .. })) else { panic!("kein Untermenü") };
        assert_eq!(labels(&entries), ["Keine Session fokussiert (aus)"]);
        let ctx = MenuContext {
            live: true,
            session: Some(Default::default()),
            moves: vec![
                crate::menu_ctx::MoveTarget { id: "g1".into(), label: "kadrell".into(), color: 1, here: true },
                crate::menu_ctx::MoveTarget { id: "g2".into(), label: "spike".into(), color: 2, here: false },
            ],
            ..MenuContext::default()
        };
        let entries = move_entries(&ctx);
        assert!(matches!(&entries[0], Entry::Choice { checked: true, action: None, .. }), "die Gruppe, in der die Session steht, ist abgehakt und aus");
        assert!(matches!(&entries[1], Entry::Choice { checked: false, action: Some(_), dot: Some(2), .. }));
    }

    #[test]
    fn the_view_menu_lists_layouts_first_and_the_tile_submenu_behind_last_session() {
        let ctx = MenuContext::live();
        let entries = menu_named(&ctx, "Ansicht");
        let l = labels(&entries);
        assert_eq!(l[..7], ["Grid", "Haupt + Spalte", "Spirale", "Frei", "Scrollen", "Reihe", "Stack"]);
        assert_eq!(l[7], "Auto-Modus ein/aus");
        let last = l.iter().position(|e| e == "lastSession").unwrap();
        assert_eq!(l[last + 1], "Kachel wählen >");
        let Some(Entry::Sub { entries: tiles, .. }) = entries.into_iter().find(|e| matches!(e, Entry::Sub { .. })) else { panic!() };
        assert_eq!(tiles.len(), 9);
        assert!(!l.iter().any(|e| e == "showStats" || e == "showHelp" || e == "showExtensions"), "Panels stehen im App-Menü");
        assert_eq!(l.last().map(String::as_str), Some("Suche"));
    }

    #[test]
    fn hotkey_items_show_the_current_binding() {
        let ctx = MenuContext { live: true, bindings: vec![(HotkeyAction::Zoom, "Alt+Z".into())], ..MenuContext::default() };
        let entries = menu_named(&ctx, "Ansicht");
        let zoom = entries.iter().find_map(|e| match e {
            Entry::Item { key: "zoom", shortcut, .. } => Some(shortcut.clone()),
            _ => None,
        });
        assert_eq!(zoom.as_deref(), Some("Alt+Z"));
    }

    #[test]
    fn edit_menu_has_clipboard_selection_and_search_items() {
        let l = labels(&menu_named(&MenuContext::live(), "Bearbeiten"));
        assert_eq!(l[..5], ["Ausschneiden", "Kopieren", "Einsetzen", "Ganze Gruppen auswählen", "Alle Sessions auswählen"]);
        for name in ["Im Terminal suchen …", "Weitersuchen", "Rückwärts suchen"] {
            assert!(l.contains(&name.to_string()), "{name} ist belegt und nicht ausgegraut");
        }
    }
}
