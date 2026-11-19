//! Kadrell-Fenster mit Baum, Leiste und Terminal-Kacheln (GPUI), verdrahtet mit dem Zustand (`state`): echte Sessions,
//! Steuerkanal, Status. Der Einstieg ist [`run`].
#![allow(dead_code)] // Theme: nicht alle Felder werden schon benutzt.

mod appearance;
mod bar_wire;
mod cli_wire;
mod ext;
mod instance;
mod keys;
mod models;
mod quit;
mod remote;
mod lang;
#[cfg(test)]
mod catalog_test;
mod menu_ctx;
mod menus;
mod misc;
mod signals;
mod state;
mod theme;
mod tray_wire;
mod ui;
mod usage;
mod windows;
mod workspace;
mod wire;

use std::collections::HashMap;

use gpui::{
    App, Context, Entity, FocusHandle, Focusable, Global, IntoElement, KeyDownEvent, MouseDownEvent, Render, Window, WindowBounds,
    WindowOptions, div, prelude::*, px, size,
};
use gpui_platform::application;

use kadrell_control::ControlServer;
use kadrell_ext::manager::ExtensionManager;
use kadrell_control::endpoint::endpoint_path;
use kadrell_core::profile::Acquire;
use kadrell_core::{Paths, Profile, SessionStatus, SidebarSort};
use kadrell_term::{TermThemeGlobal, TerminalPane};
use state::launch::LaunchEnv;
use state::{AppState, Parts};
use theme::Theme;
use ui::layout::{Id, LayoutMode, LayoutState, Selection};
use ui::model::{Group, flat_index};

pub struct ThemeGlobal(pub Theme);
impl Global for ThemeGlobal {}

/// Setzt das Farbschema der App und gibt dem Terminal die Farben, die es braucht.
fn set_theme(cx: &mut App, theme: Theme) {
    cx.set_global(TermThemeGlobal(theme.term()));
    cx.set_global(ThemeGlobal(theme));
    appearance::set_term_alpha(cx, appearance::current(cx).opacity);
}

