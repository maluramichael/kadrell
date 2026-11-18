//! Aktionen der Leiste an der Shell (Swift: `AppDelegate` `bar.on…`): Layout-Menü, Sortierung reihum, Nachleuchten
//! der gedrückten Taste, „N warten“ wählt alle wartenden, „N trennen“, Tipp wegklicken, Befehl zum Aktualisieren kopieren.

use std::rc::Rc;
use std::time::Duration;

use gpui::{Context, Pixels, Point, Window, point, px};
use kadrell_core::SessionStatus;
use kadrell_tray::Sound;

use crate::Shell;
use crate::ui::bar_keys::{FLASH_MS, Flash, layout_title};
use crate::ui::context_menu::{Cmd, Item};
use crate::ui::layout::LayoutMode;

/// Einträge des Layout-Menüs: alle sieben Layouts in der Reihenfolge von ⌘L, das aktuelle mit Haken.
pub fn layout_items(current: LayoutMode) -> Vec<Item> {
    LayoutMode::ALL.iter().map(|m| Item::Entry(format!("{}{}", if *m == current { "✓ " } else { "    " }, layout_title(*m)), Cmd::Layout(*m))).collect()
}

impl Shell {
    /// Meldet eine gedrückte Taste: sie sitzt 80 ms eingesunken da. Bei „Bewegung reduzieren“ nicht.
    pub(crate) fn flash_bar(&mut self, id: &'static str, cx: &mut Context<Self>) {
        if kadrell_tray::reduce_motion() {
            return;
        }
        self.usage.pressed = Some(id);
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(FLASH_MS)).await;
            let _ = this.update(cx, |shell, cx| {
                if shell.usage.pressed == Some(id) {
                    shell.usage.pressed = None;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Wie `flash_bar`, für die Leiste, die nur eine Funktion bekommt.
    pub(crate) fn flash_fn(&self, cx: &mut Context<Self>) -> Flash {
        let this = cx.entity();
        Rc::new(move |id, app| this.update(app, |shell, cx| shell.flash_bar(id, cx)))
    }

    /// Layout-Menü unter der Layout-Taste.
    pub(crate) fn open_layout_menu(&mut self, at: Point<Pixels>, cx: &mut Context<Self>) {
        self.open_context(point(at.x - px(10.), at.y + px(14.)), layout_items(self.layout.mode), cx);
    }

    /// Wartende Sessions in Baumreihenfolge, unabhängig von eingeklappten Gruppen (`waitingIds`).
    pub(crate) fn waiting_keys(&self) -> Vec<String> {
        self.state.ordered_keys().into_iter().filter(|k| self.state.display_status(k) == Some(SessionStatus::Waiting)).collect()
    }

    /// „N warten“: alle wartenden Sessions zeigen (ersetzt die Auswahl).
    pub(crate) fn select_waiting(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let keys = self.waiting_keys();
        if keys.is_empty() {
            return;
        }
        self.state.select_keys(&keys, false);
        self.state.zoom = false;
        self.sync(window, cx);
    }

    /// „N trennen“: alle fertigen Sessions trennen. Ohne trennbare gibt es nur einen Piepton.
    pub(crate) fn detach_idle_pill(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let keys = self.detachable_keys();
        if keys.is_empty() {
            kadrell_tray::play(kadrell_tray::SoundLevel::All, Sound::Toggle);
            return;
        }
        self.play_sound(Sound::Close);
        for key in keys {
            self.state.stop_tile(&key);
        }
        self.sync(window, cx);
    }

    /// Tipp in der Leiste wegklicken.
    pub(crate) fn dismiss_tip(&mut self, cx: &mut Context<Self>) {
        self.state.dismiss_tip();
        cx.notify();
    }

    /// Versionswarnung: der Befehl zum Aktualisieren landet in der Zwischenablage.
    pub(crate) fn copy_update_command(&mut self, cx: &mut Context<Self>) {
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(kadrell_claude::cli::UPDATE_COMMAND.to_string()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_layout_menu_marks_the_current_mode() {
        let items = layout_items(LayoutMode::Scroll);
        assert_eq!(items.len(), 7);
        let marked: Vec<&String> = items.iter().filter_map(|i| if let Item::Entry(t, _) = i { Some(t) } else { None }).filter(|t| t.starts_with('✓')).collect();
        assert_eq!(marked.len(), 1);
        assert!(marked[0].ends_with("Scrollen"));
        assert!(matches!(&items[0], Item::Entry(t, Cmd::Layout(LayoutMode::Grid)) if t.ends_with("Grid")));
    }
}
