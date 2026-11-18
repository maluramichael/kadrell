//! Lebenslauf der Kacheln, Regeln aus `AttachManager`: Lazy Start in Wellen, fehlender Ordner, Startfehler mit
//! Exit-Code, Schnappschuss der letzten Zeilen, Verlauf der Terminals und Beenden mit Wartezeit.

use std::time::{Duration, Instant};

/// Sessions je Welle: bei vielen sichtbaren Kacheln lebt sonst die letzte erst nach mehreren Sekunden.
pub const BATCH_SIZE: usize = 4;
/// Abstand der Wellen.
pub const WAVE_GAP: Duration = Duration::from_millis(500);
/// Stirbt ein Prozess so kurz nach dem Start mit Fehlercode, ist das ein Startfehler, kein normales `/exit`.
pub const STARTUP_WINDOW: Duration = Duration::from_secs(5);

/// Warteschlange des Lazy Starts samt Takt der Wellen.
#[derive(Default)]
pub struct Starts {
    /// Sessions, die in Wellen starten (sichtbare zuerst, Fokus ganz vorn).
    queue: Vec<String>,
    next_wave: Option<Instant>,
    /// Läuft schon ein Timer der Oberfläche für die nächste Welle?
    pub timer_armed: bool,
    /// Verlauf, der zuletzt auf die laufenden Terminals angewendet wurde.
    pub applied_scrollback: usize,
    /// Die gespeicherte Auswahl wird gerade wiederhergestellt: ihre Kacheln starten in Wellen, nicht sofort.
    pub restoring: bool,
}

impl Starts {
    /// Merkt `keys` vor, Reihenfolge wie übergeben, ohne Doppelte.
    pub fn enqueue(&mut self, keys: impl IntoIterator<Item = String>) {
        for key in keys {
            if !self.queue.contains(&key) {
                self.queue.push(key);
            }
        }
    }

    pub fn forget(&mut self, keys: &[String]) {
        self.queue.retain(|k| !keys.contains(k));
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    /// Nimmt die nächste Welle aus der Warteschlange, wenn ihre Zeit da ist.
    pub fn next_wave(&mut self, now: Instant) -> Vec<String> {
        if self.queue.is_empty() || self.next_wave.is_some_and(|t| now < t) {
            return Vec::new();
        }
        let wave: Vec<String> = self.queue.drain(..BATCH_SIZE.min(self.queue.len())).collect();
        self.next_wave = (!self.queue.is_empty()).then_some(now + WAVE_GAP);
        wave
    }

    /// Wann die Oberfläche wieder nachsehen soll; `None`: nichts wartet.
    pub fn wait(&self, now: Instant) -> Option<Duration> {
        if self.queue.is_empty() {
            return None;
        }
        Some(self.next_wave.map_or(Duration::ZERO, |t| t.saturating_duration_since(now)))
    }

    /// Alles Wartende sofort (Klick, `resume`): gibt die Schlüssel zurück und leert die Schlüsselliste.
    pub fn take_all(&mut self, keys: &[String]) -> Vec<String> {
        let now: Vec<String> = self.queue.iter().filter(|k| keys.contains(k)).cloned().collect();
        self.forget(&now);
        now
    }
}

/// Exit-Code eines Startfehlers: ≠ 0 und innerhalb von [`STARTUP_WINDOW`] nach dem Start.
pub fn startup_failure(code: Option<i32>, since_start: Option<Duration>) -> Option<i32> {
    code.filter(|c| *c != 0 && since_start.is_some_and(|d| d < STARTUP_WINDOW))
}

/// Letzter Bildschirm eines beendeten Prozesses: alle leeren Zeilen weg (`snapshotRows(trimTrailing: false)`).
pub fn ended_rows(rows: Vec<String>) -> Vec<String> {
    rows.into_iter().filter(|r| !r.trim().is_empty()).collect()
}

/// Bildschirm eines laufenden Terminals ohne Fenster: Leerraum am Zeilenende und leere Zeilen am Ende weg
/// (`snapshotRows(trimTrailing: true)`).
pub fn trimmed_rows(rows: Vec<String>) -> Vec<String> {
    let mut rows: Vec<String> = rows.into_iter().map(|r| r.trim_end().to_string()).collect();
    while rows.last().is_some_and(String::is_empty) {
        rows.pop();
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(n: usize) -> Vec<String> {
        (0..n).map(|i| format!("k{i}")).collect()
    }

    #[test]
    fn waves_start_four_sessions_every_500_ms_visible_first() {
        let mut s = Starts::default();
        s.enqueue(keys(10));
        let t0 = Instant::now();
        assert_eq!(s.next_wave(t0), keys(4));
        assert!(s.next_wave(t0).is_empty(), "erst nach 500 ms die nächste");
        assert_eq!(s.wait(t0), Some(WAVE_GAP));
        assert!(s.next_wave(t0 + Duration::from_millis(499)).is_empty());
        assert_eq!(s.next_wave(t0 + WAVE_GAP), keys(8)[4..].to_vec());
        assert_eq!(s.next_wave(t0 + WAVE_GAP * 2), keys(10)[8..].to_vec());
        assert_eq!(s.wait(t0 + WAVE_GAP * 2), None);
        s.enqueue(keys(1));
        assert_eq!(s.next_wave(t0 + WAVE_GAP * 2), keys(1), "eine leere Schlange wartet nicht auf den Takt");
    }

    #[test]
    fn enqueue_ignores_duplicates_and_forget_removes() {
        let mut s = Starts::default();
        s.enqueue(keys(3));
        s.enqueue(keys(5));
        s.forget(&keys(1));
        assert_eq!(s.next_wave(Instant::now()), keys(5)[1..].to_vec());
    }

    #[test]
    fn a_click_takes_its_sessions_out_of_the_queue() {
        let mut s = Starts::default();
        s.enqueue(keys(5));
        assert_eq!(s.take_all(&["k3".to_string(), "x".to_string()]), vec!["k3".to_string()]);
        assert!(!s.next_wave(Instant::now()).contains(&"k3".to_string()));
    }

    #[test]
    fn only_a_quick_nonzero_exit_is_a_startup_failure() {
        let quick = Some(Duration::from_secs(2));
        assert_eq!(startup_failure(Some(1), quick), Some(1));
        assert_eq!(startup_failure(Some(0), quick), None, "reguläres /exit");
        assert_eq!(startup_failure(None, quick), None, "Signal statt Code");
        assert_eq!(startup_failure(Some(1), Some(Duration::from_secs(5))), None, "zu spät");
        assert_eq!(startup_failure(Some(1), None), None);
    }

    #[test]
    fn snapshots_drop_blank_lines_or_trim_the_tail() {
        let rows = || vec!["a  ".to_string(), String::new(), "  ".to_string(), "b".to_string(), "  ".to_string(), String::new()];
        assert_eq!(ended_rows(rows()), vec!["a  ".to_string(), "b".to_string()]);
        assert_eq!(trimmed_rows(rows()), vec!["a".to_string(), String::new(), String::new(), "b".to_string()]);
    }
}
