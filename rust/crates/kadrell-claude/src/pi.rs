//! Pi (`pi`, earendil-works) als zweiter Agent neben Claude Code, mit Pis eigenem Standardmodell oder einem lokalen
//! Modell (Swift: `Services/Pi/PiCLI.swift`, `Services/Pi/kadrell.ts`, Teile von `Model/Session.swift`).
//!
//! Ein lokales Modell gibt Kadrell samt Adresse über die Umgebung mit; die eigene Pi-Extension (`kadrell.ts`, siehe
//! `install_extension`) macht daraus den Provider „kadrell“ und meldet den Status der Kachel per `kadrell status`, so wie
//! die Hooks bei Claude Code. Die Konversation hängt an `--session-id`: Pi legt sie beim ersten Start an und setzt sie
//! danach fort.

use crate::paths;
use std::collections::BTreeMap;

/// Name des Providers, den `kadrell.ts` registriert.
pub const PROVIDER: &str = "kadrell";

/// Quelltext der Pi-Extension, mit dem Crate ausgeliefert (`assets/kadrell.ts`, Kopie aus der Mac-App).
pub const EXTENSION_SOURCE: &str = include_str!("../assets/kadrell.ts");

/// Kopie von `LocalProviderKind` (Ollama oder llama.cpp).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LocalProviderKind {
    Ollama,
    LlamaCpp,
}

impl LocalProviderKind {
    pub fn raw(self) -> &'static str {
        match self {
            LocalProviderKind::Ollama => "ollama",
            LocalProviderKind::LlamaCpp => "llamacpp",
        }
    }
}

/// Verweis auf ein Modell eines Providers, gespeichert als `provider|name` (Ollama-Namen enthalten `:` und `/`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct LocalModelRef {
    pub provider: LocalProviderKind,
    pub name: String,
}

impl LocalModelRef {
    pub fn parse(s: &str) -> Option<LocalModelRef> {
        let (provider, name) = s.split_once('|')?;
        let provider = match provider {
            "ollama" => LocalProviderKind::Ollama,
            "llamacpp" => LocalProviderKind::LlamaCpp,
            _ => return None,
        };
        (!name.is_empty()).then(|| LocalModelRef { provider, name: name.to_string() })
    }

    pub fn string(&self) -> String {
        format!("{}|{}", self.provider.raw(), self.name)
    }
}

/// Womit eine Pi-Session startet: mit Pis eigenem Standard oder einem lokalen Modell.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PiModel {
    PiDefault,
    Local(LocalModelRef),
}

/// Wert in den Einstellungen für „Standard von Pi“; lokale Modelle stehen dort als `provider|name`.
pub const PI_DEFAULT_SETTING: &str = "pi";

impl PiModel {
    pub fn from_setting(setting: &str) -> Option<PiModel> {
        if setting == PI_DEFAULT_SETTING { Some(PiModel::PiDefault) } else { LocalModelRef::parse(setting).map(PiModel::Local) }
    }

    pub fn local(&self) -> Option<&LocalModelRef> {
        match self {
            PiModel::Local(r) => Some(r),
            PiModel::PiDefault => None,
        }
    }
}

/// Schlüssel einer Pi-Session: `pi-` plus die `--session-id` von Pi, damit ein Neustart dieselbe Konversation fortsetzt.
pub const PI_PREFIX: &str = "pi-";

pub fn new_pi_id() -> String {
    format!("{PI_PREFIX}{}", uuid::Uuid::new_v4())
}

pub fn is_pi(id: &str) -> bool {
    id.starts_with(PI_PREFIX)
}

pub fn pi_session_id(id: &str) -> &str {
    id.strip_prefix(PI_PREFIX).unwrap_or(id)
}

fn path_sep() -> char {
    if cfg!(windows) { ';' } else { ':' }
}

/// `pi` im PATH der Login-Shell, sonst unter `~/.local/bin` (Standard des Pi-Installers). nil = Pi fehlt.
/// `is_file` prüft, ob es die Datei gibt und sie ausführbar ist; unter Windows probiert es zusätzlich `.exe`/`.cmd`.
pub fn binary(env: &BTreeMap<String, String>, home: &str, is_file: &dyn Fn(&str) -> bool) -> Option<String> {
    let mut dirs: Vec<String> = env.get("PATH").map(|p| p.split(path_sep()).filter(|d| !d.is_empty()).map(String::from).collect()).unwrap_or_default();
    dirs.push(format!("{home}/.local/bin"));
    let exts: Vec<&str> = if cfg!(windows) { vec!["", ".exe", ".cmd"] } else { vec![""] };
    dirs.iter().flat_map(|d| exts.iter().map(move |e| format!("{}{e}", paths::join(d, "pi")))).find(|c| is_file(c))
}

