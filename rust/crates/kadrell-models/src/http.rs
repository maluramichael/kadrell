//! HTTP hinter einem Trait (Swift: `LocalHTTP` über `URLSession`). Tests laufen mit einer Attrappe, nie gegen ein
//! echtes Netz. Anders als der Client in `kadrell-accounts` kennt dieser DELETE mit Body (Ollama `/api/delete`) und
//! das zeilenweise Lesen eines Streams (Ollama `/api/pull`).

use std::io::{BufRead, BufReader, Read};
use std::time::Duration;

use kadrell_claude::local_model::ModelError;
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
    Delete,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    pub method: Method,
    pub url: String,
    pub body: Option<Vec<u8>>,
    pub timeout: Duration,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Response {
    pub status: u16,
    pub body: Vec<u8>,
}

/// Der Server ist nicht erreichbar oder antwortet nicht in der Zeit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TransportError {
    Unreachable(String),
    Timeout,
}

pub trait Http: Send + Sync {
    /// Schickt die Anfrage ab. Jeder Status (auch 4xx/5xx) kommt als `Ok` zurück.
    fn send(&self, req: &Request) -> Result<Response, TransportError>;

    /// Wie `send`, aber bei 2xx liest es den Body zeilenweise und ruft `on_line` je Zeile auf (`false` beendet das
    /// Lesen). Bei einem anderen Status steht der ganze Body in der Antwort und `on_line` wird nie gerufen.
    fn stream_lines(&self, req: &Request, on_line: &mut dyn FnMut(&str) -> bool) -> Result<Response, TransportError>;
}

/// Echter Client auf Basis von `ureq` (blockierend).
#[derive(Clone, Copy, Debug, Default)]
pub struct UreqHttp;

impl UreqHttp {
    fn agent(req: &Request) -> ureq::Agent {
        ureq::Agent::config_builder().timeout_global(Some(req.timeout)).http_status_as_error(false).build().into()
    }

    fn run(req: &Request) -> Result<ureq::http::Response<ureq::Body>, TransportError> {
        let agent = Self::agent(req);
        let body = req.body.clone().unwrap_or_default();
        let result = match req.method {
            Method::Get => agent.get(&req.url).call(),
            Method::Post => agent.post(&req.url).header("Content-Type", "application/json").send(body),
            Method::Delete if req.body.is_some() => {
                agent.delete(&req.url).force_send_body().header("Content-Type", "application/json").send(body)
            }
            Method::Delete => agent.delete(&req.url).call(),
        };
        result.map_err(transport)
    }
}

fn transport(e: ureq::Error) -> TransportError {
    match e {
        ureq::Error::Timeout(_) => TransportError::Timeout,
        other => TransportError::Unreachable(other.to_string()),
    }
}

fn read_error(e: std::io::Error) -> TransportError {
    TransportError::Unreachable(e.to_string())
}

impl Http for UreqHttp {
    fn send(&self, req: &Request) -> Result<Response, TransportError> {
        let mut resp = Self::run(req)?;
        let status = resp.status().as_u16();
        let mut body = Vec::new();
        resp.body_mut().as_reader().take(10 * 1024 * 1024).read_to_end(&mut body).map_err(read_error)?;
        Ok(Response { status, body })
    }

    fn stream_lines(&self, req: &Request, on_line: &mut dyn FnMut(&str) -> bool) -> Result<Response, TransportError> {
        let mut resp = Self::run(req)?;
        let status = resp.status().as_u16();
        let reader = resp.body_mut().as_reader();
        if !(200..300).contains(&status) {
            let mut body = Vec::new();
            reader.take(64 * 1024).read_to_end(&mut body).map_err(read_error)?;
            return Ok(Response { status, body });
        }
        for line in BufReader::new(reader).lines() {
            if !on_line(&line.map_err(read_error)?) {
                break;
            }
        }
        Ok(Response { status, body: Vec::new() })
    }
}

/// `host` aus einer Adresse wie `http://127.0.0.1:11434/x` (für „ist das mein Rechner?“ und Meldungen).
pub fn host_of(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let authority = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    if let Some(inner) = authority.strip_prefix('[') {
        return inner.split(']').next().unwrap_or("").to_string();
    }
    authority.split(':').next().unwrap_or("").to_string()
}

/// Basis-URL aus der Einstellung: ohne Schema gilt http, Slash am Ende weg, ohne Host `None`.
pub fn base_url(text: &str) -> Option<String> {
    let mut s = text.trim().trim_end_matches('/').to_string();
    if s.is_empty() {
        return None;
    }
    if !s.contains("://") {
        s = format!("http://{s}");
    }
    (!host_of(&s).is_empty()).then_some(s)
}

/// Prozentkodierung für Werte in der Query (alles außer Buchstaben, Ziffern und `-._~`).
pub fn percent_encode(s: &str) -> String {
    s.bytes().fold(String::new(), |mut out, b| {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
        out
    })
}

