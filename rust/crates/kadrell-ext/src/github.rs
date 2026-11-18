//! Der "Marktplatz": GitHub nach Repos mit Topic `kadrell-extension` durchsuchen und eins in den Katalog laden. Kein
//! eigener Server, nur die öffentliche Such-API und der Tarball des Repos. Installiert wird in denselben Ordner wie von
//! Hand gelegte Extensions, der Beobachter lässt sie danach erscheinen. Das Netz steckt hinter `HttpClient`.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use flate2::read::GzDecoder;
use serde_json::Value;
use tar::EntryType;

use crate::http::{HttpClient, HttpError, HttpRequest};
use crate::i18n::{tr, tr_args};

pub const TOPIC: &str = "kadrell-extension";
/// Obergrenze für den heruntergeladenen Tarball: eine Extension ist klein, ein riesiges Repo würde sonst den Speicher
/// des Einzelprozesses erschöpfen (alle Sessions hängen daran).
pub const MAX_DOWNLOAD_BYTES: u64 = 25 * 1024 * 1024;
/// Obergrenze für das Entpackte, gegen Archive, die auf ein Vielfaches anwachsen.
pub const MAX_UNPACKED_BYTES: u64 = 200 * 1024 * 1024;

/// Ein Treffer der Marktplatz-Suche.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteExtension {
    pub owner: String,
    pub repo: String,
    pub description: String,
    pub stars: u64,
    pub html_url: String,
}

impl RemoteExtension {
    pub fn full_name(&self) -> String {
        format!("{}/{}", self.owner, self.repo)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallError {
    Download,
    Extract,
    Empty,
    TooLarge,
    Occupied(String),
    /// Netzfehler mit dem Text des Clients.
    Network(String),
}

impl std::fmt::Display for InstallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let text = match self {
            Self::Download => tr(
                "Download von GitHub fehlgeschlagen",
                "Download from GitHub failed",
            ),
            Self::Extract => tr(
                "Archiv ließ sich nicht entpacken",
                "Could not extract the archive",
            ),
            Self::Empty => tr(
                "Repo enthält keine Extension (kadrell.json fehlt)",
                "Repo contains no extension (kadrell.json missing)",
            ),
            Self::TooLarge => {
                let mb = (MAX_DOWNLOAD_BYTES / 1024 / 1024).to_string();
                tr_args(
                    "Repo ist zu groß (über {0} MB)",
                    "Repo is too large (over {0} MB)",
                    &[&mb],
                )
            }
            Self::Occupied(name) => tr_args(
                "Name {0} ist schon von einer anderen Extension belegt, erst entfernen",
                "Name {0} is already taken by another extension, remove it first",
                &[name],
            ),
            Self::Network(m) => m.clone(),
        };
        f.write_str(&text)
    }
}

impl std::error::Error for InstallError {}

fn percent_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~:/@".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Such-URL: immer der Topic, dazu die freien Wörter des Nutzers, nach Sternen sortiert.
pub fn search_url(query: &str) -> String {
    let mut terms = vec![format!("topic:{TOPIC}")];
    terms.extend(query.split(' ').filter(|t| !t.is_empty()).map(String::from));
    format!(
        "https://api.github.com/search/repositories?q={}&sort=stars&per_page=20",
        percent_encode(&terms.join(" "))
    )
}

/// Antwort der Such-API in Treffer übersetzen; Einträge ohne gültige URL fallen weg, Müll ergibt eine leere Liste.
pub fn parse(data: &[u8]) -> Vec<RemoteExtension> {
    let Ok(root) = serde_json::from_slice::<Value>(data) else {
        return Vec::new();
    };
    let Some(items) = root.get("items").and_then(Value::as_array) else {
        return Vec::new();
    };
    items.iter().filter_map(parse_item).collect()
}

fn parse_item(item: &Value) -> Option<RemoteExtension> {
    let url = item.get("html_url")?.as_str()?;
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return None;
    }
    Some(RemoteExtension {
        owner: item.get("owner")?.get("login")?.as_str()?.to_string(),
        repo: item.get("name")?.as_str()?.to_string(),
        description: item
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        stars: item.get("stargazers_count")?.as_u64()?,
        html_url: url.to_string(),
    })
}

/// Der Marktplatz-Client; das Netz kommt von außen.
#[derive(Clone)]
pub struct GitHubExtensions {
    client: Arc<dyn HttpClient>,
}

