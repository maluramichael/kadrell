//! `llama-server` (llama.cpp) im Router-Modus: Client und Kindprozess (Swift: `LlamaClient.swift`).
//!
//! Endpunkte laut `tools/server/README.md` und `server-models.cpp` der installierten Version (0.6.0): `GET /health`,
//! `GET /models` (Cache, `--models-dir`, Status je Modell), `POST /models/load`, `DELETE /models?model=` (nur Cache-
//! Modelle, `can_remove`), `POST /v1/chat/completions`. Herunterladen (`POST /models`) bietet Kadrell nicht an: der
//! eigene Server läuft mit `--offline`, Modelle kommen per `llama-server -hf` in den Cache.

use std::collections::BTreeMap;
use std::net::TcpListener;
use std::sync::Mutex;
use std::time::Duration;

use kadrell_claude::cli::{find_in_path, path_exts};
use kadrell_claude::local_model::ModelError;
use kadrell_claude::paths;
use serde_json::{Value, json};

use crate::guard::ProcessGuard;
use crate::http::{Http, Method, call, get, percent_encode, with_json};
use crate::types::{LocalModel, LocalProviderKind};

pub struct LlamaClient<'a> {
    pub base: String,
    pub http: &'a dyn Http,
}

impl LlamaClient<'_> {
    fn url(&self, path: &str) -> String {
        format!("{}/{path}", self.base)
    }

    pub fn healthy(&self) -> bool {
        call(self.http, &get(self.url("health"), Duration::from_secs(2))).is_ok()
    }

    pub fn models(&self) -> Result<Vec<LocalModel>, ModelError> {
        let body = call(self.http, &get(self.url("models"), Duration::from_secs(5)))?;
        let v: Value = serde_json::from_slice(&body).map_err(|e| ModelError::Invalid(e.to_string()))?;
        let list = v.get("data").and_then(Value::as_array).ok_or_else(|| ModelError::Invalid("keine Modelle".into()))?;
        let mut models: Vec<LocalModel> = list.iter().filter_map(model_of).collect();
        models.sort_by_key(|m| m.name.to_lowercase());
        Ok(models)
    }

    /// Modell vorab laden, damit die erste Frage ohne Ladezeit antwortet. Geladen ist geladen: der Fehler „already
    /// loaded“ zählt nicht.
    pub fn load(&self, name: &str) {
        let _ = call(self.http, &with_json(Method::Post, self.url("models/load"), &json!({"model": name}), Duration::from_secs(120)));
    }

    pub fn delete(&self, name: &str) -> Result<(), ModelError> {
        let url = format!("{}?model={}", self.url("models"), percent_encode(name));
        call(self.http, &crate::http::Request { method: Method::Delete, url, body: None, timeout: Duration::from_secs(30) }).map(|_| ())
    }

    /// OpenAI-kompatibel; `response_format` mit Schema und `enable_thinking: false` wie in der README beschrieben.
    pub fn chat(&self, model: &str, system: &str, user: &str, schema: Option<&Value>, timeout: Duration) -> Result<String, ModelError> {
        let mut body = json!({
            "model": model, "temperature": 0, "max_tokens": 400,
            "chat_template_kwargs": {"enable_thinking": false},
            "messages": [{"role": "system", "content": system}, {"role": "user", "content": user}],
        });
        if let Some(schema) = schema {
            body["response_format"] = json!({"type": "json_schema", "schema": schema});
        }
        let bytes = call(self.http, &with_json(Method::Post, self.url("v1/chat/completions"), &body, timeout))?;
        let reply: Value = serde_json::from_slice(&bytes).map_err(|e| ModelError::Invalid(e.to_string()))?;
        Ok(reply["choices"][0]["message"]["content"].as_str().unwrap_or("").to_string())
    }
}

fn model_of(e: &Value) -> Option<LocalModel> {
    let name = e.get("id")?.as_str()?;
    let state = e["status"]["value"].as_str().unwrap_or("");
    let mut m = LocalModel::new(LocalProviderKind::LlamaCpp, name);
    m.detail = state.to_string();
    m.can_remove = e.get("can_remove").and_then(Value::as_bool).unwrap_or(false);
    m.downloading = state == "downloading";
    Some(m)
}

// MARK: Kindprozess

/// Argumente des eigenen Servers: Router-Modus auf 127.0.0.1, ohne Web-Oberfläche, höchstens ein Modell gleichzeitig
/// geladen, `--offline`: ohne das prüft jede Modell-Instanz beim Laden erst Hugging Face und hing dabei minutenlang bei
/// „initializing“; offline lädt dasselbe Cache-Modell in rund 2 s.
pub fn server_args(port: u16, models_dir: &str, home: &str) -> Vec<String> {
    let mut args: Vec<String> = ["--host", "127.0.0.1", "--port", &port.to_string(), "--no-webui", "--offline", "--models-max", "1"].map(String::from).to_vec();
    if !models_dir.is_empty() {
        args.extend(["--models-dir".to_string(), paths::expand_tilde(models_dir, home)]);
    }
    args
}

