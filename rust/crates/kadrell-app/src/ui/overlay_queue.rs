//! Verwaltung der Overlays wie `SheetPresenter.swift`: Solange ein Panel offen ist (Hilfe/Über, Statistik, Extensions),
//! wird ein neuer Dialog vorgemerkt und erst gezeigt, wenn das Panel zu ist (wer gerade ⏎ drückt, um es zu schließen,
//! soll nicht aus Versehen einen anderen Dialog bestätigen). Ein offener Dialog wird vom neuen ersetzt, eine offene
//! Palette macht ihm Platz. Dazu: Esc schließt einen sichtbaren Dialog, der den Fokus verloren hat (kein Softlock),
//! die Fehlermeldung der Claude-CLI und das Piepen, wenn die Extensions noch nicht laufen. Die Ausfahrt der
//! fensterfüllenden Overlays hält ihre Ansicht noch kurz am Leben (`overlay_anim`).

use std::time::Duration;

use gpui::{AnyView, Context, Div, ParentElement, Styled, Window, div};
use kadrell_i18n::trd;

use super::overlay_anim::{out_secs, set_leaving};
use crate::Shell;
use crate::misc::confirm::Spec;

/// Öffnet einen Dialog in der Shell, sobald er an der Reihe ist.
pub type Opener = Box<dyn FnOnce(&mut Shell, &mut Window, &mut Context<Shell>)>;

/// Zustand der Overlay-Verwaltung, ein Feld der Shell.
#[derive(Default)]
pub struct OverlayState {
    /// Der vorgemerkte Dialog; ein neuerer ersetzt einen älteren (`SheetPresenter.pending`).
    pending: Option<Opener>,
    /// Fensterfüllendes Overlay, das noch hinausfährt.
    closing: Hold<AnyView>,
    /// Die Extensions laufen (`ExtensionManager::start` ist gerufen); vorher piept das Panel (`togglePanel(.extensions)`).
    pub ext_started: bool,
}

/// Hält ein geschlossenes Overlay bis zum Ende seiner Ausfahrt. Jede Ausfahrt bekommt eine Nummer, damit die Uhr einer
/// alten keine neuere beendet.
pub struct Hold<T> {
    item: Option<T>,
    generation: u64,
}

impl<T> Default for Hold<T> {
    fn default() -> Self {
        Hold { item: None, generation: 0 }
    }
}

impl<T> Hold<T> {
    /// Hält `item`; gibt die Nummer der Ausfahrt zurück. Eine noch laufende wird dadurch abgelöst.
    pub fn hold(&mut self, item: T) -> u64 {
        self.generation += 1;
        self.item = Some(item);
        self.generation
    }

    /// Das Ende der Ausfahrt `generation`: lässt los, aber nur, wenn keine neuere läuft. `true`: losgelassen.
    pub fn release(&mut self, generation: u64) -> bool {
        let current = self.generation == generation && self.item.is_some();
        if current {
            self.item = None;
        }
        current
    }

    /// Lässt sofort los (neues Overlay, Ende der Zeit).
    pub fn clear(&mut self) {
        self.item = None;
    }

    /// Das gehaltene Stück, solange es hinausfährt. Danach folgt nichts mehr dem Fenster.
    pub fn get(&self) -> Option<&T> {
        self.item.as_ref()
    }
}

/// Darf ein neuer Dialog sofort erscheinen? Nicht, solange ein Panel offen ist.
pub fn shows_now(panel_open: bool) -> bool {
    !panel_open
}

impl OverlayState {
    /// Merkt einen Dialog vor; ersetzt einen schon vorgemerkten.
    pub fn defer(&mut self, open: Opener) {
        self.pending = Some(open);
    }

    pub fn take_pending(&mut self) -> Option<Opener> {
        self.pending.take()
    }

    pub fn has_pending(&self) -> bool {
        self.pending.is_some()
    }
}

impl Shell {
    /// Ein Panel ist offen: Hilfe/Über, Statistik oder Extensions. Sie schließen sich mit der eigenen Taste.
    pub(crate) fn panel_open(&self) -> bool {
        self.misc.help.is_some() || self.usage.stats.is_some() || self.ext_ui.dialog.is_some()
    }

    /// Zeigt einen Dialog: bei offenem Panel vorgemerkt bis zu dessen Schließen, sonst sofort. Die Palette macht Platz,
    /// ein offener einfacher Dialog wird ersetzt.
    pub(crate) fn present_overlay(&mut self, open: Opener, window: &mut Window, cx: &mut Context<Self>) {
        if !shows_now(self.panel_open()) {
            self.overlays.defer(open);
            return;
        }
        self.close_plain_dialogs();
        self.palette = None;
        self.finish_slide_out();
        open(self, window, cx);
    }

