//! Ziel eines Klicks auf einen Link im Terminal (Swift: `Services/LinkTarget.swift`). Relative Pfade gelten ab dem
//! Ordner der Session, wie in iTerm2.

use crate::paths;
use std::io::Read;
use std::path::Path;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LinkTarget {
    Url(String),
    Path { path: String, is_directory: bool },
}

/// `^([A-Za-z][A-Za-z0-9+.-]*://|mailto:|tel:|file:)`
fn is_url(link: &str) -> bool {
    if ["mailto:", "tel:", "file:"].iter().any(|p| link.starts_with(p)) {
        return true;
    }
    let mut chars = link.char_indices();
    if !chars.next().is_some_and(|(_, c)| c.is_ascii_alphabetic()) {
        return false;
    }
    let scheme_len = link.find(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '-'))).unwrap_or(link.len());
    link[scheme_len..].starts_with("://")
}

/// `:12` oder `:12:4` am Ende abschneiden, `None` wenn keine Position dran hängt.
fn without_position(link: &str) -> Option<&str> {
    let (rest, last) = link.rsplit_once(':')?;
    if last.is_empty() || !last.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if let Some((front, mid)) = rest.rsplit_once(':')
        && !mid.is_empty()
        && mid.bytes().all(|b| b.is_ascii_digit())
    {
        return Some(front);
    }
    Some(rest)
}

fn absolute(path: &str, base: &str, home: &str) -> String {
    let p = paths::expand_tilde(path, home);
    if paths::is_abs(&p) { paths::standardize(&p) } else { paths::standardize(&paths::join(base, &p)) }
}

/// `None` = es gibt nichts zu öffnen (Datei existiert nicht): der Klick tut dann nichts.
pub fn resolve(link: &str, base: &str, home: &str) -> Option<LinkTarget> {
    if is_url(link) {
        return Some(LinkTarget::Url(link.to_string()));
    }
    // `datei.swift:12` und `datei.swift:12:4`: erst der ganze Text (eine Datei kann so heißen), dann ohne Position.
    let candidates = std::iter::once(link).chain(without_position(link));
    candidates.map(|c| absolute(c, base, home)).find(|full| Path::new(full).exists()).map(|full| {
        let is_directory = Path::new(&full).is_dir();
        LinkTarget::Path { path: full, is_directory }
    })
}

const IMAGE_EXTENSIONS: [&str; 17] =
    ["png", "jpg", "jpeg", "gif", "svg", "webp", "heic", "heif", "tif", "tiff", "bmp", "ico", "icns", "avif", "jxl", "psd", "raw"];

/// Eingestellter Editor nur für Ordner und Textdateien. Bilder, PDFs, Archive und sonstige Binärdateien öffnet die
/// Standard-App des Systems, ein Editor zeigt dort nur Zeichensalat. Nicht lesbar = Editor.
pub fn prefers_editor(path: &str, is_directory: bool) -> bool {
    if is_directory {
        return true;
    }
    let ext = path.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    if IMAGE_EXTENSIONS.contains(&ext.as_str()) {
        return false;
    }
    let Ok(file) = std::fs::File::open(path) else { return true };
    let mut head = Vec::new();
    let _ = file.take(8192).read_to_end(&mut head);
    !head.contains(&0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::testdir::TempDir;

    fn root() -> TempDir {
        let dir = TempDir::new("link");
        dir.write("docs/specs/a.md", "x");
        dir
    }

    fn file(path: String) -> Option<LinkTarget> {
        Some(LinkTarget::Path { path, is_directory: false })
    }

    #[test]
    fn relative_path_goes_from_session_folder() {
        let d = root();
        assert_eq!(resolve("docs/specs/a.md", &d.path(), ""), file(d.sub("docs/specs/a.md")));
        assert_eq!(resolve("./docs/../docs/specs", &d.path(), ""), Some(LinkTarget::Path { path: d.sub("docs/specs"), is_directory: true }));
    }

    #[test]
    fn position_suffix_is_dropped_only_when_file_has_no_such_name() {
        let d = root();
        assert_eq!(resolve("docs/specs/a.md:12:4", &d.path(), ""), file(d.sub("docs/specs/a.md")));
        assert_eq!(resolve("docs/specs/a.md:12", &d.path(), ""), file(d.sub("docs/specs/a.md")));
        assert_eq!(resolve("docs/specs/a.md:", &d.path(), ""), None, "Doppelpunkt ohne Zahl ist keine Position");
    }

    #[test]
    fn absolute_and_home_paths() {
        let d = root();
        assert_eq!(resolve(&d.sub("docs/specs/a.md"), "/nowhere", ""), file(d.sub("docs/specs/a.md")));
        assert_eq!(resolve("~/docs/specs/a.md", "/nowhere", &d.path()), file(d.sub("docs/specs/a.md")));
    }

    #[test]
    fn missing_file_opens_nothing() {
        let d = root();
        assert_eq!(resolve("docs/gibt-es-nicht.md", &d.path(), ""), None);
    }

    #[test]
    fn urls_stay_urls() {
        assert_eq!(resolve("https://kadrell.malura.de/x", "/b", ""), Some(LinkTarget::Url("https://kadrell.malura.de/x".into())));
        assert_eq!(resolve("mailto:a@b.de", "/b", ""), Some(LinkTarget::Url("mailto:a@b.de".into())));
        assert!(!is_url("C:/x/y"), "Laufwerk ist kein Schema");
        assert!(is_url("file:///tmp/x") && is_url("tel:123") && !is_url("1http://x"));
    }

    /// Bilder und Binärdateien gehören der Standard-App, Text und Ordner dem Editor.
    #[test]
    fn prefers_editor_only_for_text_and_folders() {
        let d = TempDir::new("editor");
        let write = |name: &str, bytes: &[u8]| {
            let p = d.sub(name);
            std::fs::write(&p, bytes).unwrap();
            p
        };
        assert!(prefers_editor(&d.path(), true));
        assert!(prefers_editor(&write("a.swift", b"let x = 1\n"), false));
        assert!(prefers_editor(&write("Makefile", b"all:\n"), false));
        assert!(prefers_editor(&write("a.ts", b"const x = 1\n"), false), "TypeScript ist kein Video");
        assert!(!prefers_editor(&write("a.png", &[0x89, 0x50, 0x4E, 0x47, 0, 0, 0]), false));
        assert!(!prefers_editor(&write("a.svg", b"<svg/>"), false), "Bild, auch wenn Text");
        assert!(!prefers_editor(&write("a.pdf", &[0x25, 0x50, 0x44, 0x46, 0, 1]), false));
        assert!(!prefers_editor(&write("noext", &[1, 2, 0, 3]), false));
        assert!(prefers_editor(&d.sub("fehlt.txt"), false));
    }
}
