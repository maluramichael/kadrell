//! HTTP für `kadrell.http` im Helper und den GitHub-Katalog in der App. Die App-Seite spricht nur das Trait, die
//! echte Umsetzung (`UreqClient`) bringt die Crate mit.

use std::time::Duration;

#[derive(Debug, Clone, PartialEq)]
pub struct HttpRequest {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
    pub timeout: Duration,
    /// Obergrenze für den Antworttext in Bytes.
    pub max_body: u64,
}

impl HttpRequest {
    pub fn get(url: &str) -> HttpRequest {
        HttpRequest {
            method: "GET".into(),
            url: url.into(),
            headers: Vec::new(),
            body: None,
            timeout: Duration::from_secs(30),
            max_body: 64 << 20,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HttpError {
    /// Der Antworttext ist größer als `max_body`.
    TooLarge,
    Other(String),
}

impl std::fmt::Display for HttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooLarge => f.write_str("response too large"),
            Self::Other(m) => f.write_str(m),
        }
    }
}

impl std::error::Error for HttpError {}

/// Ein HTTP-Client. Eine Antwort mit Fehlerstatus (404, 403, …) ist trotzdem ein `Ok`.
pub trait HttpClient: Send + Sync {
    fn send(&self, request: HttpRequest) -> Result<HttpResponse, HttpError>;
}

/// Echter Client auf Basis von `ureq`; folgt Weiterleitungen wie `URLSession`.
#[derive(Debug, Clone, Copy, Default)]
pub struct UreqClient;

impl HttpClient for UreqClient {
    fn send(&self, request: HttpRequest) -> Result<HttpResponse, HttpError> {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_global(Some(request.timeout))
            .build()
            .into();
        let mut builder = ureq::http::Request::builder()
            .method(request.method.as_str())
            .uri(request.url.as_str());
        for (k, v) in &request.headers {
            builder = builder.header(k.as_str(), v.as_str());
        }
        let response = match request.body.clone() {
            Some(body) => agent.run(builder.body(body).map_err(other)?),
            None => agent.run(builder.body(()).map_err(other)?),
        }
        .map_err(other)?;
        read_response(response, request.max_body)
    }
}

fn other(e: impl std::fmt::Display) -> HttpError {
    HttpError::Other(e.to_string())
}

fn read_response(
    response: ureq::http::Response<ureq::Body>,
    max_body: u64,
) -> Result<HttpResponse, HttpError> {
    let status = response.status().as_u16();
    let headers: Vec<(String, String)> = response
        .headers()
        .iter()
        .map(|(k, v)| {
            (
                k.as_str().to_string(),
                String::from_utf8_lossy(v.as_bytes()).into_owned(),
            )
        })
        .collect();
    let declared = headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, v)| v.parse::<u64>().ok());
    if declared.is_some_and(|n| n > max_body) {
        return Err(HttpError::TooLarge);
    }
    let mut response = response;
    let body = response
        .body_mut()
        .with_config()
        .limit(max_body)
        .read_to_vec()
        .map_err(|e| match e {
            ureq::Error::BodyExceedsLimit(_) => HttpError::TooLarge,
            other_err => other(other_err),
        })?;
    Ok(HttpResponse {
        status,
        headers,
        body,
    })
}

/// `x-foo-bar` wird zu `X-Foo-Bar`, so wie Server ihre Header meist schreiben und Extensions sie erwarten.
pub fn title_case_header(name: &str) -> String {
    name.split('-')
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => {
                    first.to_ascii_uppercase().to_string() + &chars.as_str().to_ascii_lowercase()
                }
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join("-")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_names_are_title_cased() {
        assert_eq!(title_case_header("x-a"), "X-A");
        assert_eq!(title_case_header("content-length"), "Content-Length");
        assert_eq!(title_case_header("ETAG"), "Etag");
    }
}
