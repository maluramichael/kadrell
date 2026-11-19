//! Die Kacheln im Zustand: welche Session einen Prozess hat, ob er läuft oder sich beendet hat, und was die
//! Oberfläche als Nächstes starten soll. Das Terminal selbst liegt hinter [`Term`] (GPUI-frei, von jedem Thread).

use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

use kadrell_claude::cli::LaunchSpec;

use super::agents::Start;
use super::lifecycle;
use super::AppState;

/// Zugriff auf den Prozess einer Kachel. `TermHandle` aus `kadrell-term` ist die echte Umsetzung, Tests nehmen eine
/// Attrappe.
pub trait Term {
    fn pid(&self) -> Option<u32>;
    /// Der Kindprozess ist beendet.
    fn exited(&self) -> bool;
    fn write(&self, bytes: &[u8]);
    fn bracketed_paste(&self) -> bool;
    /// Sichtbare Zeilen.
    fn visible_rows(&self) -> Vec<String>;
    /// Ganzer Puffer mit Verlauf.
    fn buffer_text(&self) -> String;
    /// Prozess und Kinder beenden.
    fn shutdown(&self);
    /// Nur die Schleife des Terminals beenden, den Prozess beendet der Aufrufer (`process::terminate_all`).
    fn close_loop(&self) {}
    /// Exit-Code des beendeten Prozesses.
    fn exit_code(&self) -> Option<i32> {
        None
    }
    /// Verlauf zum Zurückscrollen auf `lines` Zeilen stellen.
    fn set_scrollback(&self, _lines: usize) {}
}

impl Term for kadrell_term::TermHandle {
    fn pid(&self) -> Option<u32> {
        kadrell_term::TermHandle::pid(self)
    }
    fn exited(&self) -> bool {
        kadrell_term::TermHandle::exited(self)
    }
    fn write(&self, bytes: &[u8]) {
        kadrell_term::TermHandle::write(self, bytes);
    }
    fn bracketed_paste(&self) -> bool {
        kadrell_term::TermHandle::bracketed_paste(self)
    }
    fn visible_rows(&self) -> Vec<String> {
        kadrell_term::TermHandle::visible_rows(self)
    }
    fn buffer_text(&self) -> String {
        kadrell_term::TermHandle::buffer_text(self)
    }
    fn shutdown(&self) {
        kadrell_term::TermHandle::shutdown(self);
    }
    fn close_loop(&self) {
        kadrell_term::TermHandle::close_loop(self);
    }
    fn exit_code(&self) -> Option<i32> {
        kadrell_term::TermHandle::exit_code(self)
    }
    fn set_scrollback(&self, lines: usize) {
        kadrell_term::TermHandle::set_scrollback(self, lines);
    }
}

/// Lebenszustand der Kachel einer Session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Run {
    /// Noch nie gestartet (Lazy Start): startet, sobald die Session sichtbar wird.
    NotStarted,
    Running,
    /// Der Prozess hat sich selbst beendet (`/exit`, Absturz).
    Ended,
    /// Über `stop` oder die Oberfläche beendet.
    Stopped,
}

pub(super) struct Tile {
    pub term: Option<Rc<dyn Term>>,
    pub state: Run,
    /// Letzter Bildschirminhalt, für `capture` nach dem Ende und für die Kachel ohne Terminal.
    pub last_rows: Vec<String>,
    /// Zeitpunkt des Starts: stirbt der Prozess kurz danach mit Fehlercode, ist das ein Startfehler.
    pub started_at: Option<Instant>,
    /// Exit-Code eines Startfehlers, nur bei schnellem, unerwartetem Ende.
    pub exit_code: Option<i32>,
    /// Der Ordner der Session gibt es nicht mehr: kein Prozess gestartet.
    pub missing_folder: bool,
}

impl Tile {
    pub(super) fn blank() -> Tile {
        Tile::new()
    }