impl GitHubExtensions {
    pub fn new(client: Arc<dyn HttpClient>) -> GitHubExtensions {
        GitHubExtensions { client }
    }

    fn request(url: &str) -> HttpRequest {
        let mut r = HttpRequest::get(url);
        r.headers
            .push(("Accept".into(), "application/vnd.github+json".into()));
        // Ohne User-Agent antwortet die GitHub-API mit 403.
        r.headers.push(("User-Agent".into(), "Kadrell".into()));
        r
    }

    /// Sucht Repos mit dem Topic. Eine Antwort ungleich 200 (Rate-Limit) ist ein Fehler, nicht "keine Treffer".
    pub fn search(&self, query: &str) -> Result<Vec<RemoteExtension>, InstallError> {
        let resp = self
            .client
            .send(Self::request(&search_url(query)))
            .map_err(|e| InstallError::Network(e.to_string()))?;
        if resp.status != 200 {
            return Err(InstallError::Download);
        }
        Ok(parse(&resp.body))
    }

    /// Lädt den Tarball des Repos (Standard-Branch), entpackt ihn und legt die Extension unter `<catalog_dir>/<name>` ab.
    /// `known_origins` (Name zu `owner/repo`) verhindert, dass ein fremdes Repo einen belegten Namen überschreibt.
    /// Liefert den Ordnernamen.
    pub fn install(
        &self,
        remote: &RemoteExtension,
        catalog_dir: &Path,
        known_origins: &HashMap<String, String>,
    ) -> Result<String, InstallError> {
        let url = format!(
            "https://api.github.com/repos/{}/{}/tarball",
            remote.owner, remote.repo
        );
        let data = self.download(&url)?;
        install_tarball(
            &data,
            &remote.repo,
            catalog_dir,
            &remote.full_name(),
            known_origins,
        )
    }

    /// Lädt mit harter Byte-Obergrenze.
    pub fn download(&self, url: &str) -> Result<Vec<u8>, InstallError> {
        let mut req = Self::request(url);
        req.max_body = MAX_DOWNLOAD_BYTES;
        req.timeout = std::time::Duration::from_secs(120);
        let resp = self.client.send(req).map_err(|e| match e {
            HttpError::TooLarge => InstallError::TooLarge,
            other => InstallError::Network(other.to_string()),
        })?;
        if resp.status != 200 {
            return Err(InstallError::Download);
        }
        if resp.body.len() as u64 > MAX_DOWNLOAD_BYTES {
            return Err(InstallError::TooLarge);
        }
        Ok(resp.body)
    }
}

/// Entpackt einen heruntergeladenen `tar.gz` und legt die Extension in den Katalog. Getrennt von `install`, damit die
/// Ablage ohne Netz prüfbar ist. Der Repo-Ordner im Archiv wird abgeschnitten (wie `--strip-components 1`); nur normale
/// Dateien und Ordner werden angelegt, Verweise und Pfade nach außen übersprungen.
pub fn install_tarball(
    data: &[u8],
    fallback_name: &str,
    catalog_dir: &Path,
    origin: &str,
    known_origins: &HashMap<String, String>,
) -> Result<String, InstallError> {
    std::fs::create_dir_all(catalog_dir).map_err(|_| InstallError::Extract)?;
    // Im Katalog selbst (versteckt), damit das Verschieben am Ende auf demselben Laufwerk bleibt.
    let tmp = catalog_dir.join(format!(
        ".kadrell-install-{}-{}",
        std::process::id(),
        unique()
    ));
    let dest = tmp.join("x");
    std::fs::create_dir_all(&dest).map_err(|_| InstallError::Extract)?;
    let result = place(
        data,
        &dest,
        fallback_name,
        catalog_dir,
        origin,
        known_origins,
    );
    let _ = std::fs::remove_dir_all(&tmp);
    result
}

fn unique() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}

