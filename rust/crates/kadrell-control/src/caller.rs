//! Wer ruft an. `KADRELL_SESSION_KEY` kann jeder Prozess setzen, die pid am Kanal nicht: Der Server bestimmt die
//! aufrufende Kachel allein aus der pid des Clients und dem Prozessbaum und ersetzt das `caller`-Feld der Anfrage
//! damit (wie `ControlCaller` der Mac-App).
//!
//! Eine Kachel ist es, wenn die pid zur Terminal-Session der Kachel gehört (Unix: `forkpty` macht die Shell zum
//! Session-Leader) oder im Prozessbaum unter dem Kindprozess einer Kachel hängt.
//!
//! ponytail: Ein Prozess, der sich per `setsid` und Doppel-Fork von der Kachel löst, gilt als außerhalb; gleicher
//! Benutzer ist keine harte Grenze. Das ist Schadensbegrenzung gegen Prompt Injection (siehe `docs`).

/// Höchste Zahl Vorfahren, die der Läufer prüft.
const MAX_DEPTH: usize = 64;

/// Zugriff auf den Prozessbaum des Systems. Hinter einem Trait, damit Tests einen erfundenen Baum füttern.
pub trait ProcessTree {
    /// Eltern-pid, `None` wenn der Prozess nicht (mehr) existiert.
    fn parent(&self, pid: u32) -> Option<u32>;
    /// pid des Session-Leaders (Unix `getsid`), `None` wo es das nicht gibt (Windows) oder der Prozess fehlt.
    fn session_leader(&self, pid: u32) -> Option<u32>;
}

/// Der Prozessbaum des laufenden Systems (macOS, Linux, Windows).
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemProcessTree;

/// Key der Kachel, aus der `peer_pid` kommt. `tiles` sind Paare aus Key und pid des Kindprozesses der Kachel.
pub fn caller_tile(peer_pid: u32, tiles: &[(String, u32)]) -> Option<String> {
    caller_tile_in(&SystemProcessTree, peer_pid, tiles)
}

/// Wie [`caller_tile`] mit eigenem Prozessbaum: erst die Terminal-Session, dann bis zu 64 Vorfahren (pid 1 und
/// kleiner zählen nicht).
pub fn caller_tile_in(tree: &dyn ProcessTree, peer_pid: u32, tiles: &[(String, u32)]) -> Option<String> {
    let find = |pid: u32| tiles.iter().find(|(_, p)| *p == pid).map(|(key, _)| key.clone());
    if let Some(key) = tree.session_leader(peer_pid).and_then(find) {
        return Some(key);
    }
    let mut pid = peer_pid;
    for _ in 0..MAX_DEPTH {
        if pid <= 1 {
            break;
        }
        if let Some(key) = find(pid) {
            return Some(key);
        }
        pid = tree.parent(pid)?;
    }
    None
}

#[cfg(target_os = "linux")]
impl ProcessTree for SystemProcessTree {
    fn parent(&self, pid: u32) -> Option<u32> {
        linux_stat(pid).map(|(ppid, _)| ppid)
    }

    fn session_leader(&self, pid: u32) -> Option<u32> {
        linux_stat(pid).map(|(_, sid)| sid)
    }
}

/// `(ppid, session)` aus `/proc/<pid>/stat`. Der Name steht in Klammern und darf Leerzeichen und Klammern enthalten,
/// deshalb ab der letzten schließenden Klammer lesen: `state ppid pgrp session ...`.
#[cfg(target_os = "linux")]
fn linux_stat(pid: u32) -> Option<(u32, u32)> {
    let text = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    parse_linux_stat(&text)
}

#[cfg(any(target_os = "linux", test))]
fn parse_linux_stat(text: &str) -> Option<(u32, u32)> {
    let rest = &text[text.rfind(')')? + 1..];
    let mut fields = rest.split_whitespace().skip(1);
    let ppid = fields.next()?.parse().ok()?;
    let _pgrp = fields.next()?;
    let session = fields.next()?.parse().ok()?;
    Some((ppid, session))
}

#[cfg(target_os = "macos")]
impl ProcessTree for SystemProcessTree {
    fn parent(&self, pid: u32) -> Option<u32> {
        let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
        let size = std::mem::size_of::<libc::proc_bsdinfo>() as libc::c_int;
        // proc_pidinfo liefert die Größe der gefüllten Struktur, 0 oder weniger bei einem Fehler (Prozess weg).
        let got = unsafe { libc::proc_pidinfo(pid as libc::c_int, libc::PROC_PIDTBSDINFO, 0, std::ptr::addr_of_mut!(info).cast(), size) };
        (got == size).then_some(info.pbi_ppid)
    }

    fn session_leader(&self, pid: u32) -> Option<u32> {
        let sid = unsafe { libc::getsid(pid as libc::pid_t) };
        u32::try_from(sid).ok().filter(|s| *s > 0)
    }
}

#[cfg(windows)]
impl ProcessTree for SystemProcessTree {
    fn parent(&self, pid: u32) -> Option<u32> {
        windows_parents().get(&pid).copied()
    }

    fn session_leader(&self, _pid: u32) -> Option<u32> {
        None
    }
}

