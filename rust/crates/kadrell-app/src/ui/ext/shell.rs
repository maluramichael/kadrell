//! Anbindung der Extension-Oberfläche an die `Shell`: Zustand (`ExtUi`), Weitergabe der Änderungen des Managers
//! (gebündelt), Seitenleisten, Statusleisten, Dialog, Tastaturfokus und die Einträge für ⌘P `>`.

use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use gpui::{AnyElement, App, AppContext, Context, Div, DragMoveEvent, Entity, Focusable, InteractiveElement, IntoElement, Window};
use kadrell_ext::bridge::{self, SettingsStore};
use kadrell_ext::http::UreqClient;
use kadrell_ext::manager::ExtensionManager;
use kadrell_ext::panel::{Dock, StatusItem};

use super::dialog::{DialogEvent, ExtensionsDialog};
use super::dialog_model::DialogModel;
use super::footer_bar::{empty_footer, footer_bar};
use super::footer_model::{self, FooterEntry};
use super::keys;
use super::model::PanelEntry;
use super::panel::{ExtPanel, PanelEvent};
use super::{FocusRightSidebar, OpenExtensions, ToggleRightSidebar, status_color};
use crate::Shell;
use crate::windows::{extra_ext, split};
use crate::ui::bar::{ExtStatus, Handler};
use crate::ui::context_menu::{Cmd, Item};
use crate::ui::palette::{Command, Run};
use kadrell_i18n::trd;
use crate::ui::theme;

/// Der Manager meldet bis zu 50-mal pro Sekunde (jede Logzeile): höchstens so oft wird neu gezeichnet.
const COALESCE: Duration = Duration::from_millis(100);
/// Ohne Einträge verschwindet die Leiste unten erst nach dieser Frist, falls nichts nachkommt.
const FOOTER_HIDE: Duration = Duration::from_millis(1500);

/// Zustand der Extension-Oberfläche in der `Shell`.
pub struct ExtUi {
    right: Entity<ExtPanel>,
    left: Entity<ExtPanel>,
    status: Vec<(String, StatusItem)>,
    footer: Vec<FooterEntry>,
    footer_shown: bool,
    footer_hiding: bool,
    pub dialog: Option<Entity<ExtensionsDialog>>,
}

impl ExtUi {
    pub fn new(manager: &ExtensionManager, cx: &mut Context<Shell>) -> Self {
        Self {
            right: cx.new(|cx| ExtPanel::new("ext-right", manager.clone(), cx)),
            left: cx.new(|cx| ExtPanel::new("ext-left", manager.clone(), cx)),
            status: Vec::new(),
            footer: Vec::new(),
            footer_shown: false,
            footer_hiding: false,
            dialog: None,
        }
    }

    /// Hat eine Seitenleiste oder der Dialog gerade die Tastatur? Dann darf die Shell sie nicht ans Terminal zurückgeben.
    pub fn keeps_focus(&self, window: &Window, cx: &App) -> bool {
        self.dialog.is_some() || self.right.read(cx).has_keyboard(window) || self.left.read(cx).has_keyboard(window)
    }
}

impl Shell {
    // MARK: Start und Änderungen

    /// Hört auf die Änderungen des Managers und auf die Ereignisse der Seitenleisten.
    pub(crate) fn start_ext_ui(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (right, left) = (self.ext_ui.right.clone(), self.ext_ui.left.clone());
        for panel in [right, left] {
            cx.subscribe_in(&panel, window, Self::on_ext_panel_event).detach();
        }
        let pings = crate::ext::changes();
        cx.spawn(async move |this, cx| {
            while pings.recv().await.is_ok() {
                cx.background_executor().timer(COALESCE).await;
                while pings.try_recv().is_ok() {}
                if this.update(cx, |shell, cx| shell.refresh_ext(cx)).is_err() {
                    break;
                }
            }
        })
        .detach();
        self.refresh_ext(cx);
    }