pub struct Shell {
    /// Zustand der App: Sessions, Gruppen, Auswahl, Kacheln, Status. Die Felder darunter sind Ansicht davon und
    /// werden nach jeder Änderung neu gebaut (`rebuild`).
    state: AppState,
    groups: Vec<Group>,
    /// Terminal-Oberflächen je Session-Schlüssel.
    panes: HashMap<String, Entity<TerminalPane>>,
    /// Fokus-Kachel (Tastaturziel), immer Teil der Auswahl. `None` ohne Sessions.
    selected: Option<Id>,
    /// Sichtbare Sessions (Kacheln), Anker für Shift-Klick.
    selection: Selection,
    layout: LayoutState,
    clock: String,
    menubar: Entity<ui::menubar::MenuBar>,
    overlay: Option<Entity<ui::new_session::NewSession>>,
    palette: Option<Entity<ui::palette::Palette>>,
    /// Schlüssel der Sessions, die die offene Suche als Treffer zeigt (`ui::palette`, `palette_highlights`).
    palette_highlight: Option<std::collections::HashSet<String>>,
    quick_start: Option<Entity<ui::quick_start::QuickStart>>,
    /// Auswahl „Starten mit“ (Claude oder Pi) und Dialog „Remote verbinden“ (`models::agent_pick`, `remote::dialog`).
    agent_pick: Option<Entity<models::agent_pick::AgentPick>>,
    remote: Option<Entity<remote::dialog::RemoteDialog>>,
    /// Index der Verlaufssuche (`#`), beim Start und bei jedem Öffnen der Suche nachgelesen.
    history: ui::palette::HistoryStore,
    settings: Option<Entity<ui::settings::SettingsDialog>>,
    rename: Option<Entity<ui::rename::Rename>>,
    context: Option<ui::context_menu::ContextMenu>,
    /// Gruppen-Dialoge, Rückfragen und Zustand des Ziehens im Baum (`ui::groups`).
    groups_ui: ui::groups::GroupsUi,
    /// Tastaturziel, solange keine Kachel da ist.
    idle_focus: FocusHandle,
    /// Der Steuerkanal; mit der Shell lebt und stirbt er.
    server: Option<ControlServer>,
    down: bool,
    /// Beenden und Neustart (`quit`): Rückfrage beantwortet, Neustart gewünscht.
    quit: quit::QuitState,
    /// Zustand der Menüs: letzter Schnappschuss, Zurücknehmen von ⌘A (`menu_ctx`).
    menu: menu_ctx::MenuState,
    /// Die Lua-Extensions dieses Profils.
    ext: ExtensionManager,
    /// Tray-Icon, Benachrichtigungen, Badge und das Verstecken des Fensters (`tray_wire`).
    tray: tray_wire::TrayWire,
    /// Stand der Sprache, für die Menüs und die Ansicht zuletzt gebaut wurden (`lang::sync_language`).
    language_generation: u64,
    /// Farbschema, das die Titelleiste zuletzt bekam (`appearance::apply`); `None` vor dem ersten Zeichnen.
    window_dark: Option<bool>,
    /// Oberfläche der Extensions: Seitenleisten, Statusleisten, Dialog (`ui::ext`).
    ext_ui: ui::ext::ExtUi,
    /// Letzter Stand der Sessions für die Extension-Events `session.new/status/remove` (`ext::SessionTracker`).
    ext_sessions: ext::SessionTracker,
    /// Konten, Nutzung, Meldungen und die Statistik (`usage`).
    usage: usage::UsageUi,
    /// Hilfe, Über, Rückfragen, Update-Pille, Leerzustand, Übernahme, Auto-Trennen (`misc`).
    misc: misc::Misc,
    /// Weitere Fenster (⌘⇧T): offene Nummern, was jedes zeigt, wer welches Terminal hält (`windows`).
    windows: windows::model::WindowSet,
    shown: HashMap<usize, Vec<String>>,
    owners: windows::model::PaneOwners,
    extras: Vec<(usize, gpui::AnyWindowHandle)>,
    main_window: Option<gpui::AnyWindowHandle>,
    /// Aufteilung des Hauptfensters (Baum, Dock, rechte Seitenleiste) und das zuletzt aktive Fenster (`windows`).
    win: windows::WinState,
    /// Zoom-, Sync-, Vorschau-, Auto- und Baum-Zustand der Arbeitsfläche samt Tabelle der Kürzel (`workspace`).
    ws: workspace::Ws,
    /// Aufblitzen und Einfahren im Baum (`ui::tree_anim`).
    tree_anim: ui::tree_anim::TreeAnim,
    /// Overlay-Verwaltung: vorgemerkter Dialog, ausfahrendes Overlay (`ui::overlay_queue`).
    overlays: ui::overlay_queue::OverlayState,
}

/// Start-Auswahl für Screenshots ohne Eingabe: `KADRELL_DEV_SELECT="0,0;0,1;1,0"` (Positionen im Baum).
fn dev_selection(groups: &[Group]) -> Vec<String> {
    std::env::var("KADRELL_DEV_SELECT").map(|v| wire::keys_at(groups, &v)).unwrap_or_default()
}

/// Start-Layout: `KADRELL_DEV_LAYOUT=grid|main|spiral|stack`.
fn start_layout() -> Option<LayoutMode> {
    std::env::var("KADRELL_DEV_LAYOUT").ok().and_then(|v| LayoutMode::from_name(&v))
}

fn now_hhmm() -> String {
    chrono::Local::now().format("%H:%M").to_string()
}

/// Layout der Oberfläche aus dem Layout des Steuerkanals (`kadrell layout`): was die Oberfläche nicht kennt, gilt als Grid.
fn ui_layout(mode: kadrell_control::LayoutMode) -> LayoutMode {
    LayoutMode::from_name(mode.as_str()).unwrap_or(LayoutMode::Grid)
}

fn control_layout(mode: LayoutMode) -> kadrell_control::LayoutMode {
    kadrell_control::LayoutMode::from_raw(mode.key()).unwrap_or_default()
}

