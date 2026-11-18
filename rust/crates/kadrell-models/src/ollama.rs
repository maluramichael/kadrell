//! Client für einen laufenden Ollama-Dienst (Swift: `OllamaClient.swift`). Endpunkte laut API-Doku (github.com/ollama/
//! ollama, `docs/api.md`): `/api/version`, `/api/tags`, `/api/pull` (Stream aus JSON-Zeilen), `/api/delete`,
//! `/api/chat` (`format` = JSON-Schema, `think`, `keep_alive`). Dazu die Modellsuche über die Webseite ollama.com.

use std::time::Duration;

use kadrell_claude::local_model::ModelError;
use serde_json::{Value, json};

use crate::http::{self, Http, Method, Request, call, error_message, get, model_error, with_json};
use crate::types::{LocalModel, LocalProviderKind, PullProgress, SearchHit};

pub struct OllamaClient<'a> {
    pub base: String,
    pub http: &'a dyn Http,
}

/// Wie ein Download endete.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PullEnd {
    Done,
    /// Der Aufrufer hat abgebrochen (Ollama setzt einen späteren Download fort).
    Cancelled,
}

impl OllamaClient<'_> {
    fn url(&self, path: &str) -> String {
        format!("{}/{path}", self.base)
    }

    fn json_of(&self, path: &str, timeout: u64) -> Result<Value, ModelError> {
        let body = call(self.http, &get(self.url(path), Duration::from_secs(timeout)))?;
        serde_json::from_slice(&body).map_err(|e| ModelError::Invalid(e.to_string()))
    }

    pub fn version(&self) -> Result<String, ModelError> {
        let v = self.json_of("api/version", 3)?;
        v.get("version").and_then(Value::as_str).map(str::to_string).ok_or_else(|| ModelError::Invalid("keine Version".into()))
    }

    /// Installierte Modelle, nach Name sortiert (ohne Rücksicht auf Groß- und Kleinschreibung).
    pub fn models(&self) -> Result<Vec<LocalModel>, ModelError> {
        let v = self.json_of("api/tags", 5)?;
        let items = v.get("models").and_then(Value::as_array).ok_or_else(|| ModelError::Invalid("keine Modelle".into()))?;
        let mut models: Vec<LocalModel> = items.iter().filter_map(model_of).collect();
        models.sort_by_key(|m| m.name.to_lowercase());
        Ok(models)
    }

    /// Lädt ein Modell aus der Ollama-Bibliothek. Der Dienst streamt eine JSON-Zeile je Fortschritt, die letzte hat
    /// `status: success`. `progress` gibt `false` zurück, um abzubrechen.
    pub fn pull(&self, name: &str, progress: &mut dyn FnMut(PullProgress) -> bool) -> Result<PullEnd, ModelError> {
        let req = with_json(Method::Post, self.url("api/pull"), &json!({"model": name, "stream": true}), Duration::from_secs(600));
        let (mut done, mut failure, mut cancelled) = (false, None, false);
        let resp = self
            .http
            .stream_lines(&req, &mut |line| match parse_pull_line(line) {
                Ok(Some(p)) => {
                    done |= p.status == "success";
                    cancelled = !progress(p);
                    !cancelled
                }
                Ok(None) => true,
                Err(e) => {
                    failure = Some(e);
                    false
                }
            })
            .map_err(|e| model_error(e, &req.url))?;
        if !(200..300).contains(&resp.status) {
            return Err(ModelError::Http(resp.status, error_message(&resp.body)));
        }
        match (failure, cancelled, done) {
            (Some(e), _, _) => Err(e),
            (None, true, _) => Ok(PullEnd::Cancelled),
            (None, false, true) => Ok(PullEnd::Done),
            (None, false, false) => Err(ModelError::Invalid(kadrell_i18n::trd!("Download ohne Abschluss beendet"))),
        }
    }

    pub fn delete(&self, name: &str) -> Result<(), ModelError> {
        call(self.http, &with_json(Method::Delete, self.url("api/delete"), &json!({"model": name}), Duration::from_secs(30))).map(|_| ())
    }

    /// Eine Antwort ohne Stream. `think: false`: Denkmodelle (qwen3) antworten sonst erst nach langem Nachdenken.
    /// Modelle ohne Denkmodus lehnen `think` ab; dann einmal ohne.
    pub fn chat(&self, model: &str, system: &str, user: &str, schema: Option<&Value>, timeout: Duration) -> Result<String, ModelError> {
        match self.chat_once(model, system, user, schema, Some(false), timeout) {
            Err(ModelError::Http(_, message)) if message.to_lowercase().contains("think") => self.chat_once(model, system, user, schema, None, timeout),
            other => other,
        }
    }

    fn chat_once(&self, model: &str, system: &str, user: &str, schema: Option<&Value>, think: Option<bool>, timeout: Duration) -> Result<String, ModelError> {
        let mut body = json!({
            "model": model, "stream": false, "keep_alive": "10m",
            "options": {"temperature": 0},
            "messages": [{"role": "system", "content": system}, {"role": "user", "content": user}],
        });
        if let Some(schema) = schema {
            body["format"] = schema.clone();
        }
        if let Some(think) = think {
            body["think"] = Value::Bool(think);
        }
        let bytes = call(self.http, &with_json(Method::Post, self.url("api/chat"), &body, timeout))?;
        let reply: Value = serde_json::from_slice(&bytes).map_err(|e| ModelError::Invalid(e.to_string()))?;
        reply["message"]["content"].as_str().map(str::to_string).ok_or_else(|| ModelError::Invalid("keine Antwort".into()))
    }

    /// Leere Nachrichtenliste lädt das Modell nur in den Speicher: beim Öffnen des Schnellstarts, damit die eigentliche
    /// Frage danach ohne Ladezeit antwortet. Fehler zählen nicht.
    pub fn preload(&self, model: &str) {
        let body = json!({"model": model, "messages": [], "keep_alive": "10m"});
        let _ = call(self.http, &with_json(Method::Post, self.url("api/chat"), &body, Duration::from_secs(60)));
    }
}