fn place(
    data: &[u8],
    dest: &Path,
    fallback_name: &str,
    catalog_dir: &Path,
    origin: &str,
    known_origins: &HashMap<String, String>,
) -> Result<String, InstallError> {
    unpack(data, dest)?;
    if !dest.join("kadrell.json").is_file() {
        return Err(InstallError::Empty);
    }
    let name = safe_name(&manifest_name(dest).unwrap_or_else(|| fallback_name.to_string()));
    if name.is_empty() {
        return Err(InstallError::Empty);
    }
    // Wie bei Shell-Hooks verlangt die Trust-Prüfung: nicht für Gruppe oder andere beschreibbar.
    harden(dest);
    let target = catalog_dir.join(&name);
    if target.exists() {
        // Nur das Repo, das den Namen vorher installiert hat, darf ihn aktualisieren.
        if known_origins.get(&name).map(String::as_str) != Some(origin) {
            return Err(InstallError::Occupied(name));
        }
        std::fs::remove_dir_all(&target).map_err(|_| InstallError::Extract)?;
    }
    std::fs::rename(dest, &target).map_err(|_| InstallError::Extract)?;
    Ok(name)
}

/// Der Pfad im Archiv ohne den obersten Ordner; `None` für alles, was nicht sicher unter `dest` landet.
fn stripped(path: &Path) -> Option<PathBuf> {
    let mut parts = path.components();
    parts.next()?;
    let mut out = PathBuf::new();
    for c in parts {
        match c {
            Component::Normal(p) => out.push(p),
            Component::CurDir => {}
            _ => return None,
        }
    }
    (!out.as_os_str().is_empty()).then_some(out)
}

fn unpack(data: &[u8], dest: &Path) -> Result<(), InstallError> {
    let mut archive = tar::Archive::new(GzDecoder::new(data));
    let mut total = 0u64;
    for entry in archive.entries().map_err(|_| InstallError::Extract)? {
        let mut entry = entry.map_err(|_| InstallError::Extract)?;
        let kind = entry.header().entry_type();
        if !matches!(kind, EntryType::Regular | EntryType::Directory) {
            continue;
        }
        let Some(rel) = entry.path().ok().and_then(|p| stripped(&p)) else {
            continue;
        };
        let out = dest.join(rel);
        if kind == EntryType::Directory {
            std::fs::create_dir_all(&out).map_err(|_| InstallError::Extract)?;
            continue;
        }
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).map_err(|_| InstallError::Extract)?;
        }
        let mut limited = (&mut entry).take(MAX_UNPACKED_BYTES.saturating_sub(total) + 1);
        let mut file = std::fs::File::create(&out).map_err(|_| InstallError::Extract)?;
        total += std::io::copy(&mut limited, &mut file).map_err(|_| InstallError::Extract)?;
        if total > MAX_UNPACKED_BYTES {
            return Err(InstallError::TooLarge);
        }
    }
    Ok(())
}

/// `chmod -R go-w` unter Unix, unter Windows gibt es nichts zu tun.
fn harden(dir: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(dir) {
            let mode = meta.permissions().mode() & !0o022;
            let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(mode));
        }
        if let Ok(read) = std::fs::read_dir(dir) {
            for e in read.flatten() {
                if e.path().is_dir() {
                    harden(&e.path());
                } else if let Ok(meta) = e.metadata() {
                    let mode = meta.permissions().mode() & !0o022;
                    let _ =
                        std::fs::set_permissions(e.path(), std::fs::Permissions::from_mode(mode));
                }
            }
        }
    }
    #[cfg(not(unix))]
    let _ = dir;
}

fn manifest_name(dir: &Path) -> Option<String> {
    let data = std::fs::read(dir.join("kadrell.json")).ok()?;
    let v: Value = serde_json::from_slice(&data).ok()?;
    let name = v.get("name")?.as_str()?;
    (!name.is_empty()).then(|| name.to_string())
}

