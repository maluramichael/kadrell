//! Das Protokoll des Steuerkanals: eine Zeile JSON (`ControlRequest`) hin, eine Zeile (`ControlResponse`) zurück.
//! Gleich auf Mac und Windows (`docs/cli-contract.md`); nur der Weg dorthin unterscheidet sich (Unix-Socket, TCP).

use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Read, Write};

/// Größte Anfrage oder Antwort in Bytes.
pub const MAX_REQUEST: usize = 1 << 20;
/// Antwort, solange die App noch startet: der Client wartet dann weiter, statt es als Fehler zu behandeln.
pub const STARTING_STATUS: i32 = 75;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ControlRequest {
    pub argv: Vec<String>,
    /// Arbeitsordner des Aufrufers, für relative Pfade.
    pub cwd: String,
    /// `KADRELL_SESSION_KEY` des Aufrufers (wie `$TMUX_PANE`). Frei setzbar und deshalb nie vertraut: der Server
    /// ersetzt ihn durch die Kachel, aus deren Prozessbaum der Aufruf kommt (`caller::caller_tile`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caller: Option<String>,
    /// Nur Windows (TCP): Token aus der Endpunktdatei. Auf dem Mac fehlt das Feld.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub token: String,
}

impl ControlRequest {
    pub fn new(argv: Vec<String>, cwd: impl Into<String>, caller: Option<String>) -> ControlRequest {
        ControlRequest { argv, cwd: cwd.into(), caller, token: String::new() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ControlResponse {
    pub status: i32,
    #[serde(default)]
    pub stdout: String,
    #[serde(default)]
    pub stderr: String,
}

impl ControlResponse {
    /// Erfolg, die Ausgabe endet mit genau einem Zeilenumbruch (leer bleibt leer).
    pub fn ok(out: impl Into<String>) -> ControlResponse {
        let mut stdout = out.into();
        if !stdout.is_empty() && !stdout.ends_with('\n') {
            stdout.push('\n');
        }
        ControlResponse { status: 0, stdout, stderr: String::new() }
    }

    /// Fehler: Status 1, `kadrell: <meldung>` auf stderr.
    pub fn fail(message: &str) -> ControlResponse {
        ControlResponse { status: 1, stdout: String::new(), stderr: format!("kadrell: {message}\n") }
    }

    /// Antwort, solange die App noch bootet.
    pub fn starting() -> ControlResponse {
        ControlResponse { status: STARTING_STATUS, stdout: String::new(), stderr: "Kadrell startet noch\n".into() }
    }
}

/// Liest bis zum Zeilenende (oder EOF), höchstens [`MAX_REQUEST`] Bytes. `None` bei leerer oder zu großer Eingabe.
pub fn read_line(reader: impl Read) -> Option<Vec<u8>> {
    let mut buf = Vec::new();
    BufReader::new(reader.take(MAX_REQUEST as u64 + 1)).read_until(b'\n', &mut buf).ok()?;
    (!buf.is_empty() && buf.len() <= MAX_REQUEST).then_some(buf)
}

/// Schreibt den Wert als eine JSON-Zeile.
pub fn write_line<T: Serialize>(mut writer: impl Write, value: &T) -> std::io::Result<()> {
    let mut data = serde_json::to_vec(value)?;
    data.push(b'\n');
    writer.write_all(&data)?;
    writer.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_matches_mac_fields() {
        let req = ControlRequest::new(vec!["ls".into()], "/x", None);
        assert_eq!(serde_json::to_string(&req).unwrap(), r#"{"argv":["ls"],"cwd":"/x"}"#);
        let with = ControlRequest { caller: Some("k".into()), token: "t".into(), ..req };
        assert_eq!(serde_json::to_string(&with).unwrap(), r#"{"argv":["ls"],"cwd":"/x","caller":"k","token":"t"}"#);
        let mac: ControlRequest = serde_json::from_str(r#"{"argv":["ls"],"cwd":"/","caller":"k"}"#).unwrap();
        assert_eq!(mac.caller.as_deref(), Some("k"));
        assert_eq!(mac.token, "");
    }

    #[test]
    fn response_helpers() {
        assert_eq!(ControlResponse::ok("a").stdout, "a\n");
        assert_eq!(ControlResponse::ok("a\n").stdout, "a\n");
        assert_eq!(ControlResponse::ok("").stdout, "");
        let fail = ControlResponse::fail("x");
        assert_eq!((fail.status, fail.stderr.as_str()), (1, "kadrell: x\n"));
        assert_eq!(ControlResponse::starting().status, STARTING_STATUS);
        assert_eq!(serde_json::to_string(&ControlResponse::ok("hallo")).unwrap(), r#"{"status":0,"stdout":"hallo\n","stderr":""}"#);
    }

    #[test]
    fn read_line_limits() {
        assert_eq!(read_line(&b"{\"a\":1}\nrest"[..]).unwrap(), b"{\"a\":1}\n");
        assert_eq!(read_line(&b"ohne ende"[..]).unwrap(), b"ohne ende");
        assert!(read_line(&b""[..]).is_none());
        let big = vec![b'x'; MAX_REQUEST + 10];
        assert!(read_line(&big[..]).is_none());
    }
}