    fn new() -> Tile {
        Tile { term: None, state: Run::NotStarted, last_rows: Vec::new(), started_at: None, exit_code: None, missing_folder: false }
    }

    /// Neuer Anlauf: Spuren des letzten Endes weg.
    fn reset(&mut self) {
        self.last_rows.clear();
        self.exit_code = None;
        self.missing_folder = false;
    }
}

impl AppState {
    pub fn run(&self, key: &str) -> Run {
        let Some(tile) = self.tiles.get(key) else { return Run::NotStarted };
        match (&tile.state, &tile.term) {
            (Run::Running, Some(t)) if t.exited() => Run::Ended,
            (state, _) => *state,
        }
    }

    pub fn term(&self, key: &str) -> Option<Rc<dyn Term>> {
        self.tiles.get(key).and_then(|t| t.term.clone()).filter(|_| self.run(key) == Run::Running)
    }

    /// Key und pid des Prozesses jeder laufenden Kachel.
    pub fn pids(&self) -> HashMap<u32, String> {
        self.tiles.iter().filter(|(k, _)| self.run(k) == Run::Running).filter_map(|(k, t)| Some((t.term.as_ref()?.pid()?, k.clone()))).collect()
    }

    /// Läuft in der Kachel ein Prozess, den die Oberfläche anzeigt? Sonst zeigt sie den Schnappschuss.
    pub fn mounted(&self, key: &str) -> bool {
        self.tiles.get(key).is_some_and(|t| t.state == Run::Running && t.term.is_some())
    }

    /// Exit-Code eines Startfehlers (Prozess kurz nach dem Start mit Fehlercode gestorben).
    pub fn exit_code(&self, key: &str) -> Option<i32> {
        self.tiles.get(key).and_then(|t| t.exit_code)
    }

    /// Der Ordner der Session fehlt: kein Prozess gestartet.
    pub fn missing_folder(&self, key: &str) -> bool {
        self.tiles.get(key).is_some_and(|t| t.missing_folder)
    }

    /// Letzter Bildschirm einer beendeten Kachel ohne Leerzeilen, für die Anzeige statt des Terminals.
    pub fn snapshot(&self, key: &str) -> Vec<String> {
        self.last_rows(key)
    }

    /// Ordner weg (gelöscht, Worktree entfernt): kein Prozess, sonst startet der Klick auf „Klick setzt fort“ denselben
    /// kaputten Aufruf endlos neu. Remote-Sessions haben kein lokales `cwd`.
    fn folder_missing(&self, key: &str) -> bool {
        self.session(key).is_some_and(|s| !s.is_remote() && !std::path::Path::new(&s.cwd).exists())
    }

    /// Die Sessions, die jetzt starten: Vorgemerktes sofort, dazu die nächste Welle der Warteschlange.
    fn due_keys(&mut self, now: Instant) -> Vec<String> {
        let mut keys = std::mem::take(&mut self.wanted);
        for key in self.starts.next_wave(now) {
            if self.run(&key) == Run::NotStarted && !keys.contains(&key) {
                keys.push(key);
            }
        }
        keys
    }

    /// Sessions, die die Oberfläche jetzt starten soll, mit Startkommando. Das Ergebnis eines gescheiterten Starts
    /// (kein `claude`) steht als Fehler daneben, die Kachel gilt dann als beendet.
    pub fn take_starts(&mut self) -> Vec<(String, Result<LaunchSpec, String>)> {
        self.take_starts_at(Instant::now())
    }