fn model_of(item: &Value) -> Option<LocalModel> {
    let name = item.get("name")?.as_str()?;
    let details = |key: &str| item["details"].get(key).and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_string);
    let mut m = LocalModel::new(LocalProviderKind::Ollama, name);
    m.size = item.get("size").and_then(Value::as_i64);
    m.detail = [details("parameter_size"), details("quantization_level")].into_iter().flatten().collect::<Vec<_>>().join(" · ");
    Some(m)
}

/// Eine Zeile aus `/api/pull`. `{"error": …}` mitten im Stream (Modell gibt es nicht) wird zum Fehler.
pub fn parse_pull_line(line: &str) -> Result<Option<PullProgress>, ModelError> {
    let Ok(Value::Object(obj)) = serde_json::from_str::<Value>(line) else { return Ok(None) };
    if let Some(e) = obj.get("error").and_then(Value::as_str) {
        return Err(ModelError::Invalid(e.to_string()));
    }
    let Some(status) = obj.get("status").and_then(Value::as_str) else { return Ok(None) };
    let number = |k: &str| obj.get(k).and_then(Value::as_i64).unwrap_or(0);
    Ok(Some(PullProgress { status: status.to_string(), completed: number("completed"), total: number("total") }))
}

// MARK: Suche

pub const SEARCH_PAGE: &str = "https://ollama.com/search";

/// Modellsuche über die Webseite ollama.com/search. Eine offizielle Such-Schnittstelle gibt es nicht. Gelesen werden nur
/// die Links `/library/<name>`, die Beschreibung und die Größen-Tags; ändert die Seite ihr Markup, bleiben im
/// schlimmsten Fall nur die Namen übrig. Laden geht ohnehin immer über den eingetippten Namen, die Suche ist nur Hilfe.
pub fn search(http: &dyn Http, query: &str) -> Result<Vec<SearchHit>, ModelError> {
    let req: Request = get(format!("{SEARCH_PAGE}?q={}", http::percent_encode(query)), Duration::from_secs(10));
    let body = call(http, &req)?;
    Ok(parse_search(&String::from_utf8_lossy(&body)))
}

/// Ein Treffer je `href="/library/<name>"`; Beschreibung (erstes `<p>`) und Tags wie `4b` aus demselben Block.
pub fn parse_search(html: &str) -> Vec<SearchHit> {
    let mut hits: Vec<SearchHit> = Vec::new();
    for block in html.split("href=\"/library/").skip(1) {
        let Some(end) = block.find('"') else { continue };
        let name = &block[..end];
        let valid = !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '/' | '-'));
        if !valid || hits.iter().any(|h| h.name == name) {
            continue;
        }
        let body = block.split("</li>").next().unwrap_or(block);
        hits.push(SearchHit { name: name.to_string(), summary: summary(body), tags: size_tags(body) });
    }
    hits
}

fn summary(block: &str) -> String {
    let Some(p) = block.find("<p") else { return String::new() };
    let rest = &block[p + 2..];
    let Some(open) = rest.find('>') else { return String::new() };
    let rest = &rest[open + 1..];
    let Some(close) = rest.find("</p>") else { return String::new() };
    decode_entities(&rest[..close]).trim().to_string()
}

