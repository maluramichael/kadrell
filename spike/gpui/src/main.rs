//! GPUI-Spike: Kadrell-Fenster mit Baum, Leiste und Terminal-Kachel.
#![allow(dead_code)] // Spike: nicht alle Theme-Felder werden benutzt.

mod fonts;
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

fn now_hhmm() -> String {
    chrono::Local::now().format("%H:%M").to_string()
}

impl Shell {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut shell = Shell { groups: mock_groups(), selected: (0, 0), clock: now_hhmm() };
        shell.select(0, 0, window, cx);
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
        session.pane = Some(cx.new(|cx| TerminalPane::new(program, args, cwd, cx)));
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
        let tile = ui::tile::tile(&self.groups[g], &self.groups[g].sessions[s], window, cx);
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
                    .child(div().flex_1().min_w_0().p(px(8.)).child(tile)),
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
