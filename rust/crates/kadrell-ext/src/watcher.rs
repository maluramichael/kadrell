//! Meldet Änderungen unter einem Ordner, gebündelt nach `latency`. Der Ordner darf fehlen und später entstehen, und er darf
//! verschwinden und wieder auftauchen: nach jeder Meldung wird der Beobachter neu aufgesetzt, und eine langsame
//! Existenzprüfung fängt Fälle ab, in denen das System nichts meldet.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use notify::{RecommendedWatcher, RecursiveMode, Watcher};

const POLL: Duration = Duration::from_millis(500);

pub struct FolderWatcher {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl FolderWatcher {
    /// Beginnt sofort zu beobachten. `on_change` läuft auf einem eigenen Thread.
    pub fn new(
        path: PathBuf,
        latency: Duration,
        on_change: Arc<dyn Fn() + Send + Sync>,
    ) -> FolderWatcher {
        // Vor dem Thread festhalten: ein Ordner, der gleich danach verschwindet, soll als Änderung gelten.
        let existed = readable(&path);
        let stop = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stop);
        let thread = std::thread::Builder::new()
            .name("ext-watcher".into())
            .spawn(move || run(&path, existed, latency, &flag, &*on_change))
            .ok();
        FolderWatcher { stop, thread }
    }

    /// Beendet die Beobachtung; es kommt danach keine Meldung mehr.
    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for FolderWatcher {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

/// Ordner, der sich lesen lässt. Unter Windows besteht ein gelöschter Ordner, den noch ein Handle (auch unser eigener
/// Beobachter) offen hält, bis zu dessen Schließen weiter und `is_dir` sagt weiter ja; lesen lässt er sich nicht mehr.
fn readable(path: &Path) -> bool {
    std::fs::read_dir(path).is_ok()
}

fn install(path: &Path, tx: &Sender<()>) -> Option<RecommendedWatcher> {
    if !readable(path) {
        return None;
    }
    let tx = tx.clone();
    let mut watcher = notify::recommended_watcher(move |_res: notify::Result<notify::Event>| {
        let _ = tx.send(());
    })
    .ok()?;
    watcher.watch(path, RecursiveMode::Recursive).ok()?;
    Some(watcher)
}

fn run(
    path: &Path,
    existed: bool,
    latency: Duration,
    stop: &AtomicBool,
    on_change: &(dyn Fn() + Send + Sync),
) {
    let (tx, rx) = mpsc::channel::<()>();
    let mut watcher = install(path, &tx);
    let mut existed = existed;
    let mut fire_at: Option<Instant> = None;
    while !stop.load(Ordering::SeqCst) {
        let wait = fire_at.map_or(POLL, |t| {
            t.saturating_duration_since(Instant::now()).min(POLL)
        });
        match rx.recv_timeout(wait) {
            Ok(()) => {
                fire_at.get_or_insert(Instant::now() + latency);
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        // Existenzprüfung: Anlegen und Löschen des Ordners selbst.
        let exists = readable(path);
        if exists && watcher.is_none() {
            // Beobachter fehlt (Ordner war weg oder der Start schlug fehl): still nachholen, die Meldung kommt über `existed`.
            watcher = install(path, &tx);
        }
        if exists != existed {
            existed = exists;
            fire_at.get_or_insert(Instant::now() + latency);
        }
        if fire_at.is_some_and(|t| Instant::now() >= t) {
            fire_at = None;
            // Neu aufsetzen: ein Beobachter auf einen damals fehlenden oder inzwischen ersetzten Ordner sieht nichts.
            drop(watcher.take());
            watcher = install(path, &tx);
            while rx.try_recv().is_ok() {}
            if !stop.load(Ordering::SeqCst) {
                on_change();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{TempDir, wait_until};
    use std::sync::atomic::AtomicUsize;

    fn counter() -> (Arc<AtomicUsize>, Arc<dyn Fn() + Send + Sync>) {
        let n = Arc::new(AtomicUsize::new(0));
        let c = Arc::clone(&n);
        (
            n,
            Arc::new(move || {
                c.fetch_add(1, Ordering::SeqCst);
            }),
        )
    }

    #[test]
    fn reports_file_changes_and_late_creation() {
        let root = TempDir::new("watch");
        let dir = root.path().join("catalog");
        let (n, cb) = counter();
        let mut w = FolderWatcher::new(dir.clone(), Duration::from_millis(100), cb);
        std::fs::create_dir_all(dir.join("a")).unwrap();
        assert!(
            wait_until(3.0, || n.load(Ordering::SeqCst) >= 1),
            "Anlegen des Ordners nicht gesehen"
        );
        let before = n.load(Ordering::SeqCst);
        std::fs::write(dir.join("a/init.lua"), "x").unwrap();
        assert!(
            wait_until(3.0, || n.load(Ordering::SeqCst) > before),
            "neue Datei nicht gesehen"
        );
        let before = n.load(Ordering::SeqCst);
        std::fs::write(dir.join("a/init.lua"), "y").unwrap();
        assert!(
            wait_until(3.0, || n.load(Ordering::SeqCst) > before),
            "Schreiben in bestehende Datei nicht gesehen"
        );
        w.stop();
        let after = n.load(Ordering::SeqCst);
        std::fs::write(dir.join("a/init.lua"), "z").unwrap();
        std::thread::sleep(Duration::from_millis(400));
        assert_eq!(
            n.load(Ordering::SeqCst),
            after,
            "nach stop() keine Meldung mehr"
        );
    }

    #[test]
    fn survives_delete_and_recreate() {
        let root = TempDir::new("watch");
        let dir = root.path().join("catalog");
        std::fs::create_dir_all(&dir).unwrap();
        let (n, cb) = counter();
        let _w = FolderWatcher::new(dir.clone(), Duration::from_millis(100), cb);
        for round in 1..=2 {
            let before = n.load(Ordering::SeqCst);
            std::fs::remove_dir_all(&dir).unwrap();
            assert!(
                wait_until(3.0, || n.load(Ordering::SeqCst) > before),
                "Runde {round}: Löschen nicht gesehen"
            );
            let before = n.load(Ordering::SeqCst);
            std::fs::create_dir_all(dir.join("a")).unwrap();
            assert!(
                wait_until(3.0, || n.load(Ordering::SeqCst) > before),
                "Runde {round}: Neuanlage nicht gesehen"
            );
            let before = n.load(Ordering::SeqCst);
            std::fs::write(dir.join("a/init.lua"), "x").unwrap();
            assert!(
                wait_until(3.0, || n.load(Ordering::SeqCst) > before),
                "Runde {round}: Datei im neuen Ordner nicht gesehen"
            );
        }
    }
}