/// Freier TCP-Port auf 127.0.0.1: an Port 0 binden, zugeteilten Port lesen, wieder schließen.
pub fn free_port() -> Option<u16> {
    TcpListener::bind("127.0.0.1:0").ok()?.local_addr().ok().map(|a| a.port())
}

fn env_path(env: &BTreeMap<String, String>) -> String {
    env.iter().find(|(k, _)| k.eq_ignore_ascii_case("PATH")).map(|(_, v)| v.clone()).unwrap_or_default()
}

/// `llama-server` im PATH, sonst an den üblichen Orten (Homebrew, `~/.local/bin`). Leer = nicht gefunden.
pub fn autodetect(env: &BTreeMap<String, String>, home: &str, is_file: &dyn Fn(&str) -> bool) -> String {
    let mut dirs = vec![env_path(env)];
    let sep = if cfg!(windows) { ";" } else { ":" };
    dirs.extend(["/opt/homebrew/bin".to_string(), "/usr/local/bin".to_string(), format!("{home}/.local/bin")]);
    let mut lookup = env.clone();
    lookup.insert("PATH".into(), dirs.join(sep));
    find_in_path("llama-server", &dirs.join(sep), &path_exts(&lookup), is_file).unwrap_or_default()
}

struct Running {
    guard: ProcessGuard,
    url: String,
    key: String,
}

/// Der eigene `llama-server`, nur wenn keine Adresse eines schon laufenden Servers eingestellt ist. Endet mit Kadrell
/// (`stop` beim Beenden; unter Windows zusätzlich durch das Job Object, auch nach einem Absturz).
pub struct LlamaServer {
    running: Mutex<Option<Running>>,
    /// Datei mit der pid des eigenen Servers, damit ein Start nach einem Absturz ihn wegräumt (nur Unix).
    pid_file: Option<std::path::PathBuf>,
}

const START_POLLS: u32 = 60;
const POLL: Duration = Duration::from_millis(250);

impl LlamaServer {
    pub fn new(pid_file: Option<std::path::PathBuf>) -> LlamaServer {
        LlamaServer { running: Mutex::new(None), pid_file }
    }

    pub fn is_running(&self) -> bool {
        self.running.lock().unwrap_or_else(|e| e.into_inner()).as_mut().is_some_and(|r| r.guard.is_running())
    }

    /// Adresse, wenn er läuft.
    pub fn url(&self) -> Option<String> {
        let mut running = self.running.lock().unwrap_or_else(|e| e.into_inner());
        let r = running.as_mut()?;
        r.guard.is_running().then(|| r.url.clone())
    }

    /// Läuft schon einer mit denselben Einstellungen, gilt der. Sonst neu starten und warten (höchstens 15 s), bis
    /// `/health` antwortet. Blockiert: nie auf dem UI-Thread rufen.
    pub fn ensure(&self, binary: &str, models_dir: &str, env: &BTreeMap<String, String>, home: &str, http: &dyn Http) -> Result<String, ModelError> {
        let key = format!("{binary}\n{models_dir}");
        let mut running = self.running.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(r) = running.as_mut()
            && r.key == key
            && r.guard.is_running()
        {
            return Ok(r.url.clone());
        }
        *running = None; // Drop beendet den alten Prozess
        if binary.is_empty() || !std::path::Path::new(binary).is_file() {
            return Err(ModelError::Invalid(kadrell_i18n::trd!("%@: llama-server nicht gefunden", binary)));
        }
        let port = free_port().unwrap_or(8091);
        let guard = ProcessGuard::spawn(binary, &server_args(port, models_dir, home), env).map_err(|e| ModelError::Invalid(e.to_string()))?;
        let url = format!("http://127.0.0.1:{port}");
        let mut started = Running { guard, url: url.clone(), key };
        for _ in 0..START_POLLS {
            if (LlamaClient { base: url.clone(), http }).healthy() {
                self.write_pid(started.guard.pid());
                *running = Some(started);
                return Ok(url);
            }
            if !started.guard.is_running() {
                break;
            }
            std::thread::sleep(POLL);
        }
        Err(ModelError::Invalid(kadrell_i18n::trd!("llama-server startet nicht")))
    }

    pub fn stop(&self) {
        *self.running.lock().unwrap_or_else(|e| e.into_inner()) = None;
        if let Some(file) = &self.pid_file {
            let _ = std::fs::remove_file(file);
        }
    }

    fn write_pid(&self, pid: u32) {
        if let Some(file) = &self.pid_file {
            let _ = std::fs::write(file, pid.to_string());
        }
    }

    /// Server eines früheren, abgestürzten Laufs desselben Profils beenden, aber nur, wenn die pid noch ein
    /// `llama-server` ist (pids werden wiederverwendet). Unter Windows nichts zu tun: das Job Object hat ihn mit
    /// Kadrell beendet.
    pub fn sweep_stale(&self) {
        let Some(file) = &self.pid_file else { return };
        #[cfg(unix)]
        if let Some(pid) = std::fs::read_to_string(file).ok().and_then(|s| s.trim().parse::<u32>().ok()).filter(|p| *p > 0) {
            crate::guard::terminate_if_named(pid, "llama-server");
        }
        let _ = std::fs::remove_file(file);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::fake::Fake;

    const BASE: &str = "http://127.0.0.1:9999";

    fn client(fake: &Fake) -> LlamaClient<'_> {
        LlamaClient { base: BASE.into(), http: fake }
    }