impl Shell {
    fn new(window: &mut Window, cx: &mut Context<Self>, mut state: AppState, jobs: async_channel::Receiver<wire::Job>, server: Option<ControlServer>, ext: ExtensionManager) -> Self {
        ui::groups::dev_seed(&mut state);
        models::install(cx, &state.models);
        let menubar = cx.new(ui::menubar::MenuBar::new);
        let mode = ui_layout(state.layout);
        let folder_home = state.home();
        let ext_ui = ui::ext::ExtUi::new(&ext, cx);
        let usage = usage::UsageUi::new(&state);
        let ws = workspace::Ws::new(&state.settings);
        let win = windows::WinState::new(&state.settings);
        let layout = LayoutState::from_settings(mode, &state.settings);
        let mut shell = Shell {
            state,
            groups: Vec::new(),
            panes: HashMap::new(),
            selected: None,
            overlays: Default::default(),
            selection: Selection { items: Vec::new(), focus: (0, 0), anchor: (0, 0) },
            layout,
            clock: now_hhmm(),
            menubar,
            overlay: None,
            palette: None,
            palette_highlight: None,
            quick_start: None,
            agent_pick: None,
            remote: None,
            history: ui::palette::HistoryStore::new(&folder_home),
            settings: None,
            rename: None,
            context: None,
            groups_ui: ui::groups::GroupsUi::new(),
            idle_focus: cx.focus_handle(),
            server,
            down: false,
            quit: quit::QuitState::default(),
            menu: menu_ctx::MenuState::default(),
            ext,
            tray: tray_wire::TrayWire::new(),
            language_generation: kadrell_i18n::global().generation(),
            window_dark: None,
            ext_ui,
            ext_sessions: ext::SessionTracker::default(),
            usage,
            misc: misc::Misc::new(),
            windows: windows::model::WindowSet::default(),
            shown: HashMap::new(),
            owners: windows::model::PaneOwners::default(),
            extras: Vec::new(),
            main_window: None,
            win,
            ws,
            tree_anim: Default::default(),
        };
        shell.rebuild();
        shell.apply_dev_view();
        shell.apply_dev_preview();
        shell.apply_dev_tree_focus();
        shell.state.ready = true;
        shell.sync(window, cx);
        shell.start_clock(cx);
        shell.start_jobs(jobs, window, cx);
        shell.start_poll(window, cx);
        shell.start_auto_tick(window, cx);
        shell.start_usage(window, cx);
        shell.open_menu_from_env(window, cx);
        shell.open_overlay_from_env(window, cx);
        shell.open_palette_from_env(window, cx);
        shell.open_quick_start_from_env(window, cx);
        shell.open_agent_pick_from_env(window, cx);
        shell.open_remote_from_env(window, cx);
        shell.open_settings_from_env(window, cx);
        shell.open_groups_from_env(window, cx);
        shell.start_ext_ui(window, cx);
        shell.start_extensions();
        shell.start_tray(window, cx);
        shell.open_extensions_from_env(window, cx);
        shell.start_misc(window, cx);
        shell.start_windows(window, cx);
        shell.start_cli_check(window, cx);
        shell
    }

    /// `KADRELL_DEV_SELECT`, `KADRELL_DEV_SESSION` und `KADRELL_DEV_LAYOUT` setzen Auswahl und Layout beim Start,
    /// gegen den echten Zustand (Positionen im Baum, die es geben muss).
    /// Startet die Extensions (Katalog einlesen, eingeschaltete Helper starten); `app.ready` folgt nach dem ersten Poll.
    fn start_extensions(&mut self) {
        self.emit_ext_events();
        self.ext.start();
        self.overlays.ext_started = true;
        self.ext_dev_enable();
    }

