//! Die Kacheln im Zustand: welche Session einen Prozess hat, ob er läuft oder sich beendet hat, und was die
//! Oberfläche als Nächstes starten soll. Das Terminal selbst liegt hinter [`Term`] (GPUI-frei, von jedem Thread).

use std::collections::HashMap;
use std::rc::Rc;

use kadrell_claude::cli::LaunchSpec;

use super::agents::Start;
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
    /// Letzter Bildschirminhalt, für `capture` nach dem Ende.
    pub last_rows: Vec<String>,
}

impl Tile {
    fn new() -> Tile {
        Tile { term: None, state: Run::NotStarted, last_rows: Vec::new() }
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

    /// Sessions, die die Oberfläche jetzt starten soll, mit Startkommando. Das Ergebnis eines gescheiterten Starts
    /// (kein `claude`) steht als Fehler daneben, die Kachel gilt dann als beendet.
    pub fn take_starts(&mut self) -> Vec<(String, Result<LaunchSpec, String>)> {
        let keys = std::mem::take(&mut self.wanted);
        let mut out = Vec::new();
        for key in keys {
            if self.session(&key).is_none() || self.run(&key) == Run::Running {
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

    /// Die Oberfläche hat den Prozess gestartet.
    pub fn attach(&mut self, key: &str, term: Rc<dyn Term>) {
        let tile = self.tiles.entry(key.to_string()).or_insert_with(Tile::new);
        tile.term = Some(term);
        tile.state = Run::Running;
        self.update_attention();
    }

    /// Der Start ist gescheitert oder der Prozess war sofort weg.
    pub fn mark_ended(&mut self, key: &str) {
        let tile = self.tiles.entry(key.to_string()).or_insert_with(Tile::new);
        tile.state = Run::Ended;
    }

    /// Der Prozess hat sich beendet. Terminals ohne Claude verschwinden samt Eintrag, bei Claude hängt es von der
    /// Einstellung ab: die Kachel bleibt (Klick setzt fort) oder wird aus der Ansicht genommen. Gibt zurück, ob
    /// sich die Ansicht geändert hat.
    pub fn on_exit(&mut self, key: &str) -> bool {
        let Some(tile) = self.tiles.get_mut(key) else { return false };
        if tile.state != Run::Running {
            return false;
        }
        tile.last_rows = tile.term.as_ref().map(|t| t.visible_rows()).unwrap_or_default();
        tile.state = Run::Ended;
        if self.session(key).is_some_and(kadrell_core::Session::is_shell) {
            self.remove_sessions(&[key.to_string()]);
        } else if self.settings.close_tile_on_exit() && self.selected.len() > 1 && self.selected.iter().any(|k| k == key) {
            self.select_keys(&[key.to_string()], true);
        }
        self.update_attention();
        true
    }

    /// Beendet den Prozess, die Kachel bleibt. Gibt `false` zurück, wenn nichts lief.
    pub fn stop_tile(&mut self, key: &str) -> bool {
        if self.run(key) != Run::Running {
            return false;
        }
        self.shutdown_tile(key);
        if let Some(tile) = self.tiles.get_mut(key) {
            tile.state = Run::Stopped;
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
        }
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
            tile.last_rows = term.visible_rows();
            term.shutdown();
        }
    }

    /// Beendet alle Prozesse (Beenden der App).
    pub fn shutdown_all(&mut self) {
        let keys: Vec<String> = self.tiles.keys().cloned().collect();
        for key in keys {
            self.shutdown_tile(&key);
        }
        self.models.shutdown(); // der eigene llama-server endet mit Kadrell
    }

    /// Zeilen einer beendeten Kachel für `capture`.
    pub fn last_rows(&self, key: &str) -> Vec<String> {
        self.tiles.get(key).map(|t| t.last_rows.clone()).unwrap_or_default()
    }
}
