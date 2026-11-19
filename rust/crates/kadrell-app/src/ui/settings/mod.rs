//! Einstellungen (⌘, / Strg+,) wie `SettingsView.swift`: zwei Spalten mit allen Abschnitten, Filter mit `/`,
//! Änderungen gelten sofort. Aufbau: `sections` baut die Zeilen als Daten, `model` filtert sie, `layout` verteilt sie
//! auf Spalten, `controls` sind die Bausteine, `view` zeichnet, `dialog` hält Zustand und Tastatur, `store` die
//! Einstellungen der Instanz, `backend` die (noch erfundenen) Konten und lokalen Modelle.

use gpui::{App, AppContext, Context, Div, Global, InteractiveElement, KeyBinding, MouseButton, MouseMoveEvent, Window, actions};

pub mod backend;
pub mod controls;
mod dialog;
pub mod filter;
pub mod layout;
pub mod model;
pub mod paths;
pub mod sections;
pub mod shortcut;
pub mod store;
pub mod truncate;
mod view;

pub use dialog::{SettingsDialog, SettingsEvent};

use crate::Shell;

actions!(kadrell_settings, [Cancel, Confirm, MoveUp, MoveDown, NextRow, PrevRow, Decrease, Increase]);

pub const CONTEXT: &str = "KadrellSettings";

/// Merker: die Tastenbelegung ist schon eingetragen (einmal pro Prozess).
struct KeysBound;
impl Global for KeysBound {}

/// Trägt die Tasten des Dialogs ein, beim ersten Öffnen.
pub fn bind_keys(cx: &mut App) {
    if cx.has_global::<KeysBound>() {
        return;
    }
    cx.set_global(KeysBound);
    let c = Some(CONTEXT);
    cx.bind_keys([
        KeyBinding::new("escape", Cancel, c),
        KeyBinding::new("enter", Confirm, c),
        KeyBinding::new("up", MoveUp, c),
        KeyBinding::new("down", MoveDown, c),
        KeyBinding::new("tab", NextRow, c),
        KeyBinding::new("shift-tab", PrevRow, c),
        KeyBinding::new("left", Decrease, c),
        KeyBinding::new("right", Increase, c),
    ]);
}

/// Aktionen, Tasten und Maus des Dialogs.
fn attach_handlers(frame: Div, cx: &mut Context<SettingsDialog>) -> Div {
    frame
        .on_action(cx.listener(|this, _: &Cancel, window, cx| this.cancel(window, cx)))
        .on_action(cx.listener(|this, _: &Confirm, window, cx| this.confirm(window, cx)))
        .on_action(cx.listener(|this, _: &MoveUp, window, cx| this.arrow(-1, window, cx)))
        .on_action(cx.listener(|this, _: &MoveDown, window, cx| this.arrow(1, window, cx)))
        .on_action(cx.listener(|this, _: &NextRow, window, cx| this.next_row(window, cx)))
        .on_action(cx.listener(|this, _: &PrevRow, window, cx| this.tab_stop(-1, window, cx)))
        .on_action(cx.listener(|this, _: &Decrease, window, cx| this.side(-1., window, cx)))
        .on_action(cx.listener(|this, _: &Increase, window, cx| this.side(1., window, cx)))
        .on_key_down(cx.listener(|this, ev, window, cx| this.on_key(ev, window, cx)))
        .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| this.press_outside(cx)))
        .on_mouse_move(cx.listener(|this, ev: &MouseMoveEvent, _, cx| {
            if ev.pressed_button == Some(MouseButton::Left) {
                this.drag_to(ev.position.x, cx);
            }
        }))
        .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, cx| this.end_drag(cx)))
        .on_mouse_up_out(MouseButton::Left, cx.listener(|this, _, _, cx| this.end_drag(cx)))
}

impl SettingsDialog {
    /// ↑↓: in der Liste die Markierung, sonst die Tastaturzeile; in einem Textfeld nichts.
    fn arrow(&mut self, delta: isize, window: &mut Window, cx: &mut Context<Self>) {
        if self.popover.is_some() {
            self.move_popover(delta, cx);
        } else if !self.text_focused(window, cx) {
            self.move_cursor(delta, window, cx);
        }
    }

    /// ←→: ein Schritt am Regler der Tastaturzeile.
    fn side(&mut self, direction: f64, window: &mut Window, cx: &mut Context<Self>) {
        if self.popover.is_none() && !self.text_focused(window, cx) {
            self.nudge(direction, cx);
        }
    }

