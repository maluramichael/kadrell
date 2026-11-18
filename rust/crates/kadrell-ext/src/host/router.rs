//! Lua-Ausgang: `exec`, `http`, `timer` und `system` erledigt der Helper selbst, alles andere geht als Protokollzeile an
//! Kadrell. Ergebnisse kommen als `result`-Zeile zurück in die Warteschlange des Lua-Threads.

use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::fs::File;
use std::io::Write;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Map, Value, json};

use super::exec;
use crate::http::{HttpClient, HttpRequest, UreqClient, title_case_header};
use crate::json::as_int;
use crate::system::{self, CpuProbe};

type TimerState = (Mutex<BinaryHeap<Reverse<(Instant, i64)>>>, Condvar);

/// Einplanung der Timer in einem einzigen Thread: eine Min-Halde nach Fälligkeit.
struct Timers {
    state: Arc<TimerState>,
}

impl Timers {
    fn new(tx: Sender<String>) -> Timers {
        let state: Arc<TimerState> = Arc::new((Mutex::new(BinaryHeap::new()), Condvar::new()));
        let shared = Arc::clone(&state);
        std::thread::Builder::new()
            .name("timers".into())
            .spawn(move || Self::run(&shared, &tx))
            .expect("timer thread");
        Timers { state }
    }

    fn schedule(&self, id: i64, after: f64) {
        // Obergrenze, damit `Instant + Dauer` nicht überläuft.
        let wait = Duration::try_from_secs_f64(after.clamp(0.0, 3.0e8)).unwrap_or_default();
        let (heap, cv) = &*self.state;
        heap.lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(Reverse((Instant::now() + wait, id)));
        cv.notify_one();
    }

    fn run(state: &TimerState, tx: &Sender<String>) {
        let (heap, cv) = state;
        let mut guard = heap.lock().unwrap_or_else(|e| e.into_inner());
        loop {
            let next = guard.peek().map(|Reverse((t, id))| (*t, *id));
            match next {
                None => guard = cv.wait(guard).unwrap_or_else(|e| e.into_inner()),
                Some((due, id)) if due <= Instant::now() => {
                    guard.pop();
                    if tx.send(format!(r#"{{"t":"timer","id":{id}}}"#)).is_err() {
                        return;
                    }
                }
                Some((due, _)) => {
                    guard = cv
                        .wait_timeout(guard, due - Instant::now())
                        .unwrap_or_else(|e| e.into_inner())
                        .0;
                }
            }
        }
    }
}

pub struct Router {
    out: Mutex<File>,
    tx: Sender<String>,
    timers: Timers,
    cpu: Mutex<CpuProbe>,
}

impl Router {
    pub fn new(out: File, tx: Sender<String>) -> Arc<Router> {
        Arc::new(Router {
            out: Mutex::new(out),
            timers: Timers::new(tx.clone()),
            tx,
            cpu: Mutex::new(CpuProbe::default()),
        })
    }

    /// Eine Zeile von Lua. Ohne `id` oder mit anderem `t` geht sie unverändert an Kadrell.
    pub fn route(self: &Arc<Self>, line: &str) {
        let Ok(msg) = serde_json::from_str::<Value>(line) else {
            return self.write_line(line);
        };
        let Some(id) = msg.get("id").and_then(as_int) else {
            return self.write_line(line);
        };
        match msg.get("t").and_then(Value::as_str) {
            Some("exec") => self.exec(id, &msg),
            Some("http") => self.http(id, &msg),
            Some("timer") => self
                .timers
                .schedule(id, msg.get("after").and_then(Value::as_f64).unwrap_or(0.0)),
            Some("system") => self.system(id, &msg),
            _ => self.write_line(line),
        }
    }

    /// Protokollzeile an Kadrell. Unmittelbar geschrieben, damit auch die letzte Zeile vor `os.exit` ankommt.
    pub fn write_line(&self, line: &str) {
        let mut data = Vec::with_capacity(line.len() + 1);
        data.extend_from_slice(line.as_bytes());
        data.push(b'\n');
        let mut out = self.out.lock().unwrap_or_else(|e| e.into_inner());
        let _ = out.write_all(&data);
        let _ = out.flush();
    }

    /// Ergebnis einer Hintergrundarbeit zurück in Lua.
    fn deliver(&self, id: i64, mut fields: Map<String, Value>) {
        fields.insert("t".into(), json!("result"));
        fields.insert("id".into(), json!(id));
        let _ = self.tx.send(Value::Object(fields).to_string());
    }

