//! Ein Terminal je Session: Kindprozess in einer ConPTY (portable-pty), Ausgabe läuft durch den VT-Parser
//! (vt100), der den Bildschirm für Anzeige und `capture` hält. Wie SwiftTerm auf dem Mac beantwortet der Parser
//! die Rückfragen der Programme nach Cursorposition und Gerät (DSR, DA), sonst warten manche Programme darauf.

use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

pub const SCROLLBACK: usize = 10_000;

/// Sammelt Antworten auf Terminal-Anfragen, die der Lese-Thread danach an das Programm zurückschreibt.
#[derive(Default)]
pub struct Responder {
    pub replies: Vec<u8>,
}

impl vt100::Callbacks for Responder {
    fn unhandled_csi(&mut self, screen: &mut vt100::Screen, i1: Option<u8>, _i2: Option<u8>, params: &[&[u16]], c: char) {
        let first = params.first().and_then(|p| p.first()).copied().unwrap_or(0);
        match (i1, c, first) {
            (None, 'n', 6) => {
                let (row, col) = screen.cursor_position();
                self.replies.extend(format!("\u{1b}[{};{}R", row + 1, col + 1).bytes());
            }
            (None, 'n', 5) => self.replies.extend(b"\x1b[0n"),
            (None, 'c', 0) => self.replies.extend(b"\x1b[?1;2c"),
            _ => {}
        }
    }
}

pub type Parser = vt100::Parser<Responder>;

pub struct Spawn {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: String,
    pub env: Vec<(String, String)>,
    pub remove_env: Vec<String>,
    pub rows: u16,
    pub cols: u16,
}

pub struct Pane {
    pub parser: Arc<Mutex<Parser>>,
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    master: Box<dyn MasterPty + Send>,
    child: Box<dyn Child + Send + Sync>,
    size: (u16, u16),
}

impl Pane {
    /// Startet den Prozess. `dirty` wird bei jeder Ausgabe gesetzt, damit die Oberfläche neu zeichnet.
    pub fn spawn(spec: Spawn, dirty: Arc<AtomicBool>) -> std::io::Result<Pane> {
        let size = PtySize { rows: spec.rows.max(2), cols: spec.cols.max(10), pixel_width: 0, pixel_height: 0 };
        let pair = native_pty_system().openpty(size).map_err(io_err)?;
        let mut cmd = CommandBuilder::new(&spec.program);
        cmd.args(&spec.args);
        cmd.cwd(&spec.cwd);
        for key in &spec.remove_env {
            cmd.env_remove(key);
        }
        for (k, v) in &spec.env {
            cmd.env(k, v);
        }
        let child = pair.slave.spawn_command(cmd).map_err(io_err)?;
        drop(pair.slave);
        let reader = pair.master.try_clone_reader().map_err(io_err)?;
        let writer: Arc<Mutex<Box<dyn Write + Send>>> = Arc::new(Mutex::new(pair.master.take_writer().map_err(io_err)?));
        let parser = Arc::new(Mutex::new(vt100::Parser::new_with_callbacks(size.rows, size.cols, SCROLLBACK, Responder::default())));
        read_loop(reader, parser.clone(), writer.clone(), dirty);
        Ok(Pane { parser, writer, master: pair.master, child, size: (size.rows, size.cols) })
    }

    pub fn write(&self, data: &[u8]) {
        if let Ok(mut w) = self.writer.lock() {
            let _ = w.write_all(data);
            let _ = w.flush();
        }
    }