    /// Neuer Stand des Managers in alle Ansichten. Gezeichnet wird nur, wenn sich etwas geändert hat.
    fn refresh_ext(&mut self, cx: &mut Context<Self>) {
        let (right, left) = (self.ext.panels_in(Dock::Right), self.ext.panels_in(Dock::Left));
        self.leave_vanishing_tab(&right, &left, cx);
        self.ext_ui.right.update(cx, |p, cx| p.set_panels(right.clone(), cx));
        self.ext_ui.left.update(cx, |p, cx| p.set_panels(left.clone(), cx));
        for panels in self.win.extra_panels.values() {
            panels.set(right.clone(), left.clone(), cx);
        }
        let status = self.ext.status_items();
        if status != self.ext_ui.status {
            self.ext_ui.status = status;
            cx.notify();
        }
        self.refresh_footer(self.ext.footer_items(), cx);
        if let Some(dialog) = self.ext_ui.dialog.clone() {
            dialog.update(cx, |d, cx| d.changed(cx));
        }
    }

    /// Verschwindet der gewählte Tab einer Seitenleiste, die gerade die Tastatur hat, bekommt sie zuerst die Kachel
    /// zurück, bevor der Bereich womöglich wegfällt (`updateExtensionPanels`). Erst nach dem Zeichnen, denn dazu
    /// braucht es das Fenster.
    fn leave_vanishing_tab(&mut self, right: &[PanelEntry], left: &[PanelEntry], cx: &mut Context<Self>) {
        let panels = [(self.ext_ui.right.clone(), self.ext_ui.right.read(cx).model().loses_tab(right)), (self.ext_ui.left.clone(), self.ext_ui.left.read(cx).model().loses_tab(left))];
        let gone: Vec<Entity<ExtPanel>> = panels.into_iter().filter(|(_, lost)| *lost).map(|(panel, _)| panel).collect();
        let (Some(main), false) = (self.main_window, gone.is_empty()) else { return };
        let shell = cx.entity();
        cx.defer(move |cx| {
            let _ = cx.update_window(main, |_, window, cx| {
                shell.update(cx, |sh, cx| {
                    if gone.iter().any(|p| p.read(cx).has_keyboard(window)) {
                        sh.focus_terminal(window, cx);
                    }
                });
            });
        });
    }