    fn apply_dev_view(&mut self) {
        if self.state.apply_dev_tiles() {
            self.rebuild();
        }
        self.apply_workspace_dev();
        let keys = dev_selection(&self.groups);
        let focus = std::env::var("KADRELL_DEV_SESSION").ok().and_then(|v| wire::keys_at(&self.groups, &v).into_iter().next());
        if !keys.is_empty() || focus.is_some() {
            let keys = if keys.is_empty() { focus.iter().cloned().collect() } else { keys };
            self.state.set_selection(keys, focus);
        }
        if let Some(mode) = start_layout() {
            self.layout.mode = mode;
            self.state.layout = control_layout(mode);
        }
    }

    /// `KADRELL_DEV_MENU=Datei` öffnet beim Start das Menü (Screenshots ohne Eingabe).
    fn open_menu_from_env(&self, window: &mut Window, cx: &mut Context<Self>) {
        if let Ok(name) = std::env::var("KADRELL_DEV_MENU") {
            self.menubar.update(cx, |m, cx| m.open_named(&name, window, cx));
        }
    }

    /// Alt allein und Tastenfolge für die gezeichnete Menüleiste (Windows).
    fn menubar_hooks(&self, root: gpui::Div, cx: &mut Context<Self>) -> gpui::Div {
        let (on_mod, on_key) = (self.menubar.clone(), self.menubar.clone());
        root.on_modifiers_changed(move |ev, window, cx| on_mod.update(cx, |m, cx| m.on_modifiers(ev, window, cx)))
            .capture_key_down(cx.listener(move |_, _: &KeyDownEvent, _, cx| on_key.update(cx, |m, _| m.note_key())))
    }

    fn start_clock(&self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(std::time::Duration::from_secs(30)).await;
                if this.update(cx, |s, cx| s.tick(cx)).is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    fn tick(&mut self, cx: &mut Context<Self>) {
        self.clock = now_hhmm();
        cx.notify();
    }

    pub(crate) fn toggle_group(&mut self, g: usize, cx: &mut Context<Self>) {
        let id = self.groups[g].id.clone();
        self.state.toggle_collapsed(&id);
        self.rebuild();
        cx.notify();
    }

    /// Springt zu einer Session: ist sie schon sichtbar, wandert nur der Fokus, sonst zeigt sie allein.
    fn select(&mut self, g: usize, s: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.selection.contains((g, s)) {
            self.selection.focus_on((g, s));
        } else {
            self.selection = Selection::single((g, s));
            self.state.zoom = false;
        }
        self.apply_selection(window, cx);
    }

    // --- Auswahl (Kacheln, Layout und Kürzel stehen in `workspace`) ---

    /// Übernimmt `selection` in den Zustand: startet fehlende Terminals und gibt der Fokus-Kachel die Tastatur.
    /// Eine Vorschau endet damit, die Tastatur verlässt den Baum.
    fn apply_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.ws.tree_focus = false;
        self.ws.preview.clear();
        self.commit_selection(window, cx);
    }

    /// Klick im Baum: nur diese Session, Strg/Cmd nimmt sie dazu oder weg, Shift markiert den Bereich.
    fn click_session(&mut self, g: usize, s: usize, m: gpui::Modifiers, window: &mut Window, cx: &mut Context<Self>) {
        let flat = flat_index(&self.groups);
        self.selection = ui::tree_rules::click_selection(&self.selection, (g, s), m.shift, m.secondary(cx), &flat);
        self.state.zoom = false;
        self.apply_selection(window, cx);
    }

    /// Klick auf eine Gruppe: alle ihre Sessions werden sichtbar.
    fn select_group(&mut self, g: usize, window: &mut Window, cx: &mut Context<Self>) {
        let ids = (0..self.groups[g].sessions.len()).map(|s| (g, s)).collect();
        self.selection.group(ids);
        self.state.zoom = false;
        self.apply_selection(window, cx);
    }

    /// Klick auf die Gruppenzeile im Baum: ersetzt die Auswahl, mit Umschalt oder Cmd/Strg kommen die Sessions dazu.
    pub(crate) fn click_group(&mut self, g: usize, m: gpui::Modifiers, window: &mut Window, cx: &mut Context<Self>) {
        if !(m.shift || m.secondary(cx)) {
            return self.select_group(g, window, cx);
        }
        let ids = (0..self.groups[g].sessions.len()).map(|s| (g, s)).collect();
        let flat = flat_index(&self.groups);
        self.selection.add_all(ids, &flat);
        self.state.zoom = false;
        self.apply_selection(window, cx);
    }

