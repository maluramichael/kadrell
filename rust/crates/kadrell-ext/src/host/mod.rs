//! `kadrell ext-host <ordner>`: der Helper-Prozess einer Extension. Eine Lua-Instanz auf einem eigenen Thread mit großem
//! Stack, Nachrichten als JSON-Zeilen über stdin/stdout. Lua sieht nur `__kadrell_send` und `__kadrell_dispatch`.
//!
//! Ablauf: ein Lese-Thread nimmt Zeilen von stdin, Arbeits-Threads (exec, http, system, Timer) liefern ihre Ergebnisse als
//! `result`-/`timer`-Zeilen in dieselbe Warteschlange, der Lua-Thread arbeitet sie der Reihe nach ab. Bei EOF auf stdin
//! endet der Prozess sofort, auch wenn Lua gerade in einer Endlosschleife hängt.

mod exec;
mod lua;
mod router;

use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::ExitCode;
use std::sync::mpsc;

use crate::sys;

/// Stack des Lua-Threads. Lua begrenzt C-Rekursion auf 200 Ebenen und rechnet mit normalem Stack; die 1 MB des
/// Windows-Hauptthreads reichen dafür nicht verlässlich.
const LUA_STACK: usize = 64 << 20;

/// Einstieg des Unterbefehls `ext-host`: läuft, bis stdin schließt, Lua `os.exit` ruft oder der Ladevorgang scheitert.
pub fn run_host(_dir: &Path) -> ExitCode {
    let out = match sys::take_protocol_stdout() {
        Ok(f) => f,
        Err(e) => return fail(&format!("kein Protokollkanal: {e}")),
    };
    // Jedes Ende (EOF auf stdin, shutdown, os.exit, Ladefehler, Kill von außen) nimmt laufende exec-Kinder mit.
    sys::die_with_children();
    let (tx, rx) = mpsc::channel::<String>();
    let router = router::Router::new(out, tx.clone());
    let spawned = std::thread::Builder::new().name("lua".into()).stack_size(LUA_STACK).spawn(move || lua::run(router, rx));
    if let Err(e) = spawned {
        return fail(&format!("Lua-Thread startet nicht: {e}"));
    }
    read_stdin(&tx);
    sys::kill_exec_children();
    ExitCode::SUCCESS
}

/// Liest Zeilen bis EOF. Binärmüll wird ersetzt, leere Zeilen übersprungen.
fn read_stdin(tx: &mpsc::Sender<String>) {
    let mut input = BufReader::new(std::io::stdin().lock());
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match input.read_until(b'\n', &mut buf) {
            Ok(0) | Err(_) => return,
            Ok(_) => {}
        }
        let line = String::from_utf8_lossy(&buf);
        let line = line.trim_end_matches(['\n', '\r']);
        if !line.is_empty() && tx.send(line.to_string()).is_err() {
            return;
        }
    }
}

pub(crate) fn fail(message: &str) -> ExitCode {
    eprintln!("ext-host: {message}");
    ExitCode::FAILURE
}