    fn spawn(self: &Arc<Self>, work: impl FnOnce(&Router) + Send + 'static) {
        let me = Arc::clone(self);
        std::thread::spawn(move || work(&me));
    }

    fn exec(self: &Arc<Self>, id: i64, msg: &Value) {
        let argv: Vec<String> = msg
            .get("argv")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        let cwd = msg.get("cwd").and_then(Value::as_str).map(String::from);
        let timeout = msg.get("timeout").and_then(Value::as_f64).unwrap_or(30.0);
        self.spawn(move |r| r.deliver(id, exec::run(&argv, cwd.as_deref(), timeout)));
    }

    fn http(self: &Arc<Self>, id: i64, msg: &Value) {
        let url = msg
            .get("url")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let lower = url.to_ascii_lowercase();
        // Nur http/https: file:// und andere Schemata würden lokale Dateien oder Dienste erreichbar machen (SSRF).
        if !(lower.starts_with("http://") || lower.starts_with("https://")) {
            return self.deliver(id, http_failure("URL muss http oder https sein"));
        }
        let request = HttpRequest {
            method: msg
                .get("method")
                .and_then(Value::as_str)
                .unwrap_or("GET")
                .to_string(),
            url,
            headers: msg
                .get("headers")
                .and_then(Value::as_object)
                .map(|h| {
                    h.iter()
                        .filter_map(|(k, v)| v.as_str().map(|v| (k.clone(), v.to_string())))
                        .collect()
                })
                .unwrap_or_default(),
            body: msg
                .get("body")
                .and_then(Value::as_str)
                .map(|b| b.as_bytes().to_vec()),
            timeout: Duration::try_from_secs_f64(
                msg.get("timeout")
                    .and_then(Value::as_f64)
                    .unwrap_or(30.0)
                    .clamp(0.001, 86_400.0),
            )
            .unwrap_or(Duration::from_secs(30)),
            max_body: 64 << 20,
        };
        self.spawn(move |r| match UreqClient.send(request) {
            Ok(resp) => r.deliver(id, http_fields(&resp)),
            Err(e) => r.deliver(id, http_failure(&e.to_string())),
        });
    }

    /// `kadrell.system`: nur lesend, gemessen wird nur auf Anfrage, immer auf einem Arbeitsthread (ein langsames
    /// Netzlaufwerk oder die 250 ms der ersten CPU-Messung sollen Lua nicht aufhalten).
    fn system(self: &Arc<Self>, id: i64, msg: &Value) {
        let op = msg
            .get("op")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let path = msg
            .get("path")
            .and_then(Value::as_str)
            .filter(|p| !p.is_empty())
            .unwrap_or("/")
            .to_string();
        self.spawn(move |r| r.deliver(id, system_fields(r, &op, &path)));
    }
}

fn http_fields(resp: &crate::http::HttpResponse) -> Map<String, Value> {
    let mut headers = Map::new();
    for (k, v) in &resp.headers {
        headers.insert(title_case_header(k), json!(v));
    }
    let mut m = Map::new();
    m.insert("status".into(), json!(resp.status));
    m.insert("headers".into(), Value::Object(headers));
    m.insert("body".into(), json!(String::from_utf8_lossy(&resp.body)));
    m
}

fn http_failure(error: &str) -> Map<String, Value> {
    let mut m = Map::new();
    m.insert("status".into(), json!(0));
    m.insert("headers".into(), json!({}));
    m.insert("body".into(), json!(""));
    m.insert("error".into(), json!(error));
    m
}

fn error_fields(text: &str) -> Map<String, Value> {
    let mut m = Map::new();
    m.insert("error".into(), json!(text));
    m
}

fn system_fields(router: &Router, op: &str, path: &str) -> Map<String, Value> {
    match op {
        "cpu" => {
            let sample = router
                .cpu
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .sample();
            sample.map_or_else(
                || error_fields("cpu load not available"),
                |c| system::cpu_fields(&c),
            )
        }
        "mem" => system::mem().unwrap_or_else(|| error_fields("memory stats not available")),
        "disk" => system::disk(path)
            .unwrap_or_else(|| error_fields(&format!("volume not readable: {path}"))),
        _ => error_fields("unknown system op"),
    }
}
