//! Platzhalter mit derselben Schnittstelle wie `terminal.rs`, solange das echte Terminal nicht baut.
#![allow(dead_code)]

use std::path::PathBuf;

use gpui::{Context, FocusHandle, Focusable, IntoElement, Render, Window, div, prelude::*};

use crate::ThemeGlobal;

pub struct TerminalPane {
    pub focus: FocusHandle,
    pub exited: bool,
    pub title: Option<String>,
}

impl TerminalPane {
    pub fn new(_program: &str, _args: &[&str], _cwd: PathBuf, cx: &mut Context<Self>) -> Self {
        Self { focus: cx.focus_handle(), exited: false, title: None }
    }
}

impl Focusable for TerminalPane {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for TerminalPane {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.global::<ThemeGlobal>().0;
        div()
            .id("terminal-stub")
            .track_focus(&self.focus)
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(t.bg)
            .text_color(t.muted)
            .child("TERMINAL")
    }
}
