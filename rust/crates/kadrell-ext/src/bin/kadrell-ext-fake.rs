//! Nur für Tests: ein Programm, das sich wie ein kaputter Extension-Helper oder wie ein einfaches Kommandozeilenwerkzeug
//! verhält. Ersetzt die Shell-Skripte der Swift-Tests, die es unter Windows nicht gibt.
//!
//! Als Werkzeug für `kadrell.exec`: `echo <text…>`, `exit <n>`, `stderr-exit <n> <text>`, `sleep <sekunden>`.
//! Als Helper (`ext-host <ordner>`, Verhalten aus der Umgebungsvariablen `FAKE_MODE`): `flood`, `garbage`, `stderr`,
//! `noready`, `longline`, `idle` (ready, liest nie stdin), `echo-hello` (liest stdin und schreibt jede Zeile nach stderr).

use std::io::{BufRead, Write};
use std::time::Duration;

fn sleep_forever() -> ! {
    loop {
        std::thread::sleep(Duration::from_secs(30));
    }
}

fn line(s: &str) {
    let mut out = std::io::stdout().lock();
    let _ = out.write_all(s.as_bytes());
    let _ = out.write_all(b"\n");
    let _ = out.flush();
}

fn helper(mode: &str) -> ! {
    match mode {
        "flood" => {
            line(r#"{"t":"ready"}"#);
            for _ in 0..1000 {
                line(r#"{"t":"status","item":null}"#);
            }
        }
        "garbage" => {
            line("kaputt");
            line(r#"{"t":"ready"}"#);
        }
        "stderr" => {
            eprintln!("eins");
            eprintln!("{}", "b".repeat(2000));
            line(r#"{"t":"ready"}"#);
        }
        "longline" => {
            line(r#"{"t":"ready"}"#);
            let mut out = std::io::stdout().lock();
            let _ = out.write_all("a".repeat(1_100_000).as_bytes());
            let _ = out.flush();
        }
        "echo-hello" => {
            line(r#"{"t":"ready"}"#);
            for l in std::io::stdin().lock().lines().map_while(Result::ok) {
                eprintln!("{l}");
            }
            std::process::exit(0);
        }
        "noready" => {}
        _ => line(r#"{"t":"ready"}"#),
    }
    sleep_forever()
}

fn tool(args: &[String]) -> i32 {
    match args.first().map(String::as_str) {
        Some("echo") => {
            line(&args[1..].join(" "));
            0
        }
        Some("exit") => args.get(1).and_then(|n| n.parse().ok()).unwrap_or(0),
        Some("stderr-exit") => {
            eprintln!("{}", args.get(2).cloned().unwrap_or_default());
            args.get(1).and_then(|n| n.parse().ok()).unwrap_or(0)
        }
        Some("sleep") => {
            std::thread::sleep(Duration::from_secs_f64(
                args.get(1).and_then(|n| n.parse().ok()).unwrap_or(1.0),
            ));
            0
        }
        _ => 2,
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|a| a == "ext-host") {
        helper(&std::env::var("FAKE_MODE").unwrap_or_default());
    }
    std::process::exit(tool(&args));
}