    /// Mit Einträgen erscheint die Leiste sofort, leer verschwindet sie erst nach `FOOTER_HIDE`.
    fn refresh_footer(&mut self, entries: Vec<FooterEntry>, cx: &mut Context<Self>) {
        if entries == self.ext_ui.footer {
            return;
        }
        let empty = entries.is_empty();
        self.ext_ui.footer = entries;
        if !empty {
            self.ext_ui.footer_shown = true;
        } else if self.ext_ui.footer_shown && !self.ext_ui.footer_hiding {
            self.ext_ui.footer_hiding = true;
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(FOOTER_HIDE).await;
                this.update(cx, |shell, cx| {
                    shell.ext_ui.footer_hiding = false;
                    if shell.ext_ui.footer.is_empty() {
                        shell.ext_ui.footer_shown = false;
                    }
                    cx.notify();
                })
                .ok();
            })
            .detach();
        }
        cx.notify();
    }

    /// Klick auf einen Eintrag der Leisten oder Aktion eines Panels: Event `ui.action` an die Extension.
    pub(crate) fn ext_action(&self, name: &str, id: &str) {
        self.ext.action(name, id);
    }

    fn on_ext_panel_event(&mut self, _: &Entity<ExtPanel>, ev: &PanelEvent, window: &mut Window, cx: &mut Context<Self>) {
        match ev {
            PanelEvent::Leave => self.focus_terminal(window, cx),
            PanelEvent::Menu { position, tab, actions } => {
                let items = actions.iter().map(|a| Item::Entry(a.label.clone(), Cmd::Ext { tab: tab.clone(), id: a.id.clone() })).collect();
                self.open_context(*position, items, cx);
            }
        }
    }

    /// Die Tastatur geht zurück an das Terminal der gewählten Session.
    fn focus_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let focus = self.selected.and_then(|(g, s)| self.groups[g].sessions[s].pane.as_ref().map(|p| p.read(cx).focus.clone()));
        match focus {
            Some(f) => f.focus(window, cx),
            None => self.idle_focus.focus(window, cx),
        }
    }

    // MARK: Seitenleisten

    /// Strg+Alt+B: rechte Seitenleiste ein/aus (pro Fenster gespeichert). Hat sie dabei die Tastatur, geht sie ans Terminal.
    fn toggle_right_sidebar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let change = self.win.sides.toggle_right();
        self.queue_save(change, cx);
        if !self.win.sides.right_visible && self.ext_ui.right.read(cx).has_keyboard(window) {
            self.focus_terminal(window, cx);
        }
    }

    /// Strg+3: Tastatur in die rechte Seitenleiste (blendet sie bei Bedarf ein). Ohne Panels geschieht nichts.
    fn focus_right_sidebar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.ext_ui.right.read(cx).is_empty() {
            return;
        }
        if !self.win.sides.right_visible {
            let change = self.win.sides.toggle_right();
            self.queue_save(change, cx);
        }
        let panel = self.ext_ui.right.clone();
        cx.notify();
        // Das Panel steht erst im nächsten Frame im Baum, ein Fokus davor liefe ins Leere.
        cx.defer_in(window, move |_, window, cx| {
            let focus = panel.read(cx).focus_handle(cx);
            focus.focus(window, cx);
        });
    }

    /// Die linke Spalte: der Baum, darunter (mit Panels) das Dock der Extensions (`windows::extra_ext`).
    pub(crate) fn ext_left_column(&self, sidebar: impl IntoElement, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let t = theme(cx);
        // Die Spalte ist so hoch wie das Fenster ohne Titel und Leiste; genau wird sie erst beim Ziehen gemessen.
        let column_h = window.viewport_size().height.as_f32() - 62.;
        let on_drag = cx.listener(|this, ev: &DragMoveEvent<split::DockDrag>, _, cx| this.drag_dock(ev, cx));
        extra_ext::left_column(&self.win.sides, &self.ext_ui.left, sidebar, window, column_h, &t, cx, on_drag)
    }

    /// Die rechte Seitenleiste, nur mit Panels und solange sie nicht ausgeblendet ist.
    pub(crate) fn ext_right_panel(&self, window: &Window, cx: &App) -> Option<AnyElement> {
        extra_ext::right_panel(&self.win.sides, &self.ext_ui.right, window, &theme(cx), cx)
    }

    // MARK: Statusleisten

    /// Einträge für die obere Leiste: je Extension einer, mit Klick, wenn er eine Aktion hat.
    pub(crate) fn ext_status(&self, cx: &mut Context<Self>) -> Vec<ExtStatus> {
        let t = theme(cx);
        self.ext_ui
            .status
            .iter()
            .map(|(name, item)| {
                let click: Option<Handler> = item.action.clone().map(|action| {
                    let name = name.clone();
                    Rc::new(cx.listener(move |this, _, _, _| this.ext_action(&name, &action))) as Handler
                });
                ExtStatus { name: name.clone(), text: item.text.clone(), dot: item.color.map(|c| status_color(c, &t)), click }
            })
            .collect()
    }

    /// Die Leiste unten: nur solange eine Extension etwas einträgt.
    pub(crate) fn ext_footer(&self, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.ext_ui.footer_shown {
            return None;
        }
        if self.ext_ui.footer.is_empty() {
            return Some(empty_footer(cx).into_any_element());
        }
        Some(footer_bar(&self.ext_ui.footer, window, cx).into_any_element())
    }

    /// Kommandos für ⌘P `>`: jeder Eintrag der Leisten (oben und unten) mit Aktion, damit er ohne Maus auslösbar ist.
    pub(crate) fn ext_palette_commands(&self) -> Vec<Command> {
        footer_model::commands(&self.ext.status_items(), &self.ext.footer_items())
            .into_iter()
            .map(|(name, what, id)| Command { title: trd!("Statusleiste: %@ · %@", name.clone(), what), run: Run::Ext { name, id } })
            .collect()
    }

    // MARK: Dialog

    /// Das eigene Kürzel öffnet den Dialog und schließt ihn wieder (wie in der Mac-App).
    pub(crate) fn toggle_extensions(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.ext_ui.dialog.is_some() {
            self.close_extensions(window, cx);
        } else if !self.overlays.ext_started {
            kadrell_tray::beep();
        } else if self.panel_open() {
            self.overlays.defer(Box::new(|shell, window, cx| shell.open_extensions(window, cx)));
        } else if self.overlay.is_none() && self.palette.is_none() && self.settings.is_none() && self.rename.is_none() {
            self.open_extensions(window, cx);
        }
    }

    fn open_extensions(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        kadrell_ext::i18n::set_locale(kadrell_i18n::global().lang().code());
        let store: Arc<dyn SettingsStore> = Arc::new(crate::ext::ProfileSettingsStore::new(self.state.profile.settings_path()));
        let model = DialogModel::new(self.ext.clone(), store);
        let hint = keys::close_hint(&self.state.settings, cfg!(target_os = "macos")).unwrap_or_default();
        let dialog = cx.new(|cx| ExtensionsDialog::new(model, Arc::new(UreqClient), hint, window, cx));
        cx.subscribe_in(&dialog, window, |this, _, _: &DialogEvent, window, cx| this.close_extensions(window, cx)).detach();
        self.ext_ui.dialog = Some(dialog);
        cx.notify();
    }

    fn close_extensions(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.ext_ui.dialog = None;
        self.sync(window, cx);
    }

    // MARK: Entwicklung

    /// Für Screenshots ohne Eingabe: `KADRELL_DEV_OVERLAY=extensions` öffnet den Dialog (`KADRELL_DEV_DELAY_MS` wartet
    /// vorher, bis die Extensions gestartet sind).
    pub(crate) fn open_extensions_from_env(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if std::env::var("KADRELL_DEV_OVERLAY").as_deref() != Ok("extensions") {
            return;
        }
        let delay = std::env::var("KADRELL_DEV_DELAY_MS").ok().and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(delay)).await;
            this.update_in(cx, |shell, window, cx| shell.open_extensions_dev(window, cx)).ok();
        })
        .detach();
    }

    /// `KADRELL_DEV_EXT_SELECT=<n>` wählt eine Zeile,
    /// `KADRELL_DEV_EXT_LOG=1` blendet das Log ein.
    fn open_extensions_dev(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_extensions(window, cx);
        let Some(dialog) = self.ext_ui.dialog.clone() else { return };
        let var = |name: &str| std::env::var(format!("KADRELL_DEV_EXT_{name}")).ok();
        dialog.update(cx, |d, cx| {
            if let Some(n) = var("SELECT").and_then(|v| v.parse::<usize>().ok()) {
                d.model.select(n);
            }
            d.model.show_log = var("LOG").is_some();
            cx.notify();
        });
    }

    /// `KADRELL_DEV_EXT_ENABLE=all` schaltet im Wegwerfprofil alle gefundenen Extensions ein und bestätigt sie
    /// (sonst wäre ihr Panel auf dem Screenshot leer). Nie im Standardprofil oder einem benannten Profil.
    pub(crate) fn ext_dev_enable(&self) {
        if std::env::var("KADRELL_DEV_EXT_ENABLE").as_deref() != Ok("all") || !self.state.profile.temporary {
            return;
        }
        let store = crate::ext::ProfileSettingsStore::new(self.state.profile.settings_path());
        for found in self.ext.found().into_iter().filter(|f| f.problem.is_none()) {
            bridge::acknowledge_extension(&store, &found.name);
            self.ext.set_enabled(&found.name, true);
        }
    }
}

/// Aktionen der Extension-Oberfläche am Wurzelelement des Fensters (Menü und Tastatur).
pub fn attach_actions(root: Div, cx: &mut Context<Shell>) -> Div {
    root.on_action(cx.listener(|this, _: &OpenExtensions, window, cx| this.toggle_extensions(window, cx)))
        .on_action(cx.listener(|this, _: &FocusRightSidebar, window, cx| this.focus_right_sidebar(window, cx)))
        .on_action(cx.listener(|this, _: &ToggleRightSidebar, window, cx| this.toggle_right_sidebar(window, cx)))
}