/// PATH für die Pi-Kachel mit dem Ordner von `pi` vorne, falls er fehlt. Aus Dock oder Finder gestartet hat die
/// Login-Shell `~/.local/bin` oft nicht im PATH; `binary` findet `pi` dort trotzdem, `#!/usr/bin/env node` aber nicht den
/// `node` daneben (Exit 127). Auch Pis Bash-Werkzeug sieht dann dieselben Programme wie im Terminal.
pub fn path_for(binary: &str, path: Option<&str>) -> String {
    path_for_with(binary, path, path_sep())
}

fn path_for_with(binary: &str, path: Option<&str>, sep: char) -> String {
    let dir = paths::parent(binary);
    match path.filter(|p| !p.is_empty()) {
        None => dir,
        Some(p) if p.split(sep).any(|d| d == dir) => p.to_string(),
        Some(p) => format!("{dir}{sep}{p}"),
    }
}

/// Aufruf und zusätzliche Umgebung für eine Pi-Kachel.
#[derive(Clone, Debug, PartialEq)]
pub struct PiLaunch {
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
}

/// Ohne `model` nimmt Pi sein eigenes Standardmodell. `base_url` ist die Adresse des Providers ohne `/v1`; fehlt sie
/// (llama.cpp ließ sich nicht starten), startet Pi trotzdem und zeigt den Fehler selbst.
pub fn launch(pi_session_id: &str, model: Option<&LocalModelRef>, base_url: Option<&str>, prompt: Option<&str>) -> PiLaunch {
    let mut args: Vec<String> = model.map(|m| vec!["--provider".into(), PROVIDER.into(), "--model".into(), m.name.clone()]).unwrap_or_default();
    args.extend(["--session-id".to_string(), pi_session_id.to_string()]);
    if let Some(p) = prompt.filter(|p| !p.is_empty()) {
        args.push(safe_prompt(p));
    }
    let mut env = BTreeMap::from([("KADRELL_PI".to_string(), "1".to_string())]);
    if let Some(m) = model {
        env.insert("KADRELL_PI_MODEL".into(), m.name.clone());
        if let Some(url) = base_url {
            env.insert("KADRELL_PI_BASE_URL".into(), format!("{}/v1", url.trim_end_matches('/')));
        }
    }
    PiLaunch { args, env }
}

/// Pis eigenes Standardmodell als `provider/modell` aus `~/.pi/agent/settings.json`, nil = nicht eingestellt.
pub fn default_model(home: &str) -> Option<String> {
    let text = std::fs::read_to_string(format!("{home}/.pi/agent/settings.json")).ok()?;
    let json: serde_json::Value = serde_json::from_str(&text).ok()?;
    let model = json.get("defaultModel")?.as_str().filter(|m| !m.is_empty())?;
    Some(match json.get("defaultProvider").and_then(|p| p.as_str()) {
        Some(p) => format!("{p}/{model}"),
        None => model.to_string(),
    })
}

/// Pi liest ein Argument mit `-` als Option und eines mit `@` als Datei: ein Leerzeichen davor macht es zum Prompt.
pub fn safe_prompt(p: &str) -> String {
    if p.starts_with('-') || p.starts_with('@') { format!(" {p}") } else { p.to_string() }
}