    /// Klick in eine Kachel: sie wird Fokus, die Auswahl bleibt. Im Auto-Modus darf es eine Kachel sein, die niemand
    /// ausgewählt hat.
    fn focus_tile(&mut self, g: usize, s: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.ws.tree_focus = false;
        if self.ws.preview.is_active() {
            self.end_preview(true, window, cx);
            return;
        }
        let key = self.groups[g].sessions[s].key.clone();
        // Ein Klick ins Terminal quittiert die Marke „neu“, auch wenn es schon die Tastatur hatte.
        if self.state.attention.mark_seen(&key) {
            self.rebuild();
        }
        self.owners.take(&key, 0);
        let ended = matches!(self.state.run(&key), state::Run::Ended | state::Run::Stopped);
        if !self.selection.contains((g, s)) && self.tiles().contains(&(g, s)) {
            self.set_focus((g, s), window, cx);
        } else if self.selection.focus != (g, s) || ended {
            self.selection.focus_on((g, s));
            self.apply_selection(window, cx);
        }
    }

    fn layout_ctl(&self, cx: &mut Context<Self>) -> ui::bar::LayoutCtl {
        use std::rc::Rc;
        let running = self.state.running_count();
        ui::bar::LayoutCtl {
            mode: self.layout.mode,
            cols_label: self.layout.cols_label(),
            cols: match self.layout.mode {
                ui::layout::LayoutMode::Grid => self.layout.cols,
                ui::layout::LayoutMode::Scroll => self.layout.scroll_cols,
                _ => 0,
            },
            pick_layout: Rc::new(cx.listener(|this, ev: &MouseDownEvent, _, cx| this.open_layout_menu(ev.position, cx))),
            dec: Rc::new(cx.listener(|this, _: &MouseDownEvent, _, cx| this.step_cols(-1, cx))),
            inc: Rc::new(cx.listener(|this, _: &MouseDownEvent, _, cx| this.step_cols(1, cx))),
            split: (self.layout.mode == ui::layout::LayoutMode::Custom).then(|| self.focused_split()),
            cycle_split: Rc::new(cx.listener(|this, _: &MouseDownEvent, _, cx| this.cycle_split(cx))),
            auto: self.ws.auto.on,
            sync: self.ws.sync,
            zoom: self.state.zoom && self.tiles().len() > 1,
            toggle_auto: Rc::new(cx.listener(|this, _: &MouseDownEvent, window, cx| {
                this.play_sound(kadrell_tray::Sound::Toggle);
                this.toggle_auto(window, cx);
            })),
            toggle_sync: Rc::new(cx.listener(|this, _: &MouseDownEvent, window, cx| {
                this.play_sound(kadrell_tray::Sound::Toggle);
                this.toggle_sync(window, cx);
            })),
            toggle_zoom: Rc::new(cx.listener(|this, _: &MouseDownEvent, window, cx| this.zoom_focused(window, cx))),
            grouped: self.state.grouped(),
            sort: self.state.sort(),
            toggle_grouped: Rc::new(cx.listener(|this, _: &MouseDownEvent, _, cx| this.toggle_grouped(cx))),
            cycle_sort: Rc::new(cx.listener(|this, _: &MouseDownEvent, _, cx| this.cycle_sort(cx))),
            pressed: self.usage.pressed,
            flash: self.flash_fn(cx),
            running,
            detached: self.state.store.sessions.sessions.len().saturating_sub(running),
            waiting: self.waiting_count(),
            ext: self.ext_status(cx),
            usage: None,
        }
    }

    /// Sessions, die auf dich warten.
    fn waiting_count(&self) -> usize {
        self.state.store.sessions.sessions.iter().filter(|s| self.state.display_status(&s.id) == Some(SessionStatus::Waiting)).count()
    }

