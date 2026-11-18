//! Kadrell-Fenster mit Baum, Leiste und Terminal-Kacheln (GPUI), verdrahtet mit dem Zustand (`state`): echte Sessions,
//! Steuerkanal, Status. Der Einstieg ist [`run`].
#![allow(dead_code)] // Theme: nicht alle Felder werden schon benutzt.

mod ext;
mod lang;
#[cfg(test)]
mod catalog_test;
mod menus;
mod state;
mod theme;
mod tray_wire;
mod ui;
mod wire;

use std::collections::HashMap;

use gpui::{
    App, Bounds, Context, Entity, FocusHandle, Focusable, Global, IntoElement, KeyDownEvent, Render, Window, WindowBounds,
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
use ui::layout::{Direction, Id, LayoutMode, LayoutState, Selection};
use ui::model::{Group, flat_index};

pub struct ThemeGlobal(pub Theme);
impl Global for ThemeGlobal {}

/// Setzt das Farbschema der App und gibt dem Terminal die Farben, die es braucht.
fn set_theme(cx: &mut App, theme: Theme) {
    cx.set_global(TermThemeGlobal(theme.term()));
    cx.set_global(ThemeGlobal(theme));
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
    quick_start: Option<Entity<ui::quick_start::QuickStart>>,
    /// Index der Verlaufssuche (`#`), beim Start und bei jedem Öffnen der Suche nachgelesen.
    history: ui::palette::HistoryStore,
    settings: Option<Entity<ui::settings::SettingsDialog>>,
    rename: Option<Entity<ui::rename::Rename>>,
    context: Option<ui::context_menu::ContextMenu>,
    /// Tastaturziel, solange keine Kachel da ist.
    idle_focus: FocusHandle,
    /// Der Steuerkanal; mit der Shell lebt und stirbt er.
    server: Option<ControlServer>,
    down: bool,
    /// Die Lua-Extensions dieses Profils.
    ext: ExtensionManager,
    /// Tray-Icon, Benachrichtigungen, Badge und das Verstecken des Fensters (`tray_wire`).
    tray: tray_wire::TrayWire,
    /// Stand der Sprache, für die Menüs und die Ansicht zuletzt gebaut wurden (`lang::sync_language`).
    language_generation: u64,
    /// Oberfläche der Extensions: Seitenleisten, Statusleisten, Dialog (`ui::ext`).
    ext_ui: ui::ext::ExtUi,
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
    fn new(window: &mut Window, cx: &mut Context<Self>, state: AppState, jobs: async_channel::Receiver<wire::Job>, server: Option<ControlServer>, ext: ExtensionManager) -> Self {
        let menubar = cx.new(ui::menubar::MenuBar::new);
        let mode = ui_layout(state.layout);
        let folder_home = state.home();
        let ext_ui = ui::ext::ExtUi::new(&ext, cx);
        let mut shell = Shell {
            state,
            groups: Vec::new(),
            panes: HashMap::new(),
            selected: None,
            selection: Selection { items: Vec::new(), focus: (0, 0), anchor: (0, 0) },
            layout: LayoutState::new(mode),
            clock: now_hhmm(),
            menubar,
            overlay: None,
            palette: None,
            quick_start: None,
            history: ui::palette::HistoryStore::new(&folder_home),
            settings: None,
            rename: None,
            context: None,
            idle_focus: cx.focus_handle(),
            server,
            down: false,
            ext,
            tray: tray_wire::TrayWire::new(),
            language_generation: kadrell_i18n::global().generation(),
            ext_ui,
        };
        shell.rebuild();
        shell.apply_dev_view();
        shell.state.ready = true;
        shell.sync(window, cx);
        shell.start_clock(cx);
        shell.start_jobs(jobs, window, cx);
        shell.start_poll(window, cx);
        shell.open_menu_from_env(window, cx);
        shell.open_overlay_from_env(window, cx);
        shell.open_palette_from_env(window, cx);
        shell.open_quick_start_from_env(window, cx);
        shell.refresh_history(cx);
        shell.open_settings_from_env(window, cx);
        shell.start_ext_ui(window, cx);
        shell.start_extensions();
        shell.start_tray(window, cx);
        shell.open_extensions_from_env(window, cx);
        shell
    }

    /// `KADRELL_DEV_SELECT`, `KADRELL_DEV_SESSION` und `KADRELL_DEV_LAYOUT` setzen Auswahl und Layout beim Start,
    /// gegen den echten Zustand (Positionen im Baum, die es geben muss).
    /// Startet die Extensions (Katalog einlesen, eingeschaltete Helper starten) und meldet `app.ready`.
    fn start_extensions(&self) {
        self.ext.start();
        self.ext_dev_enable();
        self.ext.emit("app.ready", serde_json::json!({}));
    }

    fn apply_dev_view(&mut self) {
        let keys = dev_selection(&self.groups);
        let focus = std::env::var("KADRELL_DEV_SESSION").ok().and_then(|v| wire::keys_at(&self.groups, &v).into_iter().next());
        if !keys.is_empty() || focus.is_some() {
            let keys = if keys.is_empty() { focus.iter().cloned().collect() } else { keys };
            self.state.set_selection(keys, focus);
        }
        if let Some(mode) = start_layout() {
            self.layout.mode = mode;
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

    fn toggle_group(&mut self, g: usize, cx: &mut Context<Self>) {
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
        }
        self.apply_selection(window, cx);
    }

    // --- Auswahl und Layout (Kacheln) ---

    /// Sichtbare Kacheln in Anzeigereihenfolge; gezoomt nur die Fokus-Kachel.
    fn visible(&self) -> Vec<Id> {
        match (self.state.zoom, self.selected) {
            (true, Some(focus)) => vec![focus],
            _ => self.selection.items.clone(),
        }
    }

    /// Übernimmt `selection` in den Zustand: startet fehlende Terminals und gibt der Fokus-Kachel die Tastatur.
    fn apply_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_selection(window, cx);
    }

    /// Klick im Baum: nur diese Session, Strg/Cmd nimmt sie dazu oder weg, Shift markiert den Bereich.
    fn click_session(&mut self, g: usize, s: usize, m: gpui::Modifiers, window: &mut Window, cx: &mut Context<Self>) {
        let flat = flat_index(&self.groups);
        if m.shift {
            self.selection.range((g, s), &flat);
        } else if m.secondary(cx) {
            self.selection.toggle((g, s), &flat);
        } else {
            self.selection = Selection::single((g, s));
        }
        self.apply_selection(window, cx);
    }

    /// Klick auf eine Gruppe: alle ihre Sessions werden sichtbar.
    fn select_group(&mut self, g: usize, window: &mut Window, cx: &mut Context<Self>) {
        let ids = (0..self.groups[g].sessions.len()).map(|s| (g, s)).collect();
        self.selection.group(ids);
        self.apply_selection(window, cx);
    }

    /// Klick in eine Kachel: sie wird Fokus, die Auswahl bleibt.
    fn focus_tile(&mut self, g: usize, s: usize, window: &mut Window, cx: &mut Context<Self>) {
        let key = self.groups[g].sessions[s].key.clone();
        let ended = matches!(self.state.run(&key), state::Run::Ended | state::Run::Stopped);
        if self.selection.focus != (g, s) || ended {
            self.selection.focus_on((g, s));
            self.apply_selection(window, cx);
        }
    }

    pub(crate) fn cycle_layout(&mut self, cx: &mut Context<Self>) {
        self.layout.mode = self.layout.mode.next();
        self.state.layout = control_layout(self.layout.mode);
        self.state.save_layout();
        cx.notify();
    }

    pub(crate) fn step_cols(&mut self, delta: isize, cx: &mut Context<Self>) {
        self.layout.cols = ui::layout::step_cols(self.layout.cols, self.visible().len(), delta);
        cx.notify();
    }

    /// Alt+Pfeil: Fokus zur Nachbarkachel in dieser Richtung.
    fn move_focus(&mut self, d: Direction, window: &mut Window, cx: &mut Context<Self>) {
        let visible = self.visible();
        let Some(i) = self.selected.and_then(|sel| visible.iter().position(|id| *id == sel)) else { return };
        if let Some(j) = ui::layout::neighbor(i, &self.layout.frames, d) {
            self.selection.focus_on(visible[j]);
            self.apply_selection(window, cx);
        }
    }

    fn layout_ctl(&self, cx: &mut Context<Self>) -> ui::bar::LayoutCtl {
        use std::rc::Rc;
        let running = self.state.running_count();
        ui::bar::LayoutCtl {
            mode: self.layout.mode,
            cols_label: self.layout.label(),
            cycle: Rc::new(cx.listener(|this, _, _, cx| this.cycle_layout(cx))),
            dec: Rc::new(cx.listener(|this, _, _, cx| this.step_cols(-1, cx))),
            inc: Rc::new(cx.listener(|this, _, _, cx| this.step_cols(1, cx))),
            grouped: self.state.grouped(),
            sort: self.state.sort(),
            toggle_grouped: Rc::new(cx.listener(|this, _, _, cx| this.toggle_grouped(cx))),
            toggle_alpha: Rc::new(cx.listener(|this, _, _, cx| this.toggle_sort(SidebarSort::Alpha, cx))),
            toggle_status: Rc::new(cx.listener(|this, _, _, cx| this.toggle_sort(SidebarSort::Status, cx))),
            running,
            detached: self.state.store.sessions.sessions.len().saturating_sub(running),
            waiting: self.waiting_count(),
            ext: self.ext_status(cx),
        }
    }

    /// Sessions, die auf dich warten.
    fn waiting_count(&self) -> usize {
        self.state.store.sessions.sessions.iter().filter(|s| self.state.display_status(&s.id) == Some(SessionStatus::Waiting)).count()
    }

    fn toggle_grouped(&mut self, cx: &mut Context<Self>) {
        self.state.toggle_grouped();
        self.rebuild();
        cx.notify();
    }

    fn toggle_sort(&mut self, mode: SidebarSort, cx: &mut Context<Self>) {
        self.state.toggle_sort(mode);
        self.rebuild();
        cx.notify();
    }

    /// Springt um `delta` Sessions weiter (zyklisch).
    fn step(&mut self, delta: isize, window: &mut Window, cx: &mut Context<Self>) {
        let flat = flat_index(&self.groups);
        let Some(pos) = self.selected.and_then(|sel| flat.iter().position(|p| *p == sel)) else { return };
        let next = (pos as isize + delta).rem_euclid(flat.len() as isize) as usize;
        self.select(flat[next].0, flat[next].1, window, cx);
    }

    fn jump(&mut self, n: usize, window: &mut Window, cx: &mut Context<Self>) -> bool {
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

    fn handle_alt(&mut self, key: &str, window: &mut Window, cx: &mut Context<Self>) -> bool {
        match key {
            "n" => self.step(1, window, cx),
            "p" => self.step(-1, window, cx),
            "t" => Self::toggle_theme(cx),
            "left" => self.move_focus(Direction::Left, window, cx),
            "right" => self.move_focus(Direction::Right, window, cx),
            "up" => self.move_focus(Direction::Up, window, cx),
            "down" => self.move_focus(Direction::Down, window, cx),
            k => match k.parse::<usize>() {
                Ok(d) if (1..=9).contains(&d) => return self.jump(d - 1, window, cx),
                _ => return false,
            },
        }
        true
    }

    fn on_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let m = ev.keystroke.modifiers;
        if std::env::var_os("KADRELL_DEV_KEYLOG").is_some() {
            eprintln!("key {:?}", ev.keystroke);
        }
        // Solange der Dialog offen ist, gehören Alt-Kürzel nicht der Shell.
        if (self.overlay.is_some() || self.palette.is_some() || self.settings.is_some() || self.quick_start.is_some() || self.ext_ui.dialog.is_some()) && m.alt {
            cx.stop_propagation();
            return;
        }
        // Strg+L (Cmd+L auf dem Mac) schaltet das Layout reihum.
        if self.settings.is_none() && m.secondary(cx) && !m.alt && !m.shift && ev.keystroke.key == "l" {
            self.cycle_layout(cx);
            cx.stop_propagation();
            return;
        }
        // Strg+Alt ist AltGr unter Windows und gehört dem Terminal.
        if !m.alt || m.control || m.platform || m.shift {
            return;
        }
        if self.handle_alt(ev.keystroke.key.as_str(), window, cx) {
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
        match self.selected.and_then(|(g, s)| self.groups.get(g)?.sessions.get(s)?.pane.as_ref()) {
            Some(p) => p.read(cx).focus.clone(),
            None => self.idle_focus.clone(),
        }
    }
}

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_language(cx);
        let t = ui::theme(cx);
        let sidebar = ui::sidebar::sidebar(&self.groups, &self.selection, cx);
        let sidebar = self.ext_left_column(sidebar, cx);
        let ctl = self.layout_ctl(cx);
        let workspace = ui::layout::workspace(self, cx);
        let root = div()
            .flex()
            .flex_col()
            .size_full()
            .bg(t.bg)
            .text_color(t.fg)
            .font_family(theme::FONT)
            .text_size(px(12.))
            .capture_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| this.on_key(ev, window, cx)));
        let root = self.menubar_hooks(ui::ext::attach_actions(menus::attach_actions(root, cx), cx), cx);
        let right = self.ext_right_panel(cx);
        let footer = self.ext_footer(window, cx);
        root
            .children(ui::bar::titlebar(&self.menubar, window, cx))
            .child(ui::bar::bar(&self.clock, &ctl, cx))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(sidebar)
                    .child(workspace)
                    .children(right),
            )
            .children(footer)
            .children(self.overlay.clone())
            .children(self.palette.clone())
            .children(self.quick_start.clone())
            .children(self.settings.clone())
            .children(self.ext_ui.dialog.clone())
            .children(self.rename.clone())
            .children(self.context_layer(window, cx))
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