/// Größen wie `0.6b`, `4b`, `270m`, `8x7b` als Text eines Tag-Spans mit `class`. Die Download-Zahl („42M“) steht in
/// einem Span ohne Klasse und zählt nicht.
fn size_tags(block: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for part in block.split("<span").skip(1) {
        let Some(close) = part.find('>') else { continue };
        let (attrs, text) = part.split_at(close);
        let text = &text[1..];
        let Some(inner) = text.find("</span>").map(|i| text[..i].trim()) else { continue };
        if attrs.contains("class=\"") && is_size(inner) && !out.contains(&inner.to_lowercase()) {
            out.push(inner.to_lowercase());
        }
    }
    out
}

/// `[0-9]+(.[0-9]+)?(x[0-9]+)?[bm]`, Kleinbuchstaben.
fn is_size(s: &str) -> bool {
    let Some(rest) = s.strip_suffix(['b', 'm']) else { return false };
    let digits = |t: &str| !t.is_empty() && t.chars().all(|c| c.is_ascii_digit());
    let (main, times) = match rest.split_once('x') {
        Some((a, b)) => (a, Some(b)),
        None => (rest, None),
    };
    let number = match main.split_once('.') {
        Some((a, b)) => digits(a) && digits(b),
        None => digits(main),
    };
    number && times.is_none_or(digits)
}