    pub(crate) fn toggle_grouped(&mut self, cx: &mut Context<Self>) {
        self.play_sound(kadrell_tray::Sound::Toggle);
        self.state.toggle_grouped();
        self.rebuild();
        cx.notify();
    }

    pub(crate) fn toggle_sort(&mut self, mode: SidebarSort, cx: &mut Context<Self>) {
        self.state.toggle_sort(mode);
        self.rebuild();
        cx.notify();
    }

    /// Springt zur n-ten Session im Baum (0-basiert).
    pub(crate) fn jump(&mut self, n: usize, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let flat = flat_index(&self.groups);
        let Some(&(g, s)) = flat.get(n) else { return false };
        self.select(g, s, window, cx);
        true
    }

    fn toggle_theme(cx: &mut App) {
        let next = if cx.global::<ThemeGlobal>().0.dark { Theme::latte() } else { Theme::mocha() };
        set_theme(cx, next);
        cx.refresh_windows();
    }

    fn on_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let m = ev.keystroke.modifiers;
        if std::env::var_os("KADRELL_DEV_KEYLOG").is_some() {
            eprintln!("key {:?}", ev.keystroke);
        }
        if ev.keystroke.key == "escape" && self.cancel_visible(window, cx) {
            cx.stop_propagation();
            return;
        }
        if self.usage_key(ev, window, cx) {
            return;
        }
        // Schnellstart, Verlaufssuche, Statistik und Neustart nach der Belegung aus den Einstellungen.
        if self.open_hotkey_key(ev, window, cx) {
            cx.stop_propagation();
            return;
        }
        // Solange der Dialog offen ist, gehören Alt-Kürzel nicht der Shell.
        if self.modal_open() && m.alt {
            cx.stop_propagation();
            return;
        }
        // Kürzel aus den Einstellungen, Vorschau, Baumtasten und Sync (`workspace`).
        if self.workspace_key(ev, window, cx) {
            cx.stop_propagation();
            return;
        }
        // ⌥T (Mac) bzw. Strg+Alt+T (Windows, Alt+T ist dort ⌘T): Farbschema. T ist auf deutscher Tastatur kein AltGr-Zeichen.
        let theme_mods = if cfg!(target_os = "macos") { m.alt && !m.control } else { m.alt && m.control };
        if theme_mods && !m.platform && !m.shift && ev.keystroke.key == "t" {
            Self::toggle_theme(cx);
            cx.stop_propagation();
        }
    }
}

impl Drop for Shell {
    fn drop(&mut self) {
        if !self.down {
            self.down = true;
            self.shutdown();
        }
    }
}