    #[test]
    fn router_models_and_delete() {
        let fake = Fake::new(|r| {
            if r.method == Method::Delete {
                assert!(r.url.contains("/models?model=ggml-org%2Fgemma-3-1b-it-GGUF%3AQ4_K_M"), "{}", r.url);
                return Ok((200, "{\"success\":true}".into()));
            }
            Ok((200, r#"{"object":"list","data":[
              {"id":"ggml-org/gemma-3-1b-it-GGUF:Q4_K_M","status":{"value":"unloaded"},"can_remove":true},
              {"id":"local-model","status":{"value":"loaded","args":["llama-server"]},"can_remove":false},
              {"id":"neu/modell:Q4","status":{"value":"downloading","progress":{"https://a":{"done":50,"total":200}}}}]}"#.into()))
        });
        let c = client(&fake);
        let models = c.models().unwrap();
        assert_eq!(models.iter().map(|m| m.name.as_str()).collect::<Vec<_>>(), ["ggml-org/gemma-3-1b-it-GGUF:Q4_K_M", "local-model", "neu/modell:Q4"]);
        assert_eq!(models.iter().map(|m| m.can_remove).collect::<Vec<_>>(), [true, false, false]);
        assert!(models[2].downloading);
        assert_eq!(models.iter().filter(|m| !m.downloading).count(), 2, "laufender Download ist noch nicht benutzbar");
        assert_eq!(models[1].detail, "loaded");
        c.delete("ggml-org/gemma-3-1b-it-GGUF:Q4_K_M").unwrap();
    }

    #[test]
    fn chat_uses_schema_and_no_thinking() {
        let fake = Fake::new(|r| {
            assert!(r.url.ends_with("/v1/chat/completions"));
            Ok((200, "{\"choices\":[{\"message\":{\"content\":\"{\\\"index\\\":0}\"}}]}".into()))
        });
        let schema = kadrell_claude::quick_start::schema();
        let reply = client(&fake).chat("m", "s", "u", Some(&schema), Duration::from_secs(5)).unwrap();
        assert_eq!(reply, "{\"index\":0}");
        let body = fake.body_json(0);
        assert_eq!(body["model"], "m");
        assert_eq!(body["chat_template_kwargs"]["enable_thinking"], false);
        assert_eq!(body["response_format"]["type"], "json_schema");
    }

    #[test]
    fn health_and_load_ignore_errors() {
        let fake = Fake::new(|r| Ok((if r.url.ends_with("/health") { 503 } else { 400 }, "{\"error\":{\"message\":\"already loaded\"}}".into())));
        assert!(!client(&fake).healthy());
        client(&fake).load("m");
        assert_eq!(fake.requests().len(), 2);
    }

    #[test]
    fn server_arguments() {
        let args = server_args(5555, "", "/h");
        assert_eq!(args, ["--host", "127.0.0.1", "--port", "5555", "--no-webui", "--offline", "--models-max", "1"]);
        let with_dir = server_args(5555, "~/gguf", "/h");
        assert_eq!(&with_dir[with_dir.len() - 2..], ["--models-dir", "/h/gguf"]);
    }

    #[test]
    fn free_port_is_usable() {
        let p = free_port().unwrap();
        assert!(p > 1024);
        assert!(TcpListener::bind(("127.0.0.1", p)).is_ok());
    }

    #[test]
    fn autodetect_finds_binary_in_path_or_nothing() {
        let env: BTreeMap<String, String> = [("PATH".to_string(), "/x/bin".to_string())].into();
        let exe = if cfg!(windows) { "llama-server.exe" } else { "llama-server" };
        let wanted = paths::join("/x/bin", exe);
        let wanted2 = wanted.clone();
        let is_file = move |p: &str| p == wanted2;
        assert_eq!(autodetect(&env, "/h", &is_file), wanted);
        assert_eq!(autodetect(&env, "/h", &|_| false), "");
        let home_bin = paths::join("/h/.local/bin", exe);
        let home2 = home_bin.clone();
        assert_eq!(autodetect(&env, "/h", &move |p| p == home2), home_bin);
    }

    #[test]
    fn missing_binary_is_reported_and_nothing_runs() {
        let server = LlamaServer::new(None);
        let fake = Fake::ok("");
        let err = server.ensure("/gibt/es/nicht/llama-server", "", &BTreeMap::new(), "/h", &fake).unwrap_err();
        assert!(matches!(err, ModelError::Invalid(m) if m.contains("llama-server nicht gefunden")));
        assert!(!server.is_running());
        assert!(server.url().is_none());
        assert!(fake.requests().is_empty());
        server.stop();
    }
}
