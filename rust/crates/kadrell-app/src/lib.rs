//! Kadrell-Fenster mit Baum, Leiste und Terminal-Kacheln (GPUI).
#![allow(dead_code)] // Mock-Daten und Theme: nicht alle Felder werden schon benutzt.

mod menus;
mod state;
mod theme;
mod ui;

use gpui::{
    App, Bounds, Context, Entity, FocusHandle, Focusable, Global, IntoElement, KeyDownEvent, Render, Window,
    WindowBounds, WindowOptions, div, prelude::*, px, size,
};
use gpui_platform::application;

use kadrell_term::{TermThemeGlobal, TerminalPane};
use theme::Theme;
use ui::layout::{Direction, Id, LayoutMode, LayoutState, Selection};
use ui::model::{Group, flat_index, mock_groups};

pub struct ThemeGlobal(pub Theme);
impl Global for ThemeGlobal {}

/// Setzt das Farbschema der App und gibt dem Terminal die Farben, die es braucht.
fn set_theme(cx: &mut App, theme: Theme) {
    cx.set_global(TermThemeGlobal(theme.term()));
    cx.set_global(ThemeGlobal(theme));
}

pub struct Shell {
    groups: Vec<Group>,
    /// Fokus-Kachel (Tastaturziel), immer Teil der Auswahl.
    selected: (usize, usize),
    /// Sichtbare Sessions (Kacheln), Anker für Shift-Klick.
    selection: Selection,
    layout: LayoutState,
    clock: String,
    menubar: Entity<ui::menubar::MenuBar>,
    overlay: Option<Entity<ui::new_session::NewSession>>,
    palette: Option<Entity<ui::palette::Palette>>,
}

/// Start-Auswahl für Screenshots ohne Eingabe: `KADRELL_DEV_SELECT="0,0;0,1;1,0"`.
fn start_selection(groups: &[Group]) -> Vec<Id> {
    let Ok(v) = std::env::var("KADRELL_DEV_SELECT") else { return Vec::new() };
    let valid = |id: &Id| groups.get(id.0).is_some_and(|g| id.1 < g.sessions.len());
    let parse = |p: &str| -> Option<Id> {
        let (g, s) = p.split_once(',')?;
        Some((g.trim().parse().ok()?, s.trim().parse().ok()?))
    };
    v.split(';').filter_map(parse).filter(valid).collect()
}

/// Start-Layout: `KADRELL_DEV_LAYOUT=grid|main|spiral|stack`.
fn start_layout() -> LayoutMode {
    std::env::var("KADRELL_DEV_LAYOUT").ok().and_then(|v| LayoutMode::from_name(&v)).unwrap_or(LayoutMode::Grid)
}

/// Start-Session für Screenshots ohne Eingabe: `KADRELL_DEV_SESSION=1,0` (Gruppe, Session), sonst die erste.
fn start_session(groups: &[ui::model::Group]) -> (usize, usize) {
    let parsed = std::env::var("KADRELL_DEV_SESSION").ok().and_then(|v| {
        let (g, s) = v.split_once(',')?;
        Some((g.trim().parse().ok()?, s.trim().parse().ok()?))
    });
    match parsed {
        Some((g, s)) if groups.get(g).is_some_and(|gr: &ui::model::Group| s < gr.sessions.len()) => (g, s),
        _ => (0, 0),
    }
}

fn now_hhmm() -> String {
    chrono::Local::now().format("%H:%M").to_string()
}