impl Focusable for Shell {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        // Im Baum und in der Vorschau gehört die Tastatur nicht dem Terminal der Fokus-Kachel.
        if self.ws.tree_focus || self.ws.preview.is_active() {
            return self.idle_focus.clone();
        }
        match self.selected.and_then(|(g, s)| self.groups.get(g)?.sessions.get(s)?.pane.as_ref()) {
            Some(p) => p.read(cx).focus.clone(),
            None => self.idle_focus.clone(),
        }
    }
}

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_language(cx);
        self.sync_menus(cx);
        appearance::window_color::ensure_native(window);
        ui::overlay::sync_area(window, cx);
        let t = ui::theme(cx);
        if self.window_dark != Some(t.dark) {
            self.window_dark = Some(t.dark);
            appearance::window_theme::apply(window, t.dark);
        }
        self.publish_tree_width(window, cx);
        let now = std::time::Instant::now();
        let transitions = self.state.take_transitions();
        self.tree_anim.sync(&self.groups, &transitions, now);
        if self.tree_anim.active(now) {
            window.request_animation_frame();
        }
        let sidebar_ctx = self.sidebar_ctx();
        let sidebar = ui::sidebar::sidebar(&self.groups, &self.selection, &sidebar_ctx, cx);
        let sidebar = self.ext_left_column(sidebar, window, cx);
        let mut ctl = self.layout_ctl(cx);
        let mut bar = self.bar_ctl(window, cx);
        bar.update = self.update_pill(cx);
        self.animate_usage(&mut bar, window);
        ctl.usage = Some(bar);
        self.note_main_shown();
        let workspace = ui::layout::workspace(self, cx);
        let root = div()
            .flex()
            .flex_col()
            .size_full()
            .bg(t.bg)
            .text_color(t.fg)
            .font_family(theme::FONT)
            .text_size(px(12.))
            .line_height(gpui::relative(ui::overlay::LINE_HEIGHT))
            .capture_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| this.on_key(ev, window, cx)));
        let root = self.menubar_hooks(misc::attach_actions(ui::ext::attach_actions(menus::attach_actions(root, cx), cx), cx), cx);
        let right = self.ext_right_panel(window, cx);
        let footer = self.ext_footer(window, cx);
        root
            .children(ui::bar::titlebar(&self.menubar, window, cx))
            .child(ui::bar::bar(&self.clock, &ctl, cx))
            .child(self.split_row(sidebar, workspace, right, window, cx))
            .children(footer)
            .children(self.overlay.clone())
            .children(self.palette.clone())
            .children(self.quick_start.clone())
            .children(self.agent_pick.clone())
            .children(self.remote.clone())
            .children(self.settings.clone())
            .children(self.ext_ui.dialog.clone())
            .children(self.rename.clone())
            .children(self.misc.help.clone())
            .children(self.misc.confirm.clone())
            .children(self.groups_ui.edit.clone())
            .children(self.groups_ui.confirm.clone())
            .children(self.usage.stats.clone())
            .children(self.closing_layer())
            .children(self.context_layer(window, cx))
            .children(self.usage.notice.clone())
    }
}

/// Titelleiste: macOS nutzt die normale des Systems mit Ampel (nicht getestet), Windows blendet sie aus und
/// zeichnet eigene Titelzeile samt Menü (`ui::bar::titlebar`).
fn titlebar_options(profile: &Profile) -> gpui::TitlebarOptions {
    gpui::TitlebarOptions {
        title: Some(format!("Kadrell{}", profile.label()).into()),
        appears_transparent: !cfg!(target_os = "macos"),
        traffic_light_position: None,
    }
}

/// Startschema aus den Einstellungen (`colorTheme`, Standard Instrument Dark, Unbekanntes wird Mocha);
/// `KADRELL_DEV_THEME=<id>` (z. B. `latte`) überstimmt sie.
fn start_theme(settings: &kadrell_core::Settings) -> Theme {
    std::env::var("KADRELL_DEV_THEME")
        .ok()
        .and_then(|id| Theme::by_id(&id))
        .unwrap_or_else(|| Theme::named(&settings.color_theme()))
}

/// Lebt der Prozess noch? Über den Prozessbaum des Systems (`parent` ist `None` für fehlende Prozesse).
fn process_alive(pid: u32) -> bool {
    use kadrell_control::ProcessTree;
    kadrell_control::SystemProcessTree.parent(pid).is_some()
}

/// Was `open_state` liefert: Zustand, Aufträge des Steuerkanals, der Kanal selbst und die Extensions.
type Opened = (AppState, async_channel::Receiver<wire::Job>, Option<ControlServer>, ExtensionManager);