/// Legt `~/.pi/agent/extensions/kadrell.ts` an oder bringt sie auf den Stand dieses Kadrell. Vor jedem Start einer
/// Pi-Session, nicht beim App-Start: wer Pi nie aus Kadrell startet, bekommt nichts in `~/.pi`. true = liegt aktuell da.
pub fn install_extension(source: &str, home: &str) -> bool {
    let dir = format!("{home}/.pi/agent/extensions");
    let file = format!("{dir}/kadrell.ts");
    if std::fs::read_to_string(&file).is_ok_and(|current| current == source) {
        return true;
    }
    std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(&file, source)).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::testdir::TempDir;

    fn model() -> LocalModelRef {
        LocalModelRef { provider: LocalProviderKind::Ollama, name: "qwen3.6:35b-a3b-q4_K_M".into() }
    }

    #[test]
    fn pi_id_carries_pi_session_id_and_model_ref_roundtrips() {
        let id = new_pi_id();
        assert!(is_pi(&id));
        assert!(!is_pi("shell-1"));
        assert_eq!(pi_session_id(&id).len(), 36, "hinter dem Präfix steht die --session-id von Pi");
        assert_eq!(LocalModelRef::parse(&model().string()), Some(model()));
        assert_eq!(LocalModelRef::parse("ollama|"), None);
        assert_eq!(LocalModelRef::parse("x|y"), None);
        assert_eq!(PiModel::from_setting("pi"), Some(PiModel::PiDefault));
        assert_eq!(PiModel::from_setting("ollama|m").and_then(|m| m.local().cloned()).map(|m| m.name), Some("m".to_string()));
        assert_eq!(PiModel::from_setting(""), None);
    }

    #[test]
    fn launch_passes_model_address_and_prompt() {
        let l = launch("sid", Some(&model()), Some("http://127.0.0.1:11434"), Some("Idee"));
        assert_eq!(l.args, ["--provider", "kadrell", "--model", "qwen3.6:35b-a3b-q4_K_M", "--session-id", "sid", "Idee"]);
        assert_eq!(l.env["KADRELL_PI"], "1");
        assert_eq!(l.env["KADRELL_PI_MODEL"], "qwen3.6:35b-a3b-q4_K_M");
        assert_eq!(l.env["KADRELL_PI_BASE_URL"], "http://127.0.0.1:11434/v1");
    }

    #[test]
    fn launch_without_address_or_prompt() {
        let l = launch("sid", Some(&model()), None, None);
        assert_eq!(l.args.len(), 6, "ohne Prompt kein leeres Argument");
        assert!(!l.env.contains_key("KADRELL_PI_BASE_URL"));
    }

    /// Ohne Modell nimmt Pi seinen eigenen Standard (z. B. Olla): kein Provider „kadrell“, keine Adresse.
    #[test]
    fn launch_without_model_uses_pi_default() {
        let l = launch("sid", None, Some("http://127.0.0.1:11434"), Some("Idee"));
        assert_eq!(l.args, ["--session-id", "sid", "Idee"]);
        assert_eq!(l.env, BTreeMap::from([("KADRELL_PI".to_string(), "1".to_string())]));
    }

    #[test]
    fn default_model_from_pi_settings() {
        let home = TempDir::new("pi");
        assert_eq!(default_model(&home.path()), None, "ohne settings.json kein Standard");
        home.write(".pi/agent/settings.json", r#"{"defaultProvider":"olla","defaultModel":"qwen3.8:27b"}"#);
        assert_eq!(default_model(&home.path()).as_deref(), Some("olla/qwen3.8:27b"));
        home.write(".pi/agent/settings.json", r#"{"defaultModel":"qwen3.8:27b"}"#);
        assert_eq!(default_model(&home.path()).as_deref(), Some("qwen3.8:27b"));
    }

    /// Aus Dock oder Finder gestartet fehlt `~/.local/bin` oft im PATH: der Ordner von `pi` gehört vorne in den PATH.
    #[test]
    fn path_gets_binary_folder_once() {
        assert_eq!(path_for_with("/Users/x/.local/bin/pi", Some("/usr/bin:/bin"), ':'), "/Users/x/.local/bin:/usr/bin:/bin");
        assert_eq!(path_for_with("/Users/x/.local/bin/pi", Some("/usr/bin:/Users/x/.local/bin"), ':'), "/usr/bin:/Users/x/.local/bin", "schon im PATH: Reihenfolge bleibt");
        assert_eq!(path_for_with("/opt/pi/bin/pi", None, ':'), "/opt/pi/bin");
        assert_eq!(path_for_with("C:/pi/pi.exe", Some("C:/x"), ';'), "C:/pi;C:/x");
    }

    #[test]
    fn prompt_is_never_read_as_option_or_file() {
        assert_eq!(safe_prompt("--help mir"), " --help mir");
        assert_eq!(safe_prompt("@datei.md lesen"), " @datei.md lesen");
        assert_eq!(safe_prompt("normale Idee"), "normale Idee");
    }

    #[test]
    fn binary_from_path_or_local_bin() {
        let home = TempDir::new("pibin");
        let env: BTreeMap<String, String> = [("PATH".to_string(), "/nirgends".to_string())].into();
        let is_file = |p: &str| std::path::Path::new(p).is_file();
        assert_eq!(binary(&env, &home.path(), &is_file), None);
        let exe = if cfg!(windows) { "pi.exe" } else { "pi" };
        let file = home.write(&format!(".local/bin/{exe}"), "#!/bin/sh\n");
        assert_eq!(binary(&env, &home.path(), &is_file), Some(file));
    }

    #[test]
    fn extension_ships_with_the_crate_and_installs_once() {
        assert!(EXTENSION_SOURCE.contains("KADRELL_PI"));
        let home = TempDir::new("piext");
        let file = format!("{}/.pi/agent/extensions/kadrell.ts", home.path());
        assert!(install_extension("alt", &home.path()));
        assert!(install_extension(EXTENSION_SOURCE, &home.path()));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), EXTENSION_SOURCE, "veraltete Fassung wird ersetzt");
        let written = std::fs::metadata(&file).unwrap().modified().unwrap();
        assert!(install_extension(EXTENSION_SOURCE, &home.path()));
        assert_eq!(std::fs::metadata(&file).unwrap().modified().unwrap(), written, "gleiche Fassung bleibt liegen");
    }
}
