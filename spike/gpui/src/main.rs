//! Testgerüst für die Terminal-Kachel: ein Fenster, ein Terminal, Ende beim Schließen des Fensters.
//! Das App-Gerüst (Leiste, Baum, Kachel) baut ein anderer Agent, Schnittstelle ist `terminal::TerminalPane`.
mod terminal;
mod theme;

use gpui::{App, AppContext, Bounds, Global, WindowBounds, WindowOptions, px, size};

pub struct ThemeGlobal(pub crate::theme::Theme);
impl Global for ThemeGlobal {}

fn shell() -> &'static str {
    let found = std::env::var_os("PATH")
        .is_some_and(|p| std::env::split_paths(&p).any(|dir| dir.join("pwsh.exe").is_file()));
    if found { "pwsh.exe" } else { "powershell.exe" }
}

fn main() {
    gpui_platform::application().run(|cx: &mut App| {
        cx.set_global(ThemeGlobal(theme::Theme::mocha()));
        let bounds = Bounds::centered(None, size(px(1200.), px(800.)), cx);
        let cwd = std::env::current_dir().unwrap_or_default();
        let window = cx
            .open_window(
                WindowOptions { window_bounds: Some(WindowBounds::Windowed(bounds)), ..Default::default() },
                |_, cx| cx.new(|cx| terminal::TerminalPane::new(shell(), &[], cwd, cx)),
            )
            .expect("Fenster");
        window
            .update(cx, |pane, window, cx| {
                window.focus(&pane.focus, cx);
                cx.activate(true);
            })
            .ok();
        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
    });
}