    /// Wie [`take_starts`](Self::take_starts) mit vorgegebener Uhr (Tests der Wellen).
    pub fn take_starts_at(&mut self, now: Instant) -> Vec<(String, Result<LaunchSpec, String>)> {
        let mut out = Vec::new();
        for key in self.due_keys(now) {
            if self.session(&key).is_none() || self.run(&key) == Run::Running {
                continue;
            }
            if self.folder_missing(&key) {
                self.mark_missing_folder(&key);
                continue;
            }
            let prompt = self.prompts.get(&key).cloned();
            let spec = match self.start_spec(&key, prompt.as_deref()) {
                Start::Ready(spec) => spec,
                Start::Pending => {
                    self.wanted.push(key); // der Hintergrund bereitet den Start noch vor
                    continue;
                }
            };
            self.prompts.remove(&key);
            if let Err(message) = &spec {
                self.mark_ended(&key);
                self.error = Some(message.clone());
            }
            out.push((key, spec));
        }
        out
    }

    /// Wann die Oberfläche wieder `take_starts` aufrufen soll, solange eine Welle wartet.
    pub fn next_wave_in(&self) -> Option<Duration> {
        self.starts.wait(Instant::now())
    }

    /// Sichtbare Sessions ohne Prozess warten auf ihre Welle: der Fokus zuerst, dann die übrigen in Anzeigereihenfolge.
    pub(super) fn enqueue_visible(&mut self) {
        let mut keys: Vec<String> = self.focused.iter().chain(self.selected.iter()).cloned().collect();
        keys.dedup();
        let waiting = keys.into_iter().filter(|k| self.run(k) == Run::NotStarted && !self.wanted.contains(k));
        let waiting: Vec<String> = waiting.collect();
        self.starts.enqueue(waiting);
    }

    /// Angeklickte Sessions starten sofort, nicht erst mit der nächsten Welle (Swift: `select` ruft `attachNow`).
    pub fn start_selected_now(&mut self) {
        let selected = self.selected.clone();
        for key in self.starts.take_all(&selected) {
            if !self.wanted.contains(&key) {
                self.wanted.push(key);
            }
        }
    }

    /// Timer der Oberfläche für die nächste Welle: einmal armieren, wer ihn startet, ruft `timer_fired` auf.
    pub fn arm_wave_timer(&mut self) -> Option<Duration> {
        let wait = self.next_wave_in()?;
        if self.starts.timer_armed {
            return None;
        }
        self.starts.timer_armed = true;
        Some(wait)
    }

    pub fn wave_timer_fired(&mut self) {
        self.starts.timer_armed = false;
    }

    /// Die Oberfläche hat den Prozess gestartet.
    pub fn attach(&mut self, key: &str, term: Rc<dyn Term>) {
        let tile = self.tiles.entry(key.to_string()).or_insert_with(Tile::new);
        tile.reset();
        tile.term = Some(term);
        tile.state = Run::Running;
        tile.started_at = Some(Instant::now());
        self.update_attention();
    }

    /// Der Start ist gescheitert oder der Prozess war sofort weg.
    pub fn mark_ended(&mut self, key: &str) {
        let tile = self.tiles.entry(key.to_string()).or_insert_with(Tile::new);
        tile.state = Run::Ended;
    }

    fn mark_missing_folder(&mut self, key: &str) {
        let tile = self.tiles.entry(key.to_string()).or_insert_with(Tile::new);
        tile.reset();
        tile.missing_folder = true;
        tile.state = Run::Ended;
        self.prompts.remove(key);
    }

    /// Der Prozess hat sich beendet. Terminals ohne Claude verschwinden samt Eintrag, bei Claude hängt es von der
    /// Einstellung ab: die Kachel bleibt (Klick setzt fort) oder wird aus der Ansicht genommen. Gibt zurück, ob
    /// sich die Ansicht geändert hat. Das Terminal selbst wird dabei losgelassen, nur der Schnappschuss bleibt.
    pub fn on_exit(&mut self, key: &str) -> bool {
        let Some(tile) = self.tiles.get_mut(key) else { return false };
        if tile.state != Run::Running {
            tile.term = None; // gestoppt: auch hier hält nichts mehr das Terminal
            return false;
        }
        if let Some(term) = tile.term.take() {
            tile.last_rows = lifecycle::ended_rows(term.visible_rows());
            tile.exit_code = lifecycle::startup_failure(term.exit_code(), tile.started_at.map(|t| t.elapsed()));
        }
        tile.state = Run::Ended;
        if self.session(key).is_some_and(kadrell_core::Session::is_shell) {
            self.remove_sessions(&[key.to_string()]);
        } else if self.settings.close_tile_on_exit() {
            if self.selected.len() > 1 && self.selected.iter().any(|k| k == key) {
                self.select_keys(&[key.to_string()], true);
            }
            self.view_drop_everywhere(key);
        }
        self.update_attention();
        true
    }

