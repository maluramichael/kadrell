//! Weitere Fenster (⌘⇧T): `model` (offene Fenster, Besitzer der Terminals), `extra` (die Ansicht eines weiteren Fensters)
//! und die Verbindung zur `Shell` des Hauptfensters.

pub mod current;
pub mod extra;
pub mod extra_ext;
pub mod frame;
pub mod layout;
pub mod model;
pub mod split;
pub mod views;

use gpui::{Context, Div, InteractiveElement, KeyBinding, actions};
use kadrell_core::Settings;

use crate::Shell;
use crate::state::attention::patch_setting;
use model::{OPEN_KEY, WindowSet};

actions!(kadrell_windows, [ToggleSidebar, ToggleAuto]);

/// Taste für Baum ein/aus (⌘B, ⇧⌘B macht `ui::tree_actions`); unter Windows wie überall Alt statt ⌘.
pub fn bind_keys(cx: &mut gpui::App) {
    cx.bind_keys([KeyBinding::new(crate::keys::platform("cmd-b"), ToggleSidebar, None)]);
}

/// Aktionen der Ansicht am Wurzelelement des Hauptfensters.
pub fn attach_actions(root: Div, cx: &mut Context<Shell>) -> Div {
    root.on_action(cx.listener(|this, _: &ToggleSidebar, _, cx| this.toggle_tree(cx)))
        .on_action(cx.listener(|this, _: &ToggleAuto, window, cx| this.toggle_auto(window, cx)))
}

/// Zustand der Fenster in der `Shell`: Aufteilung des Hauptfensters (Breiten, Dock), das zuletzt aktive Fenster und
/// Änderungen, die verzögert in die Einstellungen gehen (beim Ziehen eines Trenners kommen viele).
pub struct WinState {
    pub sides: layout::Sides,
    /// Dialog, den ein weiteres Fenster im Hauptfenster angefordert hat (`current`).
    pub pin: Option<current::Pin>,
    /// Seitenleiste und Dock der weiteren Fenster, damit `refresh_ext` sie aktuell hält.
    pub extra_panels: std::collections::HashMap<usize, extra_ext::ExtraPanels>,
    pending: Vec<layout::Change>,
    generation: u64,
}

impl WinState {
    pub fn new(settings: &Settings) -> WinState {
        let mut sides = layout::Sides::restore(settings, 0);
        sides.apply_dev();
        WinState { sides, pin: None, extra_panels: Default::default(), pending: Vec::new(), generation: 0 }
    }
}

impl Shell {
    /// Meldet, was Fenster `window` zeigt, und gleicht die Besitzer der Terminals ab. Gibt zurück, ob sich dadurch ein
    /// Besitzer geändert hat (dann müssen die Fenster neu zeichnen).
    pub(crate) fn note_shown(&mut self, window: usize, keys: Vec<String>) -> bool {
        self.shown.insert(window, keys);
        let before = self.owners.clone();
        self.owners.reconcile(&self.shown);
        before != self.owners
    }

    /// Das Hauptfenster meldet seine sichtbaren Kacheln, vor dem Zeichnen der Kacheln.
    pub(crate) fn note_main_shown(&mut self) {
        let keys = self.visible().into_iter().filter_map(|(g, s)| Some(self.groups.get(g)?.sessions.get(s)?.key.clone())).collect();
        self.note_shown(0, keys);
    }

    /// ⌘⇧T: weiteres Fenster mit der kleinsten freien Nummer.
    pub(crate) fn new_window_action(&mut self, cx: &mut Context<Self>) {
        let index = self.windows.next_free();
        self.open_extra(index, cx);
    }

    fn open_extra(&mut self, index: usize, cx: &mut Context<Self>) {
        self.windows.open(index);
        patch_setting(&self.state.profile.settings_path(), OPEN_KEY, self.windows.to_value());
        // Das neue Fenster liest die Shell beim Aufbau, die gerade aktualisiert wird: erst danach öffnen.
        let shell = cx.entity();
        let title = self.window_title(index);
        cx.defer(move |cx| {
            let cascade = frame::cascade_source(&shell, cx);
            if let Some(handle) = extra::open(&shell, index, title, cascade, cx) {
                shell.update(cx, |sh, _| sh.extras.push((index, handle)));
            }
        });
    }

    /// Titel des Fensters: „Kadrell“, im benannten Profil mit dessen Namen, weitere Fenster mit „ · Fenster N“.
    pub(crate) fn window_title(&self, index: usize) -> String {
        let extra = if index == 0 { String::new() } else { kadrell_i18n::trd!(" · Fenster %lld", index + 1) };
        format!("Kadrell{}{extra}", self.state.profile.label())
    }

    /// Beim Start die zuletzt offenen Fenster wieder aufmachen; `KADRELL_DEV_WINDOWS=2` öffnet für Screenshots ein
    /// weiteres Fenster (n Fenster insgesamt).
    pub(crate) fn restore_windows(&mut self, cx: &mut Context<Self>) {
        let mut set = WindowSet::restore(self.state.settings.get(OPEN_KEY));
        if let Some(n) = std::env::var("KADRELL_DEV_WINDOWS").ok().and_then(|v| v.parse::<usize>().ok()) {
            for i in 1..n {
                set.open(i);
            }
        }
        for index in set.extras() {
            self.open_extra(index, cx);
        }
    }
}