    pub fn resize(&mut self, rows: u16, cols: u16) {
        let (rows, cols) = (rows.max(2), cols.max(10));
        if self.size == (rows, cols) {
            return;
        }
        self.size = (rows, cols);
        let _ = self.master.resize(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 });
        if let Ok(mut p) = self.parser.lock() {
            p.screen_mut().set_size(rows, cols);
        }
    }

    /// `Some(code)`, sobald der Prozess beendet ist. Unter Windows meldet die ConPTY kein EOF, daher dieser Weg.
    pub fn exited(&mut self) -> Option<u32> {
        self.child.try_wait().ok().flatten().map(|s| s.exit_code())
    }

    pub fn kill(&mut self) {
        let _ = self.child.kill();
    }

    pub fn bracketed_paste(&self) -> bool {
        self.parser.lock().map(|p| p.screen().bracketed_paste()).unwrap_or(false)
    }

    pub fn application_cursor(&self) -> bool {
        self.parser.lock().map(|p| p.screen().application_cursor()).unwrap_or(false)
    }

    /// Sichtbarer Bildschirm ohne Leerzeichen am Zeilenende und ohne leere Zeilen am Schluss; mit `all` davor der Verlauf.
    pub fn capture(&self, all: bool) -> String {
        let Ok(mut p) = self.parser.lock() else { return String::new() };
        let screen = p.screen_mut();
        let cols = screen.size().1;
        let mut rows = Vec::new();
        if all {
            screen.set_scrollback(usize::MAX);
            let total = screen.scrollback();
            for offset in (1..=total).rev() {
                screen.set_scrollback(offset);
                rows.extend(screen.rows(0, cols).next());
            }
            screen.set_scrollback(0);
        }
        rows.extend(screen.rows(0, cols));
        trim_rows(rows)
    }
}

pub fn trim_rows(rows: Vec<String>) -> String {
    let mut rows: Vec<String> = rows.into_iter().map(|r| r.trim_end().to_string()).collect();
    while rows.last().is_some_and(String::is_empty) {
        rows.pop();
    }
    rows.join("\n")
}

fn read_loop(mut reader: Box<dyn Read + Send>, parser: Arc<Mutex<Parser>>, writer: Arc<Mutex<Box<dyn Write + Send>>>, dirty: Arc<AtomicBool>) {
    std::thread::spawn(move || {
        let mut buf = [0u8; 65536];
        loop {
            let n = match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => n,
            };
            let replies = match parser.lock() {
                Ok(mut p) => {
                    p.process(&buf[..n]);
                    std::mem::take(&mut p.callbacks_mut().replies)
                }
                Err(_) => break,
            };
            if !replies.is_empty() {
                if let Ok(mut w) = writer.lock() {
                    let _ = w.write_all(&replies);
                    let _ = w.flush();
                }
            }
            dirty.store(true, Ordering::Release);
        }
        dirty.store(true, Ordering::Release);
    });
}

/// portable-pty liefert `anyhow::Error`; ohne eigene anyhow-Abhängigkeit reicht der Text.
fn io_err(e: impl std::fmt::Display) -> std::io::Error {
    std::io::Error::other(e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answers_cursor_and_device_queries() {
        let mut p: Parser = vt100::Parser::new_with_callbacks(5, 20, 0, Responder::default());
        p.process(b"ab\x1b[6n\x1b[c");
        assert_eq!(p.callbacks().replies, b"\x1b[1;3R\x1b[?1;2c");
    }

    #[test]
    fn trims_like_mac_capture() {
        assert_eq!(trim_rows(vec!["a  ".into(), "".into(), "b".into(), " ".into(), "".into()]), "a\n\nb");
    }

    #[test]
    fn spawns_real_process() {
        let dirty = Arc::new(AtomicBool::new(false));
        let spec = Spawn {
            program: "cmd.exe".into(),
            args: vec!["/c".into(), "echo kadrell-pty-ok".into()],
            cwd: std::env::temp_dir().to_string_lossy().into(),
            env: vec![],
            remove_env: vec![],
            rows: 10,
            cols: 60,
        };
        let mut pane = Pane::spawn(spec, dirty).expect("spawn");
        let start = std::time::Instant::now();
        while !pane.capture(false).contains("kadrell-pty-ok") && start.elapsed().as_secs() < 10 {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert!(pane.capture(false).contains("kadrell-pty-ok"), "{}", pane.capture(false));
        while pane.exited().is_none() && start.elapsed().as_secs() < 10 {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert_eq!(pane.exited(), Some(0));
    }
}
