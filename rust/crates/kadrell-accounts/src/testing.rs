//! Fake-HTTP für Tests: gibt vorbereitete Antworten zurück und merkt sich jede Anfrage. Kein Netz.

use std::sync::Mutex;

use crate::http::{HttpClient, HttpError, HttpRequest, HttpResponse};

type Handler = Box<dyn FnMut(&HttpRequest) -> Result<HttpResponse, HttpError> + Send>;

pub struct FakeHttp {
    handler: Mutex<Handler>,
    requests: Mutex<Vec<HttpRequest>>,
}

impl FakeHttp {
    pub fn new(
        handler: impl FnMut(&HttpRequest) -> Result<HttpResponse, HttpError> + Send + 'static,
    ) -> Self {
        Self {
            handler: Mutex::new(Box::new(handler)),
            requests: Mutex::new(Vec::new()),
        }
    }

    /// Antwortet immer mit diesem Status und JSON-Text.
    pub fn json(status: u16, body: &str) -> Self {
        let body = body.as_bytes().to_vec();
        Self::new(move |_| {
            Ok(HttpResponse {
                status,
                headers: Vec::new(),
                body: body.clone(),
            })
        })
    }

    /// Antwortet immer mit diesem Status und einem Header.
    pub fn status_with_header(status: u16, name: &str, value: &str) -> Self {
        let header = (name.to_string(), value.to_string());
        Self::new(move |_| {
            Ok(HttpResponse {
                status,
                headers: vec![header.clone()],
                body: Vec::new(),
            })
        })
    }

    /// Jede Anfrage scheitert mit einem Transportfehler.
    pub fn offline() -> Self {
        Self::new(|_| Err(HttpError("offline".into())))
    }

    pub fn requests(&self) -> Vec<HttpRequest> {
        self.requests.lock().unwrap().clone()
    }
}

impl HttpClient for FakeHttp {
    fn send(&self, req: &HttpRequest) -> Result<HttpResponse, HttpError> {
        self.requests.lock().unwrap().push(req.clone());
        (self.handler.lock().unwrap())(req)
    }
}