    /// Startzustand für Screenshots ohne Eingabe, aus `KADRELL_DEV_SETTINGS_*`: `FILTER` (Text im Filter),
    /// `SCROLL` (Pixel), `CURSOR` (Zeilen-Id), `OPEN` (Zeilen-Id eines Dropdowns), `RECORD` (Name einer Kürzel-Aktion).
    pub fn dev_setup(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let var = |name: &str| std::env::var(format!("KADRELL_DEV_SETTINGS_{name}")).ok();
        if let Some(text) = var("FILTER") {
            self.open_filter(window, cx);
            self.filter_input.update(cx, |i, cx| i.set_text(&text, window, cx));
        }
        self.dev_models(&var("SEARCH"), &var("PULL"));
        self.cursor = var("CURSOR");
        if let Some(action) = var("RECORD").and_then(|n| kadrell_core::hotkeys::HotkeyAction::from_raw(&n)) {
            self.start_recording(action, cx);
        }
        let (scroll, open) = (var("SCROLL").and_then(|v| v.parse::<f32>().ok()), var("OPEN"));
        if scroll.is_some() || open.is_some() {
            self.dev_later(scroll, open, window, cx);
        }
    }

    /// `KADRELL_DEV_SETTINGS_SEARCH` sucht auf ollama.com, `KADRELL_DEV_SETTINGS_PULL` lädt ein Modell (nur Screenshots; zeigt
    /// auf den eingestellten Ollama-Dienst, also nie mit einem echten Dienst und einem Modell, das nicht geladen werden soll).
    fn dev_models(&mut self, search: &Option<String>, pull: &Option<String>) {
        if let Some(q) = search {
            self.backend.search(q);
        }
        if let Some(name) = pull {
            self.backend.pull(name);
        }
    }

    /// Scrollen und Dropdown öffnen erst, wenn der erste Frame die Rechtecke gemessen hat.
    fn dev_later(&mut self, scroll: Option<f32>, open: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(std::time::Duration::from_millis(700)).await;
            this.update_in(cx, |this, window, cx| {
                if let Some(y) = scroll {
                    this.scroll.set_offset(gpui::point(gpui::px(0.), gpui::px(-y)));
                }
                if let Some(row) = open.and_then(|id| this.find_row(&id, cx)) {
                    this.open_select(&row, window, cx);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}

// Anbindung an die Shell.

impl Shell {
    /// Aktion „Einstellungen …“ (Menü, Strg+, / ⌘,): öffnet den Dialog, solange kein anderer offen ist.
    pub fn open_settings_action(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.settings.is_none() && self.overlay.is_none() {
            self.present_overlay(Box::new(|shell, window, cx| shell.open_settings(window, cx)), window, cx);
        }
    }

    /// Für Screenshots ohne Eingabe: `KADRELL_DEV_OVERLAY=settings`.
    pub fn open_settings_from_env(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if std::env::var("KADRELL_DEV_OVERLAY").as_deref() == Ok("settings") {
            self.open_settings(window, cx);
        }
    }

    fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        store::seed(cx, self.state.settings.clone());
        let dialog = cx.new(|cx| SettingsDialog::new(window, cx));
        dialog.update(cx, |d, cx| d.dev_setup(window, cx));
        cx.subscribe_in(&dialog, window, |this, _, ev: &SettingsEvent, window, cx| match ev {
            SettingsEvent::Close => this.close_settings(window, cx),
            SettingsEvent::Changed => this.settings_changed(cx),
        })
        .detach();
        self.settings = Some(dialog);
        cx.notify();
    }

    /// Schließt den Dialog; der Fokus geht zurück an das Terminal der gewählten Session.
    fn close_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Fensterfüllend fährt der Dialog erst hinaus; die Tastatur geht sofort zurück (`OverlayPanel.dismiss`).
        if let Some(view) = self.settings.take() {
            self.begin_slide_out(view.into(), window, cx);
        }
        self.sync(window, cx);
    }
}

impl Shell {
    /// Übernimmt die Kopie des Dialogs in den echten Zustand und das Profil.
    fn settings_changed(&mut self, cx: &mut Context<Self>) {
        let new = store::snapshot(cx);
        let language_changed = crate::lang::apply(&new);
        crate::appearance::apply(cx, &new, false);
        self.state.apply_settings(new);
        if language_changed {
            cx.notify(); // zeichnet neu, `sync_language` baut Menüs und Ansicht
        }
    }
}