/// Alle Prozesse als pid zu Eltern-pid (Toolhelp32-Schnappschuss). Ein Schnappschuss je Abfrage genügt: die Abfrage
/// läuft einmal je Aufruf von `kadrell` und nicht in einer heißen Schleife.
#[cfg(windows)]
pub fn windows_parents() -> std::collections::HashMap<u32, u32> {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS};

    let mut parents = std::collections::HashMap::new();
    // SAFETY: der Schnappschuss-Handle gehört diesem Aufruf und wird am Ende geschlossen; `entry` ist initialisiert
    // und `dwSize` gesetzt, wie die API es verlangt.
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return parents;
        }
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut more = Process32FirstW(snapshot, &mut entry) != 0;
        while more {
            parents.insert(entry.th32ProcessID, entry.th32ParentProcessID);
            more = Process32NextW(snapshot, &mut entry) != 0;
        }
        CloseHandle(snapshot);
    }
    parents
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
impl ProcessTree for SystemProcessTree {
    fn parent(&self, _pid: u32) -> Option<u32> {
        None
    }

    fn session_leader(&self, _pid: u32) -> Option<u32> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// Erfundener Baum: Eltern und Session-Leader je pid.
    struct Fake {
        parents: HashMap<u32, u32>,
        sessions: HashMap<u32, u32>,
    }

    impl ProcessTree for Fake {
        fn parent(&self, pid: u32) -> Option<u32> {
            self.parents.get(&pid).copied()
        }
        fn session_leader(&self, pid: u32) -> Option<u32> {
            self.sessions.get(&pid).copied()
        }
    }

    fn fake(parents: &[(u32, u32)], sessions: &[(u32, u32)]) -> Fake {
        Fake { parents: parents.iter().copied().collect(), sessions: sessions.iter().copied().collect() }
    }

    fn tiles() -> Vec<(String, u32)> {
        vec![("k1".into(), 100), ("k2".into(), 200)]
    }

    /// Die Kachel ergibt sich aus der pid, nicht aus dem mitgeschickten Key: Terminal-Session oder Vorfahr.
    #[test]
    fn caller_from_process_tree() {
        let tree = fake(&[(300, 250), (250, 200), (400, 1), (500, 450)], &[]);
        assert_eq!(caller_tile_in(&tree, 300, &tiles()).as_deref(), Some("k2"));
        assert_eq!(caller_tile_in(&tree, 200, &tiles()).as_deref(), Some("k2"), "die Kachel selbst");
        assert_eq!(caller_tile_in(&tree, 400, &tiles()), None);
        assert_eq!(caller_tile_in(&tree, 500, &tiles()), None, "Elternprozess existiert nicht mehr");
        assert_eq!(caller_tile_in(&tree, 999, &tiles()), None, "unbekannte pid");
    }

    /// Doppelt geforkt und an launchd gehängt: die Terminal-Session bleibt.
    #[test]
    fn caller_from_terminal_session() {
        let tree = fake(&[(400, 1)], &[(400, 100)]);
        assert_eq!(caller_tile_in(&tree, 400, &tiles()).as_deref(), Some("k1"));
        let no_session = fake(&[(400, 1)], &[(400, 77)]);
        assert_eq!(caller_tile_in(&no_session, 400, &tiles()), None);
    }

    #[test]
    fn walk_is_bounded_and_stops_at_init() {
        // Zyklus (kaputte Daten) endet nach 64 Schritten statt ewig zu laufen.
        let cycle = fake(&[(10, 11), (11, 10)], &[]);
        assert_eq!(caller_tile_in(&cycle, 10, &tiles()), None);
        // Eine Kachel mit pid 1 zählt nie, auch nicht als Vorfahr.
        let tree = fake(&[(50, 1)], &[]);
        assert_eq!(caller_tile_in(&tree, 50, &[("init".into(), 1)]), None);
        // Tief, aber innerhalb der Grenze.
        let chain: Vec<(u32, u32)> = (1000..1060).map(|p| (p, p + 1)).collect();
        let deep = fake(&chain, &[]);
        assert_eq!(caller_tile_in(&deep, 1000, &[("tief".into(), 1060)]).as_deref(), Some("tief"));
    }

    #[test]
    fn linux_stat_parsing() {
        assert_eq!(parse_linux_stat("1234 (bash) S 1000 1234 1234 34816 1234 4194304"), Some((1000, 1234)));
        assert_eq!(parse_linux_stat("77 (a b) c) R 5 6 7 8"), Some((5, 7)), "Klammern im Namen");
        assert_eq!(parse_linux_stat("kaputt"), None);
    }

    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    #[test]
    fn system_tree_knows_this_process() {
        let me = std::process::id();
        assert!(SystemProcessTree.parent(me).is_some_and(|p| p != me));
        assert_eq!(SystemProcessTree.parent(u32::MAX - 1), None);
    }

    /// Ein echter Kindprozess hängt im Prozessbaum unter diesem Test.
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    #[test]
    fn system_tree_finds_real_child() {
        let mut child = if cfg!(windows) {
            std::process::Command::new("ping").args(["-n", "4", "127.0.0.1"]).stdout(std::process::Stdio::null()).spawn().unwrap()
        } else {
            std::process::Command::new("sleep").arg("3").spawn().unwrap()
        };
        let me = std::process::id();
        let found = caller_tile(child.id(), &[("test".into(), me)]);
        let _ = child.kill();
        let _ = child.wait();
        assert_eq!(found.as_deref(), Some("test"));
    }
}