fn decode_entities(s: &str) -> String {
    [("&amp;", "&"), ("&lt;", "<"), ("&gt;", ">"), ("&quot;", "\""), ("&#39;", "'"), ("&#x27;", "'")]
        .iter()
        .fold(s.to_string(), |acc, (from, to)| acc.replace(from, to))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::fake::Fake;

    const BASE: &str = "http://localhost:11434";

    fn client(fake: &Fake) -> OllamaClient<'_> {
        OllamaClient { base: BASE.into(), http: fake }
    }

    #[test]
    fn lists_installed_models_sorted() {
        let fake = Fake::new(|r| {
            assert!(r.url.ends_with("/api/tags"));
            Ok((200, r#"{"models":[{"name":"qwen3:4b","size":2500000000,"details":{"parameter_size":"4.0B","quantization_level":"Q4_K_M"}},
                       {"name":"gemma3:270m","size":291000000,"details":{}}]}"#.into()))
        });
        let models = client(&fake).models().unwrap();
        assert_eq!(models.iter().map(|m| m.name.as_str()).collect::<Vec<_>>(), ["gemma3:270m", "qwen3:4b"], "nur was installiert ist, sortiert");
        assert_eq!(models[1].detail, "4.0B · Q4_K_M");
        assert_eq!(models[1].provider, LocalProviderKind::Ollama);
        assert_eq!(models[0].detail, "");
    }

    #[test]
    fn pull_streams_progress_until_success() {
        let fake = Fake::ok("{\"status\":\"pulling manifest\"}\n{\"status\":\"pulling abc\",\"digest\":\"abc\",\"total\":1000,\"completed\":250}\n{\"status\":\"pulling abc\",\"digest\":\"abc\",\"total\":1000,\"completed\":1000}\n{\"status\":\"success\"}\n\n");
        let mut seen = Vec::new();
        let end = client(&fake).pull("gemma3:270m", &mut |p| { seen.push(p); true }).unwrap();
        assert_eq!(end, PullEnd::Done);
        assert_eq!(seen.iter().map(|p| p.status.as_str()).collect::<Vec<_>>(), ["pulling manifest", "pulling abc", "pulling abc", "success"]);
        assert_eq!(seen[1].fraction(), Some(0.25));
        assert_eq!(seen[0].fraction(), None, "ohne Gesamtgröße kein Prozentwert");
        let req = &fake.requests()[0];
        assert!(req.url.ends_with("/api/pull"));
        assert_eq!(fake.body_json(0)["model"], "gemma3:270m");
    }

    #[test]
    fn pull_error_in_stream_fails() {
        let fake = Fake::ok("{\"status\":\"pulling manifest\"}\n{\"error\":\"pull model manifest: file does not exist\"}\n");
        let err = client(&fake).pull("gibtsnicht", &mut |_| true).unwrap_err();
        assert!(crate::http::describe(&err).contains("does not exist"));
    }

    #[test]
    fn pull_without_success_is_an_error_and_cancel_is_not() {
        let fake = Fake::ok("{\"status\":\"pulling manifest\"}\n");
        assert!(client(&fake).pull("x", &mut |_| true).is_err(), "Ende ohne success");
        let fake = Fake::ok("{\"status\":\"pulling manifest\"}\n{\"status\":\"success\"}\n");
        assert_eq!(client(&fake).pull("x", &mut |_| false).unwrap(), PullEnd::Cancelled);
        let fake = Fake::new(|_| Ok((404, "{\"error\":\"nope\"}".into())));
        assert_eq!(client(&fake).pull("x", &mut |_| true).unwrap_err(), ModelError::Http(404, "nope".into()));
    }

    #[test]
    fn delete_sends_model_name_as_body() {
        let fake = Fake::ok("");
        client(&fake).delete("gemma3:270m").unwrap();
        let req = &fake.requests()[0];
        assert_eq!(req.method, Method::Delete);
        assert!(req.url.ends_with("/api/delete"));
        assert_eq!(fake.body_json(0)["model"], "gemma3:270m");
        let fake = Fake::new(|_| Ok((404, "{\"error\":\"model 'x' not found\"}".into())));
        assert_eq!(client(&fake).delete("x").unwrap_err(), ModelError::Http(404, "model 'x' not found".into()));
    }

    /// Schema als `format`, ohne Denken und ohne Stream; Modelle ohne Denkmodus lehnen `think` ab, dann einmal ohne.
    #[test]
    fn chat_sends_schema_and_retries_without_think() {
        let fake = Fake::new(|r| {
            let j: Value = serde_json::from_slice(r.body.as_deref().unwrap()).unwrap();
            if j.get("think").is_some() {
                return Ok((400, "{\"error\":\"\\\"gemma3:270m\\\" does not support thinking\"}".into()));
            }
            Ok((200, "{\"message\":{\"role\":\"assistant\",\"content\":\"{\\\"index\\\":1}\"}}".into()))
        });
        let schema = kadrell_claude::quick_start::schema();
        let reply = client(&fake).chat("gemma3:270m", "s", "u", Some(&schema), Duration::from_secs(5)).unwrap();
        assert_eq!(reply, "{\"index\":1}");
        assert_eq!(fake.requests().len(), 2);
        let first = fake.body_json(0);
        assert_eq!(first["think"], false);
        assert_eq!(first["stream"], false);
        assert!(first["format"].is_object());
        assert_eq!(first["messages"][0]["role"], "system");
        assert!(fake.body_json(1).get("think").is_none());
    }

    #[test]
    fn unreachable_server_names_the_host() {
        let fake = Fake::new(|_| Err(crate::http::TransportError::Unreachable("refused".into())));
        assert_eq!(client(&fake).version().unwrap_err(), ModelError::Unreachable("localhost".into()));
        let fake = Fake::new(|_| Err(crate::http::TransportError::Timeout));
        assert_eq!(client(&fake).version().unwrap_err(), ModelError::Timeout);
    }

    #[test]
    fn version_and_preload() {
        let fake = Fake::ok("{\"version\":\"0.9.0\"}");
        assert_eq!(client(&fake).version().unwrap(), "0.9.0");
        client(&fake).preload("m");
        assert_eq!(fake.body_json(1)["messages"], json!([]));
    }

    /// Ausschnitt aus dem echten Markup von ollama.com/search (Oktober 2026).
    #[test]
    fn search_parses_names_summaries_and_sizes() {
        let html = r#"
        <li class="x"><a href="/library/qwen3" class="group"><h2 title="qwen3"><span >qwen3</span></h2>
        <p class="mt-1 line-clamp-2">Qwen3 is the latest generation &amp; more.</p>
        <span class="inline-flex">tools</span><span class="inline-flex rounded-md bg-[#ddf4ff]">0.6b</span>
        <span class="inline-flex rounded-md bg-[#ddf4ff]">4b</span><span title="42,001,875 downloads"><span >42M</span></span></a></li>
        <li><a href="/library/gemma3"><p>Gemma</p><span class="inline-flex">270m</span></a></li>
        <a href="/library/qwen3">doppelt</a>
        "#;
        let hits = parse_search(html);
        assert_eq!(hits.iter().map(|h| h.name.as_str()).collect::<Vec<_>>(), ["qwen3", "gemma3"]);
        assert_eq!(hits[0].summary, "Qwen3 is the latest generation & more.");
        assert_eq!(hits[0].tags, ["0.6b", "4b"], "Download-Zahl „42M“ ist keine Größe");
        assert_eq!(hits[1].tags, ["270m"]);
    }

    #[test]
    fn search_asks_the_web_page_with_an_encoded_query() {
        let fake = Fake::ok("<a href=\"/library/qwen3\"></a>");
        let hits = search(&fake, "qwen 3&x").unwrap();
        assert_eq!(hits[0].name, "qwen3");
        assert_eq!(fake.requests()[0].url, "https://ollama.com/search?q=qwen%203%26x");
    }

    #[test]
    fn size_pattern() {
        for ok in ["0.6b", "4b", "270m", "8x7b", "1.5b"] {
            assert!(is_size(ok), "{ok}");
        }
        for bad in ["42M", "tools", "b", "x7b", "1.b", "4", "latest"] {
            assert!(!is_size(bad), "{bad}");
        }
    }
}