/// Startschema; `KADRELL_DEV_THEME=latte` startet hell.
fn start_theme() -> Theme {
    match std::env::var("KADRELL_DEV_THEME").as_deref() {
        Ok("latte") => Theme::latte(),
        _ => Theme::mocha(),
    }
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
    application().run(move |cx: &mut App| {
        kadrell_term::fonts::load(cx);
        ui::text_input::bind_keys(cx);
        ui::new_session::bind_keys(cx);
        ui::palette::bind_keys(cx);
        ui::quick_start::bind_keys(cx);
        ui::rename::bind_keys(cx);
        ui::ext::dialog::bind_keys(cx);
        ui::ext::keys::bind_keys(cx, &title_profile.settings());
        set_theme(cx, start_theme());
        menus::init(cx);
        let bounds = Bounds::centered(None, ui::overlay::window_size(), cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(titlebar_options(&title_profile)),
            window_min_size: Some(size(px(640.), px(400.))),
            ..Default::default()
        };
        let handle = cx.open_window(options, |window, cx| cx.new(|cx| Shell::new(window, cx, state, jobs, server, ext))).unwrap();
        tray_wire::install_reopen(cx, handle);
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
        cx.on_window_closed(|cx, _| cx.quit()).detach();
        cx.activate(true);
    });
    0
}
