//! HTTP hinter einem Trait: Tests laufen mit einer Fake-Implementierung, nie gegen das Netz.

use std::io::Read;
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HttpRequest {
    pub method: Method,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
    pub timeout: Duration,
}

impl HttpRequest {
    /// Wert eines Headers, ohne Rücksicht auf Groß- und Kleinschreibung.
    pub fn header(&self, name: &str) -> Option<&str> {
        header_of(&self.headers, name)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl HttpResponse {
    pub fn header(&self, name: &str) -> Option<&str> {
        header_of(&self.headers, name)
    }
}

fn header_of<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
}

/// Transportfehler (kein Status): keine Verbindung, Zeitüberschreitung. Der Text enthält nie Header oder Token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpError(pub String);

impl std::fmt::Display for HttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for HttpError {}

pub trait HttpClient: Send + Sync {
    /// Schickt die Anfrage ab. Jeder HTTP-Status (auch 4xx/5xx) kommt als `Ok` zurück.
    fn send(&self, req: &HttpRequest) -> Result<HttpResponse, HttpError>;
}

/// Echter Client auf Basis von `ureq` (blockierend, rustls).
#[derive(Clone, Copy, Debug, Default)]
pub struct UreqClient;

impl HttpClient for UreqClient {
    fn send(&self, req: &HttpRequest) -> Result<HttpResponse, HttpError> {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(req.timeout))
            .http_status_as_error(false)
            .build()
            .into();
        let mut builder = ureq::http::Request::builder().uri(req.url.as_str());
        for (k, v) in &req.headers {
            builder = builder.header(k.as_str(), v.as_str());
        }
        let result = match req.method {
            Method::Get => agent.run(builder.method("GET").body(()).map_err(request_error)?),
            Method::Post => {
                let body = req.body.clone().unwrap_or_default();
                agent.run(builder.method("POST").body(body).map_err(request_error)?)
            }
        };
        let mut resp = result.map_err(|e| HttpError(transport_text(&e)))?;
        let status = resp.status().as_u16();
        let headers = resp
            .headers()
            .iter()
            .filter_map(|(k, v)| {
                v.to_str()
                    .ok()
                    .map(|v| (k.as_str().to_string(), v.to_string()))
            })
            .collect();
        let mut body = Vec::new();
        resp.body_mut()
            .as_reader()
            .take(10 * 1024 * 1024)
            .read_to_end(&mut body)
            .map_err(|e| HttpError(e.to_string()))?;
        Ok(HttpResponse {
            status,
            headers,
            body,
        })
    }
}

fn request_error(e: ureq::http::Error) -> HttpError {
    HttpError(format!("ungültige Anfrage: {e}"))
}

fn transport_text(e: &ureq::Error) -> String {
    e.to_string()
}