impl Shell {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let menubar = cx.new(ui::menubar::MenuBar::new);
        let groups = mock_groups();
        let first = start_session(&groups);
        let mut selection = Selection::single(first);
        let flat = flat_index(&groups);
        for id in start_selection(&groups) {
            selection.toggle(id, &flat);
        }
        selection.focus_on(first);
        let mut shell = Shell {
            groups,
            selected: first,
            selection,
            layout: LayoutState::new(start_layout()),
            clock: now_hhmm(),
            menubar,
            overlay: None,
            palette: None,
        };
        shell.apply_selection(window, cx);
        shell.start_clock(cx);
        shell.open_menu_from_env(window, cx);
        shell.open_overlay_from_env(window, cx);
        shell.open_palette_from_env(window, cx);
        shell
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
        self.groups[g].collapsed = !self.groups[g].collapsed;
        cx.notify();
    }

    fn ensure_pane(&mut self, g: usize, s: usize, cx: &mut Context<Self>) {
        let session = &mut self.groups[g].sessions[s];
        if session.pane.is_some() {
            return;
        }
        let l = &session.launch;
        let (program, args, cwd) = (l.program, l.args, l.cwd());
        session.pane = Some(cx.new(|cx| TerminalPane::new(program, args, cwd.clone(), cx)));
        self.load_branch(g, s, cwd, cx);
    }

    /// Liest den Git-Zweig einmal im Hintergrund und trägt ihn in die Session ein.
    fn load_branch(&self, g: usize, s: usize, cwd: std::path::PathBuf, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let branch = cx.background_executor().spawn(async move { ui::model::git_branch(&cwd) }).await;
            let _ = this.update(cx, |shell, cx| {
                shell.groups[g].sessions[s].branch = branch;
                cx.notify();
            });
        })
        .detach();
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

    /// Sichtbare Kacheln in Anzeigereihenfolge.
    fn visible(&self) -> Vec<Id> {
        self.selection.items.clone()
    }

    /// Übernimmt `selection`: legt fehlende Terminals an und gibt der Fokus-Kachel die Tastatur.
    fn apply_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for (g, s) in self.selection.items.clone() {
            self.ensure_pane(g, s, cx);
        }
        let (g, s) = self.selection.focus;
        self.selected = (g, s);
        let focus = self.groups[g].sessions[s].pane.as_ref().map(|p| p.read(cx).focus.clone());
        if let Some(focus) = focus {
            focus.focus(window, cx);
        }
        cx.notify();
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
        if self.selection.focus != (g, s) {
            self.selection.focus_on((g, s));
            self.apply_selection(window, cx);
        }
    }

    pub(crate) fn cycle_layout(&mut self, cx: &mut Context<Self>) {
        self.layout.mode = self.layout.mode.next();
        cx.notify();
    }

    pub(crate) fn step_cols(&mut self, delta: isize, cx: &mut Context<Self>) {
        self.layout.cols = ui::layout::step_cols(self.layout.cols, self.visible().len(), delta);
        cx.notify();
    }

    /// Alt+Pfeil: Fokus zur Nachbarkachel in dieser Richtung.
    fn move_focus(&mut self, d: Direction, window: &mut Window, cx: &mut Context<Self>) {
        let visible = self.visible();
        let Some(i) = visible.iter().position(|id| *id == self.selected) else { return };
        if let Some(j) = ui::layout::neighbor(i, &self.layout.frames, d) {
            self.selection.focus_on(visible[j]);
            self.apply_selection(window, cx);
        }
    }

    fn layout_ctl(&self, cx: &mut Context<Self>) -> ui::bar::LayoutCtl {
        use std::rc::Rc;
        ui::bar::LayoutCtl {
            mode: self.layout.mode,
            cols_label: self.layout.label(),
            cycle: Rc::new(cx.listener(|this, _, _, cx| this.cycle_layout(cx))),
            dec: Rc::new(cx.listener(|this, _, _, cx| this.step_cols(-1, cx))),
            inc: Rc::new(cx.listener(|this, _, _, cx| this.step_cols(1, cx))),
        }
    }

    /// Springt um `delta` Sessions weiter (zyklisch).
    fn step(&mut self, delta: isize, window: &mut Window, cx: &mut Context<Self>) {
        let flat = flat_index(&self.groups);
        let Some(pos) = flat.iter().position(|p| *p == self.selected) else { return };
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
        if (self.overlay.is_some() || self.palette.is_some()) && m.alt {
            cx.stop_propagation();
            return;
        }
        // Strg+L (Cmd+L auf dem Mac) schaltet das Layout reihum.
        if m.secondary(cx) && !m.alt && !m.shift && ev.keystroke.key == "l" {
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

impl Focusable for Shell {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        let (g, s) = self.selected;
        match &self.groups[g].sessions[s].pane {
            Some(p) => p.read(cx).focus.clone(),
            None => cx.focus_handle(),
        }
    }
}

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = ui::theme(cx);
        let sidebar = ui::sidebar::sidebar(&self.groups, &self.selection, cx);
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
        let root = self.menubar_hooks(menus::attach_actions(root, cx), cx);
        root
            .children(ui::bar::titlebar(&self.menubar, window, cx))
            .child(ui::bar::bar(&self.clock, &ctl, cx))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(sidebar)
                    .child(workspace),
            )
            .child(ui::footer::footer(cx))
            .children(self.overlay.clone())
            .children(self.palette.clone())
    }
}

/// Titelleiste: macOS nutzt die normale des Systems mit Ampel (nicht getestet), Windows blendet sie aus und
/// zeichnet eigene Titelzeile samt Menü (`ui::bar::titlebar`).
fn titlebar_options() -> gpui::TitlebarOptions {
    gpui::TitlebarOptions {
        title: Some("Kadrell".into()),
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

/// Startet die App (Fenster, Event-Schleife) und kehrt beim Schließen des Fensters zurück.
pub fn run() {
    application().run(|cx: &mut App| {
        kadrell_term::fonts::load(cx);
        ui::text_input::bind_keys(cx);
        ui::new_session::bind_keys(cx);
        ui::palette::bind_keys(cx);
        set_theme(cx, start_theme());
        menus::init(cx);
        let bounds = Bounds::centered(None, ui::overlay::window_size(), cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(titlebar_options()),
            window_min_size: Some(size(px(640.), px(400.))),
            ..Default::default()
        };
        cx.open_window(options, |window, cx| cx.new(|cx| Shell::new(window, cx))).unwrap();
        cx.on_window_closed(|cx, _| cx.quit()).detach();
        cx.activate(true);
    });
}