/// Nur harmlose ASCII-Zeichen und keine führenden Punkte: ein Name mit "/" oder ".." darf nicht aus dem Katalog
/// ausbrechen, und Unicode-Homoglyphen sollen keinen fremden Namen nachahmen.
pub fn safe_name(s: &str) -> String {
    let kept: String = s
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || "._-".contains(*c))
        .collect();
    kept.trim_start_matches('.').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::HttpResponse;
    use crate::manifest;
    use crate::testutil::TempDir;
    use std::sync::Mutex;

    fn decode(s: &str) -> String {
        let bytes = s.as_bytes();
        let mut out = Vec::new();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'%' && i + 2 < bytes.len() {
                out.push(u8::from_str_radix(&s[i + 1..i + 3], 16).unwrap());
                i += 3;
            } else {
                out.push(bytes[i]);
                i += 1;
            }
        }
        String::from_utf8(out).unwrap()
    }

    fn param(url: &str, key: &str) -> Option<String> {
        let query = url.split_once('?')?.1;
        query
            .split('&')
            .find_map(|kv| kv.strip_prefix(&format!("{key}=")).map(decode))
    }

    #[test]
    fn search_url_carries_topic_and_terms() {
        let url = search_url("jira ticket");
        assert_eq!(
            param(&url, "q").as_deref(),
            Some("topic:kadrell-extension jira ticket")
        );
        assert_eq!(param(&url, "sort").as_deref(), Some("stars"));
        assert_eq!(param(&url, "per_page").as_deref(), Some("20"));
        assert!(url.starts_with("https://api.github.com/search/repositories?"));
    }

    #[test]
    fn parse_reads_items() {
        let json = br#"{"items":[{"name":"jira","html_url":"https://github.com/me/jira","description":"Jira panel","stargazers_count":42,"owner":{"login":"me"}}]}"#;
        assert_eq!(
            parse(json),
            vec![RemoteExtension {
                owner: "me".into(),
                repo: "jira".into(),
                description: "Jira panel".into(),
                stars: 42,
                html_url: "https://github.com/me/jira".into()
            }]
        );
    }

    #[test]
    fn parse_tolerates_garbage() {
        assert!(parse(b"nonsense").is_empty());
        assert!(parse(br#"{"items":[{"name":"x"}]}"#).is_empty());
    }

    /// Baut ein `tar.gz` wie GitHub: ein oberster Ordner `<repo>-main/` mit `kadrell.json` und `init.lua`. `ext_name` None
    /// lässt das Manifest weg, "" schreibt es ohne name-Feld.
    fn make_tarball(repo: &str, ext_name: Option<&str>, version: &str) -> Vec<u8> {
        let gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        let mut builder = tar::Builder::new(gz);
        let top = format!("{repo}-main");
        let mut add = |path: String, body: &[u8], dir: bool| {
            let mut h = tar::Header::new_gnu();
            h.set_path(&path).unwrap();
            h.set_size(if dir { 0 } else { body.len() as u64 });
            h.set_mode(0o777);
            h.set_entry_type(if dir {
                EntryType::Directory
            } else {
                EntryType::Regular
            });
            h.set_cksum();
            builder.append(&h, body).unwrap();
        };
        add(format!("{top}/"), b"", true);
        if let Some(name) = ext_name {
            let mut manifest = serde_json::json!({"version": version, "apiVersion": 1});
            if !name.is_empty() {
                manifest["name"] = Value::from(name);
            }
            add(
                format!("{top}/kadrell.json"),
                manifest.to_string().as_bytes(),
                false,
            );
        }
        add(format!("{top}/init.lua"), b"-- demo", false);
        builder.into_inner().unwrap().finish().unwrap()
    }

    fn origins(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn install_tarball_places_extension_under_manifest_name() {
        let root = TempDir::new("store");
        let catalog = root.path().join("catalog");
        let data = make_tarball("my-repo", Some("demo"), "1.0.0");
        let name = install_tarball(&data, "my-repo", &catalog, "", &HashMap::new()).unwrap();
        assert_eq!(
            name, "demo",
            "der Name aus dem Manifest bestimmt den Ordner, nicht der Repo-Name"
        );
        let found = manifest::scan(&catalog);
        assert_eq!(
            found.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(),
            ["demo"]
        );
        assert!(found[0].problem.is_none(), "{:?}", found[0].problem);
        let hidden: Vec<_> = std::fs::read_dir(&catalog)
            .unwrap()
            .flatten()
            .map(|e| e.file_name())
            .collect();
        assert_eq!(hidden.len(), 1, "keine Reste des Entpackens: {hidden:?}");
    }

    #[test]
    fn install_tarball_falls_back_to_repo_name_without_manifest_name() {
        let root = TempDir::new("store");
        let data = make_tarball("repo", Some(""), "1.0.0");
        let name =
            install_tarball(&data, "repo", &root.path().join("c"), "", &HashMap::new()).unwrap();
        assert_eq!(name, "repo");
    }

    #[test]
    fn install_tarball_rejects_repo_without_manifest() {
        let root = TempDir::new("store");
        let data = make_tarball("repo", None, "1.0.0");
        let err = install_tarball(&data, "repo", &root.path().join("c"), "", &HashMap::new())
            .unwrap_err();
        assert_eq!(
            err,
            InstallError::Empty,
            "ohne kadrell.json darf nichts installiert werden"
        );
    }

    #[test]
    fn install_tarball_overwrites_on_update_from_same_origin() {
        let root = TempDir::new("store");
        let catalog = root.path().join("catalog");
        install_tarball(
            &make_tarball("r", Some("demo"), "1.0.0"),
            "r",
            &catalog,
            "a/x",
            &HashMap::new(),
        )
        .unwrap();
        install_tarball(
            &make_tarball("r", Some("demo"), "2.0.0"),
            "r",
            &catalog,
            "a/x",
            &origins(&[("demo", "a/x")]),
        )
        .unwrap();
        assert_eq!(
            manifest::scan(&catalog)[0]
                .manifest
                .as_ref()
                .unwrap()
                .version,
            "2.0.0"
        );
    }

    /// Ein fremdes Repo darf einen schon belegten Namen nicht überschreiben.
    #[test]
    fn install_tarball_refuses_hijack_from_different_origin() {
        let root = TempDir::new("store");
        let catalog = root.path().join("catalog");
        install_tarball(
            &make_tarball("jira", Some("jira"), "1.0.0"),
            "jira",
            &catalog,
            "a/jira",
            &HashMap::new(),
        )
        .unwrap();
        let err = install_tarball(
            &make_tarball("jira-fork", Some("jira"), "9.9.9"),
            "jira-fork",
            &catalog,
            "b/jira-fork",
            &origins(&[("jira", "a/jira")]),
        )
        .unwrap_err();
        assert!(
            matches!(err, InstallError::Occupied(_)),
            "fremde Herkunft darf den Namen nicht kapern"
        );
        assert_eq!(
            manifest::scan(&catalog)[0]
                .manifest
                .as_ref()
                .unwrap()
                .version,
            "1.0.0",
            "das Original bleibt unangetastet"
        );
    }

    /// Ein Manifest-Name mit Pfadtrennern bleibt im Katalog (safe_name), bricht nicht aus.
    #[test]
    fn install_tarball_sanitizes_traversal_name() {
        let root = TempDir::new("store");
        let catalog = root.path().join("catalog");
        let name = install_tarball(
            &make_tarball("r", Some("../../evil"), "1.0.0"),
            "r",
            &catalog,
            "",
            &HashMap::new(),
        )
        .unwrap();
        assert_eq!(name, "evil");
        assert!(catalog.join("evil").exists());
        assert!(
            !root.path().join("evil").exists(),
            "nichts außerhalb des Katalogs"
        );
    }

    /// Nicht-ASCII-Zeichen (Homoglyphen) fallen aus dem Ordnernamen.
    #[test]
    fn install_tarball_strips_non_ascii_from_name() {
        let root = TempDir::new("store");
        let name = install_tarball(
            &make_tarball("r", Some("café-x"), "1.0.0"),
            "r",
            &root.path().join("catalog"),
            "",
            &HashMap::new(),
        )
        .unwrap();
        assert_eq!(name, "caf-x");
    }

    /// Ein Name nur aus Punkten wird nach safe_name leer und abgelehnt.
    #[test]
    fn install_tarball_rejects_dot_only_name() {
        let root = TempDir::new("store");
        let err = install_tarball(
            &make_tarball("r", Some(".."), "1.0.0"),
            "",
            &root.path().join("c"),
            "",
            &HashMap::new(),
        )
        .unwrap_err();
        assert_eq!(
            err,
            InstallError::Empty,
            "leerer Name nach safe_name muss abgelehnt werden"
        );
    }

    #[test]
    fn install_tarball_ignores_links_and_escaping_paths() {
        let gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        let mut builder = tar::Builder::new(gz);
        let mut file = |path: &str, kind: EntryType, link: Option<&str>, body: &[u8]| {
            let mut h = tar::Header::new_gnu();
            h.set_path(path).unwrap();
            h.set_size(body.len() as u64);
            h.set_mode(0o644);
            h.set_entry_type(kind);
            if let Some(l) = link {
                h.set_link_name(l).unwrap();
            }
            h.set_cksum();
            builder.append(&h, body).unwrap();
        };
        file(
            "top/kadrell.json",
            EntryType::Regular,
            None,
            br#"{"name":"x","apiVersion":1}"#,
        );
        file("top/init.lua", EntryType::Regular, None, b"-- x");
        file("top/link", EntryType::Symlink, Some("/etc"), b"");
        file("top/hard", EntryType::Link, Some("top/init.lua"), b"");
        let data = builder.into_inner().unwrap().finish().unwrap();
        let root = TempDir::new("store");
        let catalog = root.path().join("catalog");
        install_tarball(&data, "x", &catalog, "", &HashMap::new()).unwrap();
        assert!(catalog.join("x/init.lua").exists());
        assert!(!catalog.join("x/link").exists() && !catalog.join("x/hard").exists());
    }

    #[test]
    fn unpack_rejects_non_archives() {
        let root = TempDir::new("store");
        let err = install_tarball(
            b"das ist kein Tarball",
            "x",
            &root.path().join("c"),
            "",
            &HashMap::new(),
        )
        .unwrap_err();
        assert_eq!(err, InstallError::Extract);
    }

    struct FakeHttp {
        status: u16,
        body: Vec<u8>,
        seen: Mutex<Vec<HttpRequest>>,
        fail: Option<HttpError>,
    }

    impl HttpClient for FakeHttp {
        fn send(&self, request: HttpRequest) -> Result<HttpResponse, HttpError> {
            self.seen.lock().unwrap().push(request);
            match &self.fail {
                Some(e) => Err(e.clone()),
                None => Ok(HttpResponse {
                    status: self.status,
                    headers: vec![],
                    body: self.body.clone(),
                }),
            }
        }
    }

    fn fake(status: u16, body: &[u8]) -> Arc<FakeHttp> {
        Arc::new(FakeHttp {
            status,
            body: body.to_vec(),
            seen: Mutex::new(vec![]),
            fail: None,
        })
    }

    #[test]
    fn search_sends_user_agent_and_reports_rate_limit() {
        let ok = fake(200, br#"{"items":[]}"#);
        assert!(
            GitHubExtensions::new(ok.clone())
                .search("x")
                .unwrap()
                .is_empty()
        );
        let req = ok.seen.lock().unwrap()[0].clone();
        assert!(
            req.headers
                .iter()
                .any(|(k, v)| k == "User-Agent" && v == "Kadrell")
        );
        assert!(
            req.headers
                .iter()
                .any(|(k, v)| k == "Accept" && v == "application/vnd.github+json")
        );
        let limited = fake(403, b"{}");
        assert_eq!(
            GitHubExtensions::new(limited).search("x").unwrap_err(),
            InstallError::Download,
            "403 ist ein Fehler, nicht 'keine Treffer'"
        );
    }

    #[test]
    fn install_downloads_and_places() {
        let root = TempDir::new("store");
        let client = fake(200, &make_tarball("jira", Some("jira"), "1.0.0"));
        let gh = GitHubExtensions::new(client.clone());
        let remote = RemoteExtension {
            owner: "me".into(),
            repo: "jira".into(),
            description: String::new(),
            stars: 1,
            html_url: "https://github.com/me/jira".into(),
        };
        let name = gh
            .install(&remote, &root.path().join("catalog"), &HashMap::new())
            .unwrap();
        assert_eq!(name, "jira");
        let req = client.seen.lock().unwrap()[0].clone();
        assert_eq!(req.url, "https://api.github.com/repos/me/jira/tarball");
        assert_eq!(req.max_body, MAX_DOWNLOAD_BYTES);
    }

    #[test]
    fn download_maps_size_and_status_errors() {
        let big = Arc::new(FakeHttp {
            status: 200,
            body: vec![],
            seen: Mutex::new(vec![]),
            fail: Some(HttpError::TooLarge),
        });
        assert_eq!(
            GitHubExtensions::new(big)
                .download("https://x")
                .unwrap_err(),
            InstallError::TooLarge
        );
        assert_eq!(
            GitHubExtensions::new(fake(404, b""))
                .download("https://x")
                .unwrap_err(),
            InstallError::Download
        );
        assert_eq!(
            GitHubExtensions::new(fake(200, b""))
                .download("https://x")
                .unwrap(),
            Vec::<u8>::new()
        );
    }
}
