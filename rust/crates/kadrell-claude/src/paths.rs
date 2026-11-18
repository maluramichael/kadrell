//! Pfad-Helfer ohne Plattformannahmen. Die Swift-Quellen rechnen mit `/`; unter Windows kommen `\` und `C:` dazu.
//! Intern gilt `/` als Trenner, `canonicalize` wird nie benutzt (es hängt auf Windows `\\?\` an).

/// Absolut: führendes `/` oder `\`, oder Laufwerk `X:` mit Trenner.
pub fn is_abs(p: &str) -> bool {
    let b = p.as_bytes();
    p.starts_with('/')
        || p.starts_with('\\')
        || (b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b[2] == b'/' || b[2] == b'\\'))
}

pub fn fwd(p: &str) -> String {
    p.replace('\\', "/")
}

/// `dir` + `name` mit `/`, ein schon vorhandener Trenner am Ende wird nicht verdoppelt.
pub fn join(dir: &str, name: &str) -> String {
    if dir.ends_with('/') || dir.ends_with('\\') { format!("{dir}{name}") } else { format!("{dir}/{name}") }
}

/// Letzter Pfadbestandteil, wie `lastPathComponent` (Trenner am Ende zählen nicht).
pub fn last_component(p: &str) -> &str {
    let t = p.trim_end_matches(['/', '\\']);
    if t.is_empty() {
        return if p.is_empty() { "" } else { "/" };
    }
    t.rsplit(['/', '\\']).next().unwrap_or(t)
}

/// Ordner darüber, wie `deletingLastPathComponent`. Für `/x` ist das `/`, für `x` leer.
pub fn parent(p: &str) -> String {
    let t = p.trim_end_matches(['/', '\\']);
    match t.rfind(['/', '\\']) {
        Some(0) => "/".into(),
        Some(i) => t[..i].to_string(),
        None => String::new(),
    }
}

/// `.` und `..` lexikalisch auflösen, alles mit `/` (wie `standardizingPath`, ohne Symlinks aufzulösen).
pub fn standardize(p: &str) -> String {
    let p = fwd(p);
    let (prefix, rest) = split_root(&p);
    let mut out: Vec<&str> = Vec::new();
    for seg in rest.split('/') {
        match seg {
            "" | "." => {}
            ".." if out.last().is_some_and(|l| *l != "..") => {
                out.pop();
            }
            ".." if !prefix.is_empty() => {}
            s => out.push(s),
        }
    }
    let joined = out.join("/");
    if prefix.is_empty() && joined.is_empty() {
        return ".".into();
    }
    format!("{prefix}{joined}")
}

/// Wurzel (`/`, `C:/` oder `C:`) und Rest eines Pfads mit `/`; ohne Wurzel ist der erste Teil leer.
pub fn split_root(p: &str) -> (&str, &str) {
    let b = p.as_bytes();
    if b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':' {
        let n = if b.len() >= 3 && b[2] == b'/' { 3 } else { 2 };
        return (&p[..n], &p[n..]);
    }
    match p.strip_prefix('/') {
        Some(rest) => ("/", rest),
        None => ("", p),
    }
}

/// `~` und `~/…` auf `home` abbilden.
pub fn expand_tilde(p: &str, home: &str) -> String {
    if p == "~" {
        home.to_string()
    } else if let Some(rest) = p.strip_prefix("~/") {
        format!("{home}/{rest}")
    } else {
        p.to_string()
    }
}

/// Persönlicher Ordner aus der Umgebung (`HOME`, sonst `USERPROFILE`), leer wenn keiner da ist.
pub fn home_dir() -> String {
    std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")).map(|h| fwd(&h)).unwrap_or_default()
}

/// `~/…` statt des Home-Pfads, wie `Theme.shortPath`.
pub fn short_path(path: &str, home: &str) -> String {
    match path.strip_prefix(home) {
        Some(rest) if !home.is_empty() && (rest.is_empty() || rest.starts_with('/') || rest.starts_with('\\')) => format!("~{rest}"),
        _ => path.to_string(),
    }
}

#[cfg(test)]
pub mod testdir {
    use std::path::PathBuf;

    /// Wegwerf-Ordner unter dem Temp-Verzeichnis, wird beim Drop gelöscht.
    pub struct TempDir(pub PathBuf);

    impl TempDir {
        pub fn new(tag: &str) -> TempDir {
            let p = std::env::temp_dir().join(format!("kadrell-claude-{tag}-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&p).unwrap();
            TempDir(p)
        }
        /// Pfad mit `/`, wie ihn der Code intern verwendet.
        pub fn path(&self) -> String {
            super::fwd(&self.0.to_string_lossy())
        }
        pub fn sub(&self, rel: &str) -> String {
            format!("{}/{rel}", self.path())
        }
        pub fn mkdir(&self, rel: &str) -> String {
            let p = self.sub(rel);
            std::fs::create_dir_all(&p).unwrap();
            p
        }
        pub fn write(&self, rel: &str, content: &str) -> String {
            let p = self.sub(rel);
            if let Some(dir) = std::path::Path::new(&p).parent() {
                std::fs::create_dir_all(dir).unwrap();
            }
            std::fs::write(&p, content).unwrap();
            p
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helpers() {
        assert!(is_abs("/x") && is_abs("C:\\x") && is_abs("c:/x") && !is_abs("x/y") && !is_abs("~/x"));
        assert_eq!(last_component("/p/kadrell/"), "kadrell");
        assert_eq!(last_component("C:\\p\\a"), "a");
        assert_eq!(parent("/x"), "/");
        assert_eq!(parent("/a/b"), "/a");
        assert_eq!(standardize("/a/./b/../c"), "/a/c");
        assert_eq!(standardize("C:\\a\\..\\b"), "C:/b");
        assert_eq!(standardize("../x"), "../x");
        assert_eq!(join("/a/", "b"), "/a/b");
        assert_eq!(short_path("/h/me/x", "/h/me"), "~/x");
        assert_eq!(short_path("/h/meer", "/h/me"), "/h/meer");
    }
}
