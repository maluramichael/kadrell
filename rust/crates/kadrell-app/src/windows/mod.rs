//! Weitere Fenster (⌘⇧T): `model` (offene Fenster, Besitzer der Terminals), `extra` (die Ansicht eines weiteren Fensters)
//! und die Verbindung zur `Shell` des Hauptfensters.

pub mod extra;
pub mod model;

use gpui::Context;

use crate::Shell;
use crate::state::attention::patch_setting;
use model::{OPEN_KEY, WindowSet};

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
        cx.defer(move |cx| {
            if let Some(handle) = extra::open(&shell, index, format!("Kadrell {}", index + 1), cx) {
                shell.update(cx, |sh, _| sh.extras.push((index, handle)));
            }
        });
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