    /// Schließt offene einfache Dialoge (nicht die Rückfragen und Panels), wenn ein neuer kommt.
    fn close_plain_dialogs(&mut self) {
        self.overlay = None;
        self.rename = None;
        self.quick_start = None;
        self.agent_pick = None;
        self.remote = None;
        self.groups_ui.edit = None;
    }

    /// Nach jedem Schließen: ist kein Panel mehr offen, kommt der vorgemerkte Dialog dran.
    pub(crate) fn run_pending_overlay(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.panel_open() {
            return;
        }
        if let Some(open) = self.overlays.take_pending() {
            self.present_overlay(open, window, cx);
        }
        self.show_next_confirm(window, cx);
    }

    // MARK: Ausfahrt

    /// Hält ein geschlossenes fensterfüllendes Overlay, bis es hinausgefahren ist. Die Tastatur ist da schon zurück.
    pub(crate) fn begin_slide_out(&mut self, view: AnyView, window: &mut Window, cx: &mut Context<Self>) {
        let reduce = crate::appearance::breathing::reduce_motion();
        let generation = self.overlays.closing.hold(view);
        set_leaving(true);
        let wait = Duration::from_secs_f32(out_secs(reduce));
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(wait).await;
            this.update(cx, |shell, cx| {
                if shell.overlays.closing.release(generation) {
                    shell.finish_slide_out();
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    /// Beendet eine laufende Ausfahrt sofort (neues Overlay, Ende der Zeit).
    pub(crate) fn finish_slide_out(&mut self) {
        self.overlays.closing.clear();
        set_leaving(false);
    }

    /// Das ausfahrende Overlay, gezeichnet über dem Fenster. Seine Abdunklung fängt keine Klicks mehr ab
    /// (`backdrop_full` liest `leaving`), sie gehen ans Hauptfenster.
    pub(crate) fn closing_layer(&self) -> Option<Div> {
        let view = self.overlays.closing.get()?.clone();
        Some(div().absolute().top_0().left_0().size_full().child(view))
    }

    // MARK: Esc ohne Fokus

    /// Ein Dialog ist offen, hat aber nicht mehr den Fokus (nach Dropdown, Klick daneben, App-Wechsel): Esc landet dann am
    /// Hauptfenster. Ohne das schlösse nichts den Dialog und die Abdunklung blockte alle Klicks. `true`: geschlossen.
    pub(crate) fn cancel_visible(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if !self.plain_overlay_open() || !self.main_window_has_focus(window, cx) {
            return false;
        }
        self.close_plain_dialogs();
        self.settings = None;
        self.palette = None;
        self.sync(window, cx);
        true
    }

    fn plain_overlay_open(&self) -> bool {
        self.overlay.is_some()
            || self.rename.is_some()
            || self.quick_start.is_some()
            || self.agent_pick.is_some()
            || self.remote.is_some()
            || self.settings.is_some()
            || self.palette.is_some()
            || self.groups_ui.edit.is_some()
    }

    /// Hat das Hauptfenster die Tastatur: kein Fokus, die Leerlauf-Fläche der Shell oder ein Terminal? Genau dann kam Esc
    /// am Hauptfenster an (`cancelVisible`); jeder Fokus in einem Dialog (auch in dessen zweitem Feld) gehört dem Dialog.
    fn main_window_has_focus(&self, window: &Window, cx: &gpui::App) -> bool {
        let Some(focused) = window.focused(cx) else { return true };
        focused == self.idle_focus
            || self.groups.iter().flat_map(|g| g.sessions.iter()).filter_map(|s| s.pane.as_ref()).any(|p| p.read(cx).focus == focused)
    }

    // MARK: Meldungen

    /// Fehlermeldung der Claude-CLI: ins Log (privat) und als Mitteilung mit der ersten Zeile der Ausgabe (`report`).
    pub(crate) fn report(&mut self, detail: &str, window: &mut Window, cx: &mut Context<Self>) {
        let line = first_line(detail);
        eprintln!("kadrell: {line}");
        self.ask(Spec { title: trd!("Claude CLI meldet einen Fehler"), text: line, button: trd!("OK"), destructive: false, notice: true }, None, window, cx);
    }
}

/// Stempel der Programmdatei (Änderungszeit und Größe), `None` wenn sie sich nicht lesen lässt.
fn binary_stamp() -> Option<(std::time::SystemTime, u64)> {
    let meta = std::fs::metadata(std::env::current_exe().ok()?).ok()?;
    Some((meta.modified().ok()?, meta.len()))
}

static LAUNCH_STAMP: std::sync::OnceLock<Option<(std::time::SystemTime, u64)>> = std::sync::OnceLock::new();

/// Merkt sich die Programmdatei beim Start (der erste Frame ruft es).
pub fn note_launch() {
    LAUNCH_STAMP.get_or_init(binary_stamp);
}

/// Wurde die App ersetzt, während sie läuft? Nur am Mac ein Problem: dort verwirft macOS den Dateidialog wegen der
/// geänderten Signatur (`AppBinary.replaced`). Unter Windows sperrt das System die laufende Datei, der Dialog des
/// Systems geht immer.
pub fn binary_replaced() -> bool {
    cfg!(target_os = "macos") && LAUNCH_STAMP.get().is_some_and(|at| *at != binary_stamp())
}

/// Warnt statt den Dateidialog zu öffnen, wenn die App ersetzt wurde. `true`: gewarnt, nichts öffnen.
pub fn warn_if_replaced(window: &mut Window, cx: &mut gpui::App) -> bool {
    if !binary_replaced() {
        return false;
    }
    let detail = trd!("Der Dateiauswahl-Dialog öffnet sich erst nach einem Neustart von Kadrell. Pfade lassen sich weiter eintippen oder hineinziehen.");
    let answer = window.prompt(gpui::PromptLevel::Warning, &trd!("Kadrell wurde seit dem Start ersetzt"), Some(&detail), &["OK"], cx);
    window.spawn(cx, async move |_| {
        let _ = answer.await;
    })
    .detach();
    true
}

/// Erste nichtleere Zeile einer Ausgabe, ohne Ränder.
pub fn first_line(output: &str) -> String {
    output.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("").to_string()
}

/// Systemton für „geht nicht“ (`NSSound.beep`, Windows `MessageBeep`).
pub fn beep() {
    #[cfg(target_os = "windows")]
    // SAFETY: MessageBeep hat keine Vorbedingungen; 0xFFFFFFFF ist der Standardton.
    unsafe {
        let _ = MessageBeep(0xFFFF_FFFF);
    }
    #[cfg(target_os = "macos")]
    // SAFETY: NSBeep (AppKit) hat keine Parameter.
    unsafe {
        NSBeep();
    }
}

#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn NSBeep();
}

#[cfg(target_os = "windows")]
#[link(name = "user32")]
unsafe extern "system" {
    fn MessageBeep(kind: u32) -> i32;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dialog_waits_while_a_panel_is_open() {
        assert!(!shows_now(true));
        assert!(shows_now(false));
    }

    #[test]
    fn a_newer_pending_dialog_replaces_an_older_one() {
        let mut state = OverlayState::default();
        assert!(!state.has_pending());
        state.defer(Box::new(|_, _, _| {}));
        state.defer(Box::new(|_, _, _| {}));
        assert!(state.has_pending());
        assert!(state.take_pending().is_some());
        assert!(!state.has_pending());
        assert!(state.take_pending().is_none());
    }

    /// `testFullWindowSlidesOutBeforeClosing`: nach dem Schließen wird das Overlay noch gehalten (es fährt hinaus), nach
    /// der Ausfahrt ist es weg.
    #[test]
    fn a_closed_overlay_is_held_until_it_has_slid_out() {
        let mut hold: Hold<&str> = Hold::default();
        assert!(hold.get().is_none());
        let slide = hold.hold("Einstellungen");
        assert_eq!(hold.get(), Some(&"Einstellungen"), "fährt noch hinaus");
        assert!(hold.release(slide));
        assert!(hold.get().is_none(), "nach der Ausfahrt weg");
    }

    /// Die Uhr einer alten Ausfahrt beendet keine neuere: wer schnell wieder öffnet und schließt, sieht die ganze Ausfahrt.
    #[test]
    fn an_old_clock_does_not_end_a_newer_slide_out() {
        let mut hold: Hold<&str> = Hold::default();
        let old = hold.hold("Statistik");
        let newer = hold.hold("Einstellungen");
        assert!(!hold.release(old));
        assert_eq!(hold.get(), Some(&"Einstellungen"));
        assert!(hold.release(newer));
        assert!(!hold.release(newer), "ein zweites Ende ändert nichts");
    }

    /// `testDismissedOverlayNoLongerFollows`: ein geschlossenes Overlay hängt nicht mehr am Fenster. Gezeichnet wird nur,
    /// was die Shell hält; nach dem Loslassen kann eine Größenänderung nichts mehr bewegen, weil nichts mehr zu bewegen ist.
    #[test]
    fn a_dismissed_overlay_no_longer_follows_the_window() {
        let mut hold: Hold<&str> = Hold::default();
        let slide = hold.hold("Einstellungen");
        hold.clear();
        let before = hold.get().copied();
        super::super::overlay::set_area(super::super::overlay::Area { width: 900., height: 600., ..Default::default() });
        assert_eq!(hold.get().copied(), before);
        assert!(!hold.release(slide), "ein später Ablauf der Uhr findet nichts mehr");
    }

    #[test]
    fn report_shows_only_the_first_output_line() {
        assert_eq!(first_line("\n  Fehler: kaputt\nzweite Zeile"), "Fehler: kaputt");
        assert_eq!(first_line(""), "");
    }
}