/// `{"error": "…"}` (Ollama) oder `{"error": {"message": "…"}}` (llama.cpp), sonst der Text selbst (200 Zeichen).
pub fn error_message(body: &[u8]) -> String {
    if let Ok(Value::Object(obj)) = serde_json::from_slice::<Value>(body) {
        match obj.get("error") {
            Some(Value::String(s)) => return s.clone(),
            Some(Value::Object(e)) => {
                if let Some(Value::String(s)) = e.get("message") {
                    return s.clone();
                }
            }
            _ => {}
        }
    }
    String::from_utf8_lossy(&body[..body.len().min(200)]).trim().to_string()
}

/// Erstes JSON-Objekt in einer Modellantwort: kleine Modelle setzen gern Codezäune oder Text drumherum.
pub fn first_json_object(text: &str) -> Option<serde_json::Map<String, Value>> {
    let (start, end) = (text.find('{')?, text.rfind('}')?);
    if start >= end {
        return None;
    }
    match serde_json::from_str::<Value>(&text[start..=end]) {
        Ok(Value::Object(m)) => Some(m),
        _ => None,
    }
}

/// Antwort-Body bei 2xx, sonst `ModelError::Http` mit der Meldung aus dem Body.
pub fn checked(resp: Response) -> Result<Vec<u8>, ModelError> {
    if (200..300).contains(&resp.status) { Ok(resp.body) } else { Err(ModelError::Http(resp.status, error_message(&resp.body))) }
}

/// Transportfehler als `ModelError` (Host für die Meldung „nicht erreichbar“).
pub fn model_error(e: TransportError, url: &str) -> ModelError {
    match e {
        TransportError::Timeout => ModelError::Timeout,
        TransportError::Unreachable(_) => ModelError::Unreachable(host_of(url)),
    }
}

/// Anfrage schicken und den Body prüfen.
pub fn call(http: &dyn Http, req: &Request) -> Result<Vec<u8>, ModelError> {
    http.send(req).map_err(|e| model_error(e, &req.url)).and_then(checked)
}

pub fn get(url: String, timeout: Duration) -> Request {
    Request { method: Method::Get, url, body: None, timeout }
}

pub fn with_json(method: Method, url: String, body: &Value, timeout: Duration) -> Request {
    Request { method, url, body: Some(body.to_string().into_bytes()), timeout }
}

/// Fertige Meldung für die Oberfläche (Swift: `LocalModelError.describe`), in der eingestellten Sprache.
pub fn describe(e: &ModelError) -> String {
    use kadrell_i18n::trd;
    match e {
        ModelError::Unreachable(s) => trd!("nicht erreichbar: %@", s.as_str()),
        ModelError::Http(code, s) if s.is_empty() => format!("HTTP {code}"),
        ModelError::Http(code, s) => format!("HTTP {code}: {s}"),
        ModelError::Invalid(s) => s.clone(),
        ModelError::Timeout => trd!("keine Antwort in der Zeit"),
    }
}

pub mod fake {
    //! Attrappe für Tests (auch die der App): Antworten kommen aus einer Funktion, alle Anfragen werden mitgeschrieben.
    use super::*;
    use std::sync::Mutex;

    type Answer = Result<(u16, String), TransportError>;
    type Handler = dyn Fn(&Request) -> Answer + Send + Sync;

    pub struct Fake {
        handler: Box<Handler>,
        pub seen: Mutex<Vec<Request>>,
    }

    impl Fake {
        pub fn new(handler: impl Fn(&Request) -> Answer + Send + Sync + 'static) -> Fake {
            Fake { handler: Box::new(handler), seen: Mutex::new(Vec::new()) }
        }

        pub fn ok(body: &str) -> Fake {
            let body = body.to_string();
            Fake::new(move |_| Ok((200, body.clone())))
        }

        pub fn requests(&self) -> Vec<Request> {
            self.seen.lock().unwrap().clone()
        }

        pub fn body_json(&self, i: usize) -> Value {
            serde_json::from_slice(self.requests()[i].body.as_deref().unwrap_or(b"null")).unwrap()
        }

        fn answer(&self, req: &Request) -> Answer {
            self.seen.lock().unwrap().push(req.clone());
            (self.handler)(req)
        }
    }

    impl Http for Fake {
        fn send(&self, req: &Request) -> Result<Response, TransportError> {
            self.answer(req).map(|(status, body)| Response { status, body: body.into_bytes() })
        }

        fn stream_lines(&self, req: &Request, on_line: &mut dyn FnMut(&str) -> bool) -> Result<Response, TransportError> {
            let (status, body) = self.answer(req)?;
            if !(200..300).contains(&status) {
                return Ok(Response { status, body: body.into_bytes() });
            }
            for line in body.lines() {
                if !on_line(line) {
                    break;
                }
            }
            Ok(Response { status, body: Vec::new() })
        }
    }
}
