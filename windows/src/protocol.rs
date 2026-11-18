//! Steuerkanal wie der Socket der Mac-App: eine Zeile JSON (`ControlRequest`) hin, eine Zeile zurück.
//!
//! Windows-Weg: TCP auf 127.0.0.1 mit zufälligem Port statt AF_UNIX. Der Port und ein zufälliges Token liegen in
//! der Endpunktdatei im Profilordner (`%APPDATA%`, nur der eigene Benutzer kann lesen). Ohne passendes Token
//! antwortet die App nicht. `KADRELL_SOCKET` zeigt in jeder Kachel auf diese Datei, wie auf dem Mac auf den Socket.
//! Die Felder `argv`, `cwd`, `caller` und `status`, `stdout`, `stderr` sind dieselben wie auf dem Mac.

use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::Path;
use std::time::Duration;

pub const MAX_REQUEST: usize = 1 << 20;
/// Antwort, solange die App noch startet; der Client wartet dann weiter.
pub const STARTING_STATUS: i32 = 75;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ControlRequest {
    pub argv: Vec<String>,
    /// Arbeitsordner des Aufrufers, für relative Pfade.
    pub cwd: String,
    /// `KADRELL_SESSION_KEY` des Aufrufers, wenn er in einer Kachel läuft (wie `$TMUX_PANE`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caller: Option<String>,
    /// Nur Windows: Token aus der Endpunktdatei.
    #[serde(default)]
    pub token: String,
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
    pub fn ok(out: impl Into<String>) -> Self {
        let mut stdout = out.into();
        if !stdout.is_empty() && !stdout.ends_with('\n') {
            stdout.push('\n');
        }
        ControlResponse { status: 0, stdout, stderr: String::new() }
    }

    pub fn fail(message: &str) -> Self {
        ControlResponse { status: 1, stdout: String::new(), stderr: format!("kadrell: {message}\n") }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Endpoint {
    pub port: u16,
    pub token: String,
    pub pid: u32,
}

impl Endpoint {
    pub fn read(path: &Path) -> Option<Endpoint> {
        serde_json::from_slice(&std::fs::read(path).ok()?).ok()
    }

    pub fn write(&self, path: &Path) -> std::io::Result<()> {
        std::fs::write(path, serde_json::to_vec(self)?)
    }

    fn addr(&self) -> SocketAddr {
        SocketAddr::from(([127, 0, 0, 1], self.port))
    }
}

/// Liest eine Zeile, höchstens `MAX_REQUEST` Bytes.
pub fn read_line(stream: &TcpStream) -> Option<Vec<u8>> {
    let mut buf = Vec::new();
    let mut reader = BufReader::new(stream).take(MAX_REQUEST as u64 + 1);
    reader.read_until(b'\n', &mut buf).ok()?;
    (!buf.is_empty() && buf.len() <= MAX_REQUEST).then_some(buf)
}

pub fn write_line<T: Serialize>(mut stream: &TcpStream, value: &T) -> std::io::Result<()> {
    let mut data = serde_json::to_vec(value)?;
    data.push(b'\n');
    stream.write_all(&data)
}

/// Schickt die Anfrage. `Ok(None)`: niemand lauscht auf dem Endpunkt.
pub fn send(endpoint: &Endpoint, req: &ControlRequest, timeout: Duration) -> Result<Option<ControlResponse>, String> {
    let Ok(stream) = TcpStream::connect_timeout(&endpoint.addr(), Duration::from_secs(2)) else { return Ok(None) };
    stream.set_read_timeout(Some(timeout)).map_err(|e| e.to_string())?;
    let mut req = req.clone();
    req.token = endpoint.token.clone();
    write_line(&stream, &req).map_err(|e| e.to_string())?;
    let line = read_line(&stream).ok_or("keine Antwort von Kadrell")?;
    serde_json::from_slice(&line).map(Some).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_matches_mac_fields() {
        let req = ControlRequest { argv: vec!["ls".into()], cwd: "C:/x".into(), caller: None, token: "t".into() };
        let json = serde_json::to_string(&req).unwrap();
        assert_eq!(json, r#"{"argv":["ls"],"cwd":"C:/x","token":"t"}"#);
        // Eine Anfrage im Mac-Format (ohne token) bleibt lesbar.
        let mac: ControlRequest = serde_json::from_str(r#"{"argv":["ls"],"cwd":"/","caller":"k"}"#).unwrap();
        assert_eq!(mac.caller.as_deref(), Some("k"));
    }

    #[test]
    fn ok_appends_newline() {
        assert_eq!(ControlResponse::ok("a").stdout, "a\n");
        assert_eq!(ControlResponse::ok("").stdout, "");
        assert_eq!(ControlResponse::fail("x").stderr, "kadrell: x\n");
    }
}
