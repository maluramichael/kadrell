//! GPUI-Spike: Kadrell-Fenster mit Baum, Leiste und Terminal-Kachel.
#![allow(dead_code)] // Spike: nicht alle Theme-Felder werden benutzt.

mod fonts;
mod boxdraw;
mod terminal;
mod theme;
mod ui;

use gpui::{
    App, Bounds, Context, FocusHandle, Focusable, Global, IntoElement, KeyDownEvent, Render, Window,
    WindowBounds, WindowOptions, div, prelude::*, px, size,
};
use gpui_platform::application;

use terminal::TerminalPane;
use theme::Theme;
use ui::model::{Group, flat_index, mock_groups};

pub struct ThemeGlobal(pub Theme);
impl Global for ThemeGlobal {}

pub struct Shell {
    groups: Vec<Group>,
    selected: (usize, usize),
    clock: String,
}

/// Start-Session für Screenshots ohne Eingabe: `KADRELL_SPIKE_SESSION=1,0` (Gruppe, Session), sonst die erste.
fn start_session(groups: &[ui::model::Group]) -> (usize, usize) {
    let parsed = std::env::var("KADRELL_SPIKE_SESSION").ok().and_then(|v| {
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
        let mut shell = Shell { groups: mock_groups(), selected: (0, 0), clock: now_hhmm() };
        let (g, s) = start_session(&shell.groups);
        shell.select(g, s, window, cx);
        shell.start_clock(cx);
        shell
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

    fn select(&mut self, g: usize, s: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.ensure_pane(g, s, cx);
        self.selected = (g, s);
        let focus = self.groups[g].sessions[s].pane.as_ref().map(|p| p.read(cx).focus.clone());
        if let Some(focus) = focus {
            focus.focus(window, cx);
        }
        cx.notify();
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
        cx.set_global(ThemeGlobal(next));
        cx.refresh_windows();
    }

    fn handle_alt(&mut self, key: &str, window: &mut Window, cx: &mut Context<Self>) -> bool {
        match key {
            "n" => self.step(1, window, cx),
            "p" => self.step(-1, window, cx),
            "t" => Self::toggle_theme(cx),
            k => match k.parse::<usize>() {
                Ok(d) if (1..=9).contains(&d) => return self.jump(d - 1, window, cx),
                _ => return false,
            },
        }
        true
    }

    fn on_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let m = ev.keystroke.modifiers;
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
        let (g, s) = self.selected;
        let tile = ui::tile::tile(&self.groups[g], &self.groups[g].sessions[s], ui::tile::TileStyle::for_count(1), window, cx);
        let sidebar = ui::sidebar::sidebar(&self.groups, self.selected, cx);
        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(t.bg)
            .text_color(t.fg)
            .font_family(theme::FONT)
            .text_size(px(12.))
            .capture_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| this.on_key(ev, window, cx)))
            .child(ui::bar::bar(&self.clock, window, cx))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(sidebar)
                    .child(ui::tile::tile_area(tile, ui::tile::TileStyle::for_count(1))),
            )
            .child(ui::footer::footer(cx))
    }
}

/// Eigene Titelleiste: Die Leiste der App ist die Titelleiste (Windows: eigene Knöpfe, macOS: Ampel eingerückt;
/// der macOS-Zweig ist hier nicht getestet).
fn titlebar_options() -> gpui::TitlebarOptions {
    gpui::TitlebarOptions {
        title: Some("Kadrell (GPUI-Spike)".into()),
        appears_transparent: true,
        traffic_light_position: Some(gpui::point(px(12.), px(9.))),
    }
}

/// Startschema; `KADRELL_SPIKE_THEME=latte` startet hell.
fn start_theme() -> Theme {
    match std::env::var("KADRELL_SPIKE_THEME").as_deref() {
        Ok("latte") => Theme::latte(),
        _ => Theme::mocha(),
    }
}

fn main() {
    terminal::strip_inherited_env();
    application().run(|cx: &mut App| {
        fonts::load(cx);
        cx.set_global(ThemeGlobal(start_theme()));
        let bounds = Bounds::centered(None, size(px(1400.), px(900.)), cx);
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
