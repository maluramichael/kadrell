//! Prozesse beenden: Signal an die Prozessgruppe (Unix) bzw. den Baum (Windows), Warten auf das Ende mit Frist, danach
//! hart. Port von `AttachManager.signal`/`shutdown` und `ProcessRunner.killChildren` der Mac-App.

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Wartezeit beim Beenden der App, bevor übrig gebliebene Prozesse hart beendet werden.
pub const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);
/// Abstand der Prüfungen beim Warten.
const POLL: Duration = Duration::from_millis(100);

/// Alle Nachkommen von `pid` aus Zeilen `(pid, ppid)`, Enkel vor ihren Eltern (so beendet man von unten nach oben).
pub fn descendants(rows: &[(u32, u32)], pid: u32) -> Vec<u32> {
    let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
    for &(p, parent) in rows {
        children.entry(parent).or_default().push(p);
    }
    let mut out = Vec::new();
    collect(&children, pid, &mut out, 0);
    out
}

fn collect(children: &HashMap<u32, Vec<u32>>, pid: u32, out: &mut Vec<u32>, depth: usize) {
    if depth > 64 {
        return; // Schleife in kaputten Daten
    }
    for &child in children.get(&pid).into_iter().flatten() {
        collect(children, child, out, depth + 1);
        out.push(child);
    }
}

/// Lebt der Prozess noch? Unter Unix per Signal 0, unter Windows per `tasklist`.
pub fn is_alive(pid: u32) -> bool {
    imp::is_alive(pid)
}

/// Erstes Signal beim Beenden: SIGHUP an die Gruppe (Unix), `taskkill /T` an den Baum (Windows). Das Programm bekommt
/// so Gelegenheit, sich zu beenden und seine Kinder mitzunehmen.
pub fn hang_up(pid: u32) {
    imp::hang_up(pid);
}

/// Letzte Stufe: SIGKILL an Gruppe und Nachkommen (Unix), `taskkill /F /T` (Windows).
pub fn kill_hard(pid: u32) {
    imp::kill_hard(pid);
}

/// SIGKILL an alle Nachkommen von `pid` (Hilfsprozesse, die in eigenen Gruppen laufen), `pid` selbst bleibt.
pub fn kill_children(pid: u32) {
    imp::kill_children(pid);
}

/// Beenden der App: alle bekommen `hang_up`, dann wird bis `timeout` gewartet (Abfrage alle 100 ms), wer dann noch
/// lebt, bekommt `kill_hard`. Gibt die pids derer zurück, die hart beendet werden mussten.
pub fn terminate_all(pids: &[u32], timeout: Duration) -> Vec<u32> {
    // Gleichzeitig: unter Windows ist jedes Beenden ein eigener `taskkill`-Prozess.
    std::thread::scope(|s| {
        for &pid in pids {
            s.spawn(move || hang_up(pid));
        }
    });
    let deadline = Instant::now() + timeout;
    let mut alive = imp::alive_among(pids);
    while !alive.is_empty() && Instant::now() < deadline {
        std::thread::sleep(POLL);
        alive = imp::alive_among(&alive);
    }
    for &pid in &alive {
        eprintln!("kadrell: Prozess {pid} reagiert nicht auf das Beenden, wird hart beendet");
        kill_hard(pid);
    }
    alive
}

#[cfg(unix)]
mod imp {
    use super::descendants;

    fn signal_group(pid: u32, sig: libc::c_int) {
        let pid = pid as libc::pid_t;
        // SAFETY: `kill` mit einer pid und einem Signal hat keine weiteren Voraussetzungen.
        // `forkpty` macht den Prozess zum Gruppenleiter: das Signal geht an die ganze Gruppe, damit Kinder nicht verwaist
        // weiterlaufen. Gibt es die Gruppe nicht, nur an die pid.
        unsafe {
            if libc::kill(-pid, sig) != 0 {
                libc::kill(pid, sig);
            }
        }
    }

    pub fn hang_up(pid: u32) {
        signal_group(pid, libc::SIGHUP);
    }

    pub fn kill_hard(pid: u32) {
        kill_children(pid);
        signal_group(pid, libc::SIGKILL);
    }