    /// Beendet den Prozess, die Kachel bleibt mit dem letzten Bildschirm. Gibt `false` zurück, wenn nichts lief.
    pub fn stop_tile(&mut self, key: &str) -> bool {
        if self.run(key) != Run::Running {
            return false;
        }
        self.shutdown_tile(key);
        if let Some(tile) = self.tiles.get_mut(key) {
            tile.state = Run::Stopped;
            tile.term = None;
        }
        self.update_attention();
        true
    }

    /// Prozess (wieder) starten: die Kachel startet beim nächsten Abgleich der Oberfläche.
    pub fn request_start(&mut self, key: &str) -> Result<(), String> {
        let name = self.session(key).map(kadrell_core::Session::title).ok_or_else(|| "Session fehlt".to_string())?;
        if self.run(key) == Run::Running {
            return Err(format!("„{name}“ läuft schon"));
        }
        if let Some(tile) = self.tiles.get_mut(key) {
            tile.state = Run::NotStarted;
            tile.reset();
        }
        self.starts.forget(&[key.to_string()]);
        if !self.wanted.iter().any(|k| k == key) {
            self.wanted.push(key.to_string());
        }
        Ok(())
    }

    /// Merkt den Bildschirm und beendet den Prozess, falls er läuft.
    pub(super) fn shutdown_tile(&mut self, key: &str) {
        let Some(tile) = self.tiles.get_mut(key) else { return };
        let Some(term) = tile.term.clone() else { return };
        if !term.exited() {
            tile.last_rows = lifecycle::ended_rows(term.visible_rows());
            term.shutdown();
        }
    }

    /// Beenden der App: merkt die Bildschirme, schließt die Schleifen der Terminals und gibt die pids der noch
    /// laufenden Prozesse zurück. Die Oberfläche beendet sie mit `process::terminate_all` (5 s Frist, dann hart).
    pub fn shutdown_all(&mut self) -> Vec<u32> {
        let mut pids = Vec::new();
        for tile in self.tiles.values_mut() {
            let Some(term) = tile.term.clone().filter(|t| !t.exited()) else { continue };
            tile.last_rows = lifecycle::ended_rows(term.visible_rows());
            term.close_loop();
            pids.extend(term.pid());
        }
        self.models.shutdown(); // der eigene llama-server endet mit Kadrell
        pids
    }

    /// Die Einstellung „Verlauf zum Zurückscrollen“ auf die laufenden Terminals anwenden, wenn sie sich geändert hat;
    /// beim Verkleinern fallen die ältesten Zeilen weg. Neue Terminals bekommen den Wert beim Start (`scrollback`).
    pub fn apply_scrollback(&mut self) {
        let lines = self.scrollback();
        if lines == self.starts.applied_scrollback {
            return;
        }
        self.starts.applied_scrollback = lines;
        for term in self.tiles.values().filter_map(|t| t.term.as_ref()) {
            term.set_scrollback(lines);
        }
    }

    /// Zeilen Verlauf für neue Terminals (`terminal.scrollback`, 1000 bis 50000).
    pub fn scrollback(&self) -> usize {
        self.settings.terminal_scrollback() as usize
    }

    /// Zeilen einer beendeten Kachel für `capture` und die Anzeige.
    pub fn last_rows(&self, key: &str) -> Vec<String> {
        self.tiles.get(key).map(|t| t.last_rows.clone()).unwrap_or_default()
    }
}
