//! Tipp, Ansage und die Klänge für Öffnen, Schließen und Umschalten (Swift: `AttentionTracker.showTip/announce`,
//! `Feedback.play`). Die Entscheidung trifft der Tracker bzw. der Zustand, hier läuft, was die Oberfläche tun muss.

use std::time::Duration;

use gpui::Context;
use kadrell_claude::attention::{TIP_SECONDS, TipText};

use crate::Shell;
use crate::state::feedback::announcement;

impl Shell {
    /// Die vorgemerkten Klänge (neue Session, entfernt, umgeschaltet) spielen, durch die Einstellung „Sounds“ gefiltert.
    pub(crate) fn play_queued_sounds(&mut self) {
        let level = super::rules::sound_level(self.state.settings.sounds());
        for sound in self.state.take_sounds() {
            kadrell_tray::play(level, sound);
        }
    }

    /// Spielt sofort einen Klang der Oberfläche (grüne Pille, Gruppierung).
    pub(crate) fn play_sound(&mut self, sound: kadrell_tray::Sound) {
        self.state.queue_sound(sound);
        self.play_queued_sounds();
    }

    /// Tipp in der Leiste zeigen; nach 12 s verschwindet er von selbst, aber nur, wenn noch derselbe da steht.
    pub(crate) fn show_tip(&mut self, text: TipText, cx: &mut Context<Self>) {
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_secs(TIP_SECONDS)).await;
            let _ = this.update(cx, |shell, cx| {
                shell.state.attention.clear_tip(Some(&text));
                cx.notify();
            });
        })
        .detach();
    }

    /// Ansage für Bildschirmleser. GPUI kennt keine Live-Region, deshalb steht der Satz als Beschreibung am
    /// Status-Display der Leiste (`usage_bar`), wo ein Leser ihn beim nächsten Besuch findet.
    pub(crate) fn announce(&mut self, key: &str, waiting: bool, cx: &mut Context<Self>) {
        let title = self.state.session(key).map(kadrell_core::Session::title).unwrap_or_default();
        self.state.announcement = Some(announcement(&title, waiting));
        cx.notify();
    }
}