    pub fn is_alive(pid: u32) -> bool {
        // SAFETY: Signal 0 prüft nur, ob es den Prozess gibt.
        unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
    }

    pub fn alive_among(pids: &[u32]) -> Vec<u32> {
        pids.iter().copied().filter(|p| is_alive(*p)).collect()
    }

    fn rows() -> Vec<(u32, u32)> {
        let Ok(out) = std::process::Command::new("ps").args(["-axo", "pid=,ppid="]).output() else { return Vec::new() };
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter_map(|l| {
                let mut f = l.split_whitespace();
                Some((f.next()?.parse().ok()?, f.next()?.parse().ok()?))
            })
            .collect()
    }

    pub fn kill_children(pid: u32) {
        for child in descendants(&rows(), pid) {
            // SAFETY: wie oben.
            unsafe { libc::kill(child as libc::pid_t, libc::SIGKILL) };
        }
    }
}

#[cfg(windows)]
mod imp {
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    fn taskkill(args: &[&str]) {
        let _ = Command::new("taskkill").args(args).creation_flags(CREATE_NO_WINDOW).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status();
    }

    /// Windows kennt kein SIGHUP: der Baum wird sofort beendet (wie bisher `TermHandle::shutdown`).
    pub fn hang_up(pid: u32) {
        taskkill(&["/F", "/T", "/PID", &pid.to_string()]);
    }

    pub fn kill_hard(pid: u32) {
        hang_up(pid);
    }

    pub fn kill_children(pid: u32) {
        hang_up(pid);
    }

    /// Wer von `pids` noch läuft, mit einer einzigen Abfrage der Prozessliste.
    pub fn alive_among(pids: &[u32]) -> Vec<u32> {
        if pids.is_empty() {
            return Vec::new();
        }
        let Ok(out) = Command::new("tasklist").args(["/FO", "CSV", "/NH"]).creation_flags(CREATE_NO_WINDOW).stdin(Stdio::null()).stderr(Stdio::null()).output() else {
            return Vec::new();
        };
        let list = String::from_utf8_lossy(&out.stdout);
        pids.iter().copied().filter(|pid| list.contains(&format!("\",\"{pid}\",\""))).collect()
    }

    pub fn is_alive(pid: u32) -> bool {
        !alive_among(&[pid]).is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descendants_lists_grandchildren_before_their_parents() {
        let rows = [(2, 1), (3, 2), (4, 2), (5, 3), (9, 8)];
        assert_eq!(descendants(&rows, 1), vec![5, 3, 4, 2]);
        assert_eq!(descendants(&rows, 9), Vec::<u32>::new());
        assert_eq!(descendants(&[(1, 2), (2, 1)], 1), descendants(&[(1, 2), (2, 1)], 1), "Schleife endet");
    }

    #[cfg(windows)]
    fn sleeper() -> std::process::Child {
        std::process::Command::new("ping").args(["-n", "60", "127.0.0.1"]).stdout(std::process::Stdio::null()).spawn().unwrap()
    }

    #[cfg(unix)]
    fn sleeper() -> std::process::Child {
        std::process::Command::new("sleep").arg("60").spawn().unwrap()
    }

    #[test]
    fn terminate_all_ends_a_running_process_within_the_timeout() {
        let mut child = sleeper();
        let pid = child.id();
        assert!(is_alive(pid));
        // Unter Unix bleibt ein beendetes Kind Zombie, bis es abgeholt ist: ein Thread holt es sofort ab.
        let reaper = std::thread::spawn(move || child.wait());
        let started = Instant::now();
        let hard = terminate_all(&[pid], Duration::from_secs(5));
        let _ = reaper.join();
        assert!(started.elapsed() < Duration::from_secs(5), "das Warten endet mit dem Prozess");
        assert!(hard.is_empty(), "SIGHUP genügt");
        assert!(!is_alive(pid));
    }

    #[test]
    fn nothing_to_end_returns_at_once() {
        let started = Instant::now();
        assert!(terminate_all(&[], SHUTDOWN_TIMEOUT).is_empty());
        assert!(started.elapsed() < Duration::from_secs(1));
    }
}
