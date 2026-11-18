//! `kadrell.json` einer Extension und der Katalog unter `~/.config/kadrell/extensions`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::i18n::{tr, tr_args};
use crate::json::as_int;

/// Höchste `apiVersion`, die diese Kadrell-Version versteht.
pub const SUPPORTED_API: i64 = 1;

/// Eine Wahl bei `choice`: `{ "value": …, "label": … }` oder nur der Wert als Text (dann ist er auch das Label).
#[derive(Debug, Clone, PartialEq)]
pub struct SettingOption {
    pub value: String,
    pub label: String,
}

/// Eine Einstellung aus dem Manifest. `kind` ist `string`, `secret`, `bool`, `number` oder `choice`; `secret` liegt
/// im Schlüsselbund, nie in einer Datei. Ein unbekannter Typ gilt als `string`.
#[derive(Debug, Clone, PartialEq)]
pub struct Setting {
    pub key: String,
    pub kind: String,
    pub label: String,
    pub default: Option<Value>,
    /// Nur bei `choice`: die Auswahl.
    pub options: Option<Vec<SettingOption>>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ExtensionManifest {
    pub name: String,
    pub version: String,
    pub description: String,
    pub api_version: i64,
    pub permissions: Vec<String>,
    pub settings: Vec<Setting>,
}

impl ExtensionManifest {
    /// Nur `name` und `apiVersion` sind Pflicht; ein Manifest ohne Beschreibung oder Einstellungen ist gültig.
    pub fn parse(data: &[u8]) -> Option<ExtensionManifest> {
        let v: Value = serde_json::from_slice(data).ok()?;
        v.as_object()?;
        Some(ExtensionManifest {
            name: v.get("name")?.as_str()?.to_string(),
            api_version: as_int(v.get("apiVersion")?)?,
            version: optional_text(&v, "version")?,
            description: optional_text(&v, "description")?,
            permissions: optional_list(&v, "permissions", |p| p.as_str().map(String::from))?,
            settings: optional_list(&v, "settings", parse_setting)?,
        })
    }
}

fn optional_text(v: &Value, key: &str) -> Option<String> {
    match v.get(key) {
        None | Some(Value::Null) => Some(String::new()),
        Some(Value::String(s)) => Some(s.clone()),
        Some(_) => None,
    }
}

/// Fehlt der Schlüssel, ist die Liste leer; ein falscher Typ oder ein kaputtes Element macht das Manifest ungültig.
fn optional_list<T>(v: &Value, key: &str, item: impl Fn(&Value) -> Option<T>) -> Option<Vec<T>> {
    match v.get(key) {
        None | Some(Value::Null) => Some(Vec::new()),
        Some(Value::Array(a)) => a.iter().map(item).collect(),
        Some(_) => None,
    }
}

fn parse_setting(v: &Value) -> Option<Setting> {
    Some(Setting {
        key: v.get("key")?.as_str()?.to_string(),
        kind: v.get("type")?.as_str()?.to_string(),
        label: v.get("label")?.as_str()?.to_string(),
        default: v.get("default").filter(|d| !d.is_null()).cloned(),
        options: match v.get("options") {
            None | Some(Value::Null) => None,
            Some(o) => Some(o.as_array()?.iter().map(parse_option).collect::<Option<Vec<_>>>()?),
        },
    })
}

fn parse_option(v: &Value) -> Option<SettingOption> {
    if let Some(plain) = v.as_str() {
        return Some(SettingOption { value: plain.to_string(), label: plain.to_string() });
    }
    let value = v.get("value")?.as_str()?.to_string();
    let label = match v.get("label") {
        None | Some(Value::Null) => value.clone(),
        Some(l) => l.as_str()?.to_string(),
    };
    Some(SettingOption { value, label })
}

/// Ein Ordner unter `~/.config/kadrell/extensions`; `problem` ist ein Klartext für den Dialog, dann lädt sie nicht.
#[derive(Debug, Clone, PartialEq)]
pub struct FoundExtension {
    pub name: String,
    pub dir: PathBuf,
    pub manifest: Option<ExtensionManifest>,
    pub problem: Option<String>,
}

/// Home-Verzeichnis des Nutzers (`HOME`, unter Windows `USERPROFILE`).
pub fn home_dir() -> PathBuf {
    let var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    std::env::var_os(var).or_else(|| std::env::var_os("HOME")).map(PathBuf::from).unwrap_or_default()
}

/// `~/.config/kadrell/extensions`, ersetzt durch `--extensions-dir <ordner>` oder `KADRELL_EXTENSIONS_DIR`.
pub fn default_dir() -> PathBuf {
    let args: Vec<String> = std::env::args().collect();
    let env: HashMap<String, String> = std::env::vars().collect();
    custom_dir(&args, &env).unwrap_or_else(|| home_dir().join(".config").join("kadrell").join("extensions"))
}

/// Das Argument geht vor der Umgebung; leer heißt: kein eigener Ordner. `~` wird aufgelöst.
pub fn custom_dir(arguments: &[String], environment: &HashMap<String, String>) -> Option<PathBuf> {
    let raw = match arguments.iter().position(|a| a == "--extensions-dir") {
        Some(i) if i + 1 < arguments.len() => Some(arguments[i + 1].clone()),
        _ => environment.get("KADRELL_EXTENSIONS_DIR").cloned(),
    };
    let path = raw?.trim().to_string();
    if path.is_empty() {
        return None;
    }
    Some(standardized(&expand_tilde(&path)))
}

fn expand_tilde(path: &str) -> PathBuf {
    if path == "~" {
        return home_dir();
    }
    match path.strip_prefix("~/").or_else(|| path.strip_prefix("~\\")) {
        Some(rest) => home_dir().join(rest),
        None => PathBuf::from(path),
    }
}

/// Wie `URL.standardizedFileURL`: ohne abschließenden Schrägstrich, ohne `.` und `..`.
fn standardized(path: &Path) -> PathBuf {
    let abs = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let mut out = PathBuf::new();
    for c in abs.components() {
        match c {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Alle Ordner unter `dir` ohne versteckte, nach Name sortiert, jeder mit Manifest und Befund.
pub fn scan(dir: &Path) -> Vec<FoundExtension> {
    let Ok(read) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut names: Vec<String> = read
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| !n.starts_with('.'))
        .collect();
    names.sort();
    names
        .into_iter()
        .filter_map(|name| {
            let path = dir.join(&name);
            path.is_dir().then(|| inspect(name, path))
        })
        .collect()
}

fn inspect(name: String, path: PathBuf) -> FoundExtension {
    let manifest = std::fs::read(path.join("kadrell.json")).ok().and_then(|d| ExtensionManifest::parse(&d));
    let problem = problem(&name, &path, manifest.as_ref());
    FoundExtension { name, dir: path, manifest, problem }
}

fn problem(name: &str, path: &Path, manifest: Option<&ExtensionManifest>) -> Option<String> {
    let Some(manifest) = manifest else {
        return Some(tr("kadrell.json fehlt oder ist kaputt", "kadrell.json is missing or broken"));
    };
    if manifest.name != name {
        return Some(tr_args(
            "Name im Manifest („{0}“) passt nicht zum Ordnernamen",
            "Name in the manifest (“{0}”) does not match the folder name",
            &[&manifest.name],
        ));
    }
    if manifest.api_version > SUPPORTED_API {
        let (api, supported) = (manifest.api_version.to_string(), SUPPORTED_API.to_string());
        return Some(tr_args(
            "Braucht neueres Kadrell (API {0}, unterstützt: {1})",
            "Needs a newer Kadrell (API {0}, supported: {1})",
            &[&api, &supported],
        ));
    }
    if !path.join("init.lua").exists() {
        return Some(tr("init.lua fehlt", "init.lua is missing"));
    }
    untrusted_file(path)
}

/// Wie bei Shell-Hooks: Ordner und Dateien müssen dir gehören und dürfen für andere nicht beschreibbar sein. Jede
/// Lua-Datei, denn `require` lädt auch die aus Unterordnern.
fn untrusted_file(path: &Path) -> Option<String> {
    let mut all = vec![path.to_path_buf(), path.join("kadrell.json")];
    all.extend(lua_files(path));
    let bad = all.into_iter().find(|p| !trusted(p))?;
    let file = bad.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    Some(tr_args(
        "{0} gehört nicht dir oder ist für andere beschreibbar",
        "{0} is not owned by you or is writable by others",
        &[&file],
    ))
}

/// Gehört der Pfad dem Nutzer und ist er für Gruppe und andere nicht beschreibbar? Nur unter Unix prüfbar, unter
/// Windows regeln die Zugriffslisten des Profilordners das schon; dort gilt jeder Pfad als vertrauenswürdig.
#[cfg(unix)]
pub fn trusted(path: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    // SAFETY: getuid hat keine Vorbedingungen und kann nicht fehlschlagen.
    let uid = unsafe { libc::getuid() };
    std::fs::metadata(path).map(|m| m.uid() == uid && m.mode() & 0o022 == 0).unwrap_or(false)
}

#[cfg(not(unix))]
pub fn trusted(path: &Path) -> bool {
    path.exists()
}

/// Alle Lua-Dateien einer Extension, auch in Unterordnern, ohne versteckte.
pub fn lua_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    collect_lua(dir, &mut out);
    out.sort();
    out
}

fn collect_lua(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(read) = std::fs::read_dir(dir) else { return };
    for entry in read.flatten() {
        if entry.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            collect_lua(&path, out);
        } else if path.extension().is_some_and(|e| e == "lua") {
            out.push(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempDir;

    fn make(root: &Path, folder: &str, name: Option<&str>, api: i64, with_init: bool) -> PathBuf {
        let dir = root.join(folder);
        std::fs::create_dir_all(&dir).unwrap();
        let manifest = serde_json::json!({"name": name.unwrap_or(folder), "version": "0.1.0", "apiVersion": api});
        std::fs::write(dir.join("kadrell.json"), manifest.to_string()).unwrap();
        if with_init {
            std::fs::write(dir.join("init.lua"), "-- leer").unwrap();
        }
        dir
    }

    #[test]
    fn scan_finds_valid_and_reports_problems() {
        let root = TempDir::new("catalog");
        make(root.path(), "ok", None, 1, true);
        make(root.path(), "nameMismatch", Some("anders"), 1, true);
        make(root.path(), "apiTooHigh", None, 2, true);
        make(root.path(), "noInit", None, 1, false);
        let found = scan(root.path());
        let names: Vec<_> = found.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["apiTooHigh", "nameMismatch", "noInit", "ok"], "nach Name sortiert");
        for ext in &found {
            if ext.name == "ok" {
                assert!(ext.problem.is_none(), "{:?}", ext.problem);
                assert_eq!(ext.manifest.as_ref().unwrap().version, "0.1.0");
            } else {
                assert!(ext.problem.is_some(), "{}", ext.name);
            }
        }
    }

    #[cfg(unix)]
    #[test]
    fn group_writable_folder_is_untrusted() {
        use std::os::unix::fs::PermissionsExt;
        let root = TempDir::new("catalog");
        let dir = make(root.path(), "groupWritable", None, 1, true);
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o775)).unwrap();
        assert!(scan(root.path())[0].problem.is_some());
    }

    #[test]
    fn scan_reports_broken_manifest_and_missing_dir() {
        let root = TempDir::new("catalog");
        let dir = make(root.path(), "kaputt", None, 1, true);
        std::fs::write(dir.join("kadrell.json"), "{").unwrap();
        assert!(scan(root.path())[0].problem.is_some());
        assert!(scan(&root.path().join("gibt-es-nicht")).is_empty());
    }

    #[test]
    fn scan_skips_dot_folders() {
        let root = TempDir::new("catalog");
        make(root.path(), ".versteckt", None, 1, true);
        make(root.path(), "sichtbar", None, 1, true);
        std::fs::write(root.path().join("datei.txt"), "x").unwrap();
        let names: Vec<_> = scan(root.path()).into_iter().map(|f| f.name).collect();
        assert_eq!(names, ["sichtbar"]);
    }

    #[test]
    fn manifest_settings_and_defaults() {
        let json = r#"{"name":"j","apiVersion":1,"permissions":["exec"],"settings":[{"key":"k","type":"bool","label":"L","default":true}]}"#;
        let m = ExtensionManifest::parse(json.as_bytes()).unwrap();
        assert_eq!(
            m.settings,
            vec![Setting { key: "k".into(), kind: "bool".into(), label: "L".into(), default: Some(Value::Bool(true)), options: None }]
        );
        assert_eq!(m.description, "");
        assert_eq!(m.permissions, ["exec"]);
        assert!(ExtensionManifest::parse(br#"{"apiVersion":1}"#).is_none(), "name ist Pflicht");
        assert!(ExtensionManifest::parse(br#"{"name":"x"}"#).is_none(), "apiVersion ist Pflicht");
    }

    #[test]
    fn choice_and_number_settings_decode() {
        let data = br#"{"name":"x","apiVersion":1,"settings":[
          {"key":"cpu","type":"choice","label":"CPU","default":"compact","options":[{"value":"compact","label":"Kompakt"},"detailed"]},
          {"key":"every","type":"number","label":"Sekunden","default":5}]}"#;
        let m = ExtensionManifest::parse(data).unwrap();
        assert_eq!(
            m.settings[0].options,
            Some(vec![
                SettingOption { value: "compact".into(), label: "Kompakt".into() },
                SettingOption { value: "detailed".into(), label: "detailed".into() },
            ])
        );
        assert_eq!(m.settings[1].default, Some(serde_json::json!(5)));
        assert!(m.settings[1].options.is_none());
    }

    #[test]
    fn custom_extensions_dir() {
        let none: HashMap<String, String> = HashMap::new();
        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        assert_eq!(custom_dir(&s(&["Kadrell", "--profile", "tmp"]), &none), None);
        let abs = if cfg!(windows) { "C:/tmp/ext/" } else { "/tmp/ext/" };
        let want = if cfg!(windows) { PathBuf::from("C:\\tmp\\ext") } else { PathBuf::from("/tmp/ext") };
        assert_eq!(custom_dir(&s(&["Kadrell", "--extensions-dir", abs]), &none), Some(want));
        let env: HashMap<_, _> = [("KADRELL_EXTENSIONS_DIR".to_string(), "~/x".to_string())].into();
        assert_eq!(custom_dir(&[], &env), Some(standardized(&home_dir().join("x"))));
        let both: HashMap<_, _> = [("KADRELL_EXTENSIONS_DIR".to_string(), "/b".to_string())].into();
        assert_eq!(
            custom_dir(&s(&["--extensions-dir", "/a"]), &both),
            Some(standardized(Path::new("/a"))),
            "das Argument geht vor"
        );
        let blank: HashMap<_, _> = [("KADRELL_EXTENSIONS_DIR".to_string(), "  ".to_string())].into();
        assert_eq!(custom_dir(&[], &blank), None);
    }

    /// Jede Lua-Datei der Extension wird geprüft, nicht nur init.lua.
    #[cfg(unix)]
    #[test]
    fn group_writable_lua_in_subfolder_is_untrusted() {
        use std::os::unix::fs::PermissionsExt;
        let root = TempDir::new("catalog");
        let sub = make(root.path(), "lib", None, 1, true).join("lib");
        std::fs::create_dir_all(&sub).unwrap();
        let file = sub.join("x.lua");
        std::fs::write(&file, "return 1").unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o664)).unwrap();
        let problem = scan(root.path())[0].problem.clone().unwrap();
        assert!(problem.contains("x.lua"), "{problem}");
    }

    #[test]
    fn lua_files_include_subfolders_but_not_hidden() {
        let root = TempDir::new("lua");
        std::fs::create_dir_all(root.path().join("sub")).unwrap();
        std::fs::create_dir_all(root.path().join(".git")).unwrap();
        for f in ["a.lua", "sub/b.lua", ".git/c.lua", "d.txt"] {
            std::fs::write(root.path().join(f), "").unwrap();
        }
        let names: Vec<_> = lua_files(root.path()).iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect();
        assert_eq!(names, ["a.lua", "b.lua"]);
    }
}