/// Öffnet das Profil, sperrt es und baut den Zustand. `Err`: mit dieser Meldung beenden (Exit-Code 1).
fn open_state(profile_name: Option<&str>) -> Result<Option<Opened>, String> {
    let paths = Paths::from_env();
    let profile = Profile::open(profile_name, &paths, std::process::id(), &process_alive).map_err(|e| format!("Profilordner: {e}"))?;
    let lock = match profile.acquire(&process_alive) {
        Acquire::Locked(lock) => Some(lock),
        Acquire::Unprotected => None,
        Acquire::Running(pid) => {
            eprintln!("kadrell: dieses Profil läuft schon (pid {pid}), das Fenster wird gezeigt");
            tray_wire::forward_show(&endpoint_path(&profile.dir));
            return Ok(None);
        }
    };
    kadrell_core::profile::clear_quit_marker(&profile.dir);
    let endpoint = endpoint_path(&profile.dir);
    let exe = std::env::current_exe().map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|_| "kadrell".into());
    let home = paths.home.to_string_lossy().replace('\\', "/");
    theme::pin_at_start(&profile);
    let env = LaunchEnv::from_process(&profile.settings().claude_binary(), exe, endpoint.to_string_lossy().into_owned(), home);
    let state = AppState::new(Parts { paths, profile, lock, env, endpoint: endpoint.clone() });
    let (tx, rx) = async_channel::unbounded();
    let service = wire::service(tx);
    let server = match ControlServer::start(&endpoint, service.clone()) {
        Ok(server) => Some(server),
        Err(e) => {
            eprintln!("kadrell: Steuerkanal nicht gestartet: {e}");
            None
        }
    };
    let ext = ext::build(&state, service);
    Ok(Some((state, rx, server, ext)))
}

/// Startet die App (Profil, Steuerkanal, Fenster, Event-Schleife) und kehrt beim Schließen des Fensters zurück.
/// Gibt den Exit-Code zurück.
pub fn run(profile_name: Option<String>) -> i32 {
    // Vor Profil und Fenster: ein SIGTERM während des Starts wartet, statt die App ohne Aufräumen zu beenden.
    let signals = signals::install_quit_signals();
    let (state, jobs, server, ext) = match open_state(profile_name.as_deref()) {
        Ok(Some(parts)) => parts,
        Ok(None) => return 0,
        Err(message) => {
            eprintln!("kadrell: {message}");
            return 1;
        }
    };
    let title_profile = state.profile.clone();
    lang::apply_at_start(&state.settings);
    let app = application();
    let window_slot = tray_wire::install_reopen(&app);
    app.run(move |cx: &mut App| {
        kadrell_term::fonts::load(cx);
        appearance::fonts::scan(cx);
        appearance::zoom::bind_keys(cx);
        ui::tree_actions::bind_keys(cx);
        ui::text_input::bind_keys(cx);
        ui::new_session::bind_keys(cx);
        ui::palette::bind_keys(cx);
        ui::quick_start::bind_keys(cx);
        models::agent_pick::bind_keys(cx);
        remote::dialog::bind_keys(cx);
        ui::rename::bind_keys(cx);
        ui::groups::bind_keys(cx);
        ui::ext::dialog::bind_keys(cx);
        ui::ext::keys::bind_keys(cx, &title_profile.settings());
        ui::notice::bind_keys(cx);
        misc::bind_keys(cx, &title_profile.settings());
        appearance::apply(cx, &title_profile.settings(), true);
        set_theme(cx, start_theme(&title_profile.settings()));
        menus::init(cx);
        let bounds = windows::frame::main_bounds(&title_profile.settings(), cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(titlebar_options(&title_profile)),
            window_min_size: Some(size(px(windows::frame::MIN_SIZE.0), px(windows::frame::MIN_SIZE.1))),
            ..Default::default()
        };
        let handle = cx.open_window(options, |window, cx| cx.new(|cx| Shell::new(window, cx, state, jobs, server, ext))).unwrap();
        window_slot.set(Some(handle));
        cx.on_app_quit(move |cx| {
            let _ = handle.update(cx, |shell, _, _| {
                if !shell.down {
                    shell.down = true;
                    shell.shutdown();
                }
            });
            async {}
        })
        .detach();
        // Ein weiteres Fenster zu: die App läuft weiter. Ist das Hauptfenster weg, endet sie.
        let main_id = handle.window_id();
        cx.on_window_closed(move |cx, _| {
            if !cx.windows().iter().any(|w| w.window_id() == main_id) {
                cx.quit();
            }
        })
        .detach();
        let _ = handle.update(cx, |shell, window, cx| {
            shell.main_window = Some(window.window_handle());
            shell.restore_windows(cx);
        });
        signals::quit_on(cx, signals);
        cx.activate(true);
    });
    0
}
