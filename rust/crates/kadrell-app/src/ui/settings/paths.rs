//! Pfadfelder der Einstellungen (`pathField` in `SettingsLayout.swift`): Tab schreibt das Kürzel aus (`~/d/p/kad`),
//! eine abgelegte Datei oder ein Ordner setzt den Pfad. Reine Funktionen mit austauschbarem Dateisystem, testbar ohne
//! Fenster. Die Felder selbst sind gewöhnliche `TextInput`s, die Zeilen-Id sagt, was ein Feld annimmt.

use std::path::{Path, PathBuf};

use crate::ui::folders;
use crate::ui::groups::files::folder_for;

/// Was ein Pfadfeld annimmt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathKind {
    /// Projektordner: ein Ordner, bei einer Datei ihr Ordner (`FolderIndex.folder(for:)`).
    Folder,
    /// Hintergrundbild: nur Dateien, die sich als Bild laden lassen.
    Image,
}

/// Art des Pfadfelds der Zeile mit dieser Id, `None` für gewöhnliche Textfelder.
pub fn kind_of(row: &str) -> Option<PathKind> {
    match row {
        "startFolder" => Some(PathKind::Folder),
        "backgroundImage" => Some(PathKind::Image),
        _ => None,
    }
}

/// Endungen, die GPUI als Bild laden kann (`NSImage(contentsOf:)` der Mac-App).
const IMAGE_EXTENSIONS: [&str; 9] = ["png", "jpg", "jpeg", "gif", "webp", "bmp", "tif", "tiff", "svg"];

/// Ist der Pfad eine vorhandene Bilddatei?
pub fn is_image(path: &Path) -> bool {
    let ext = path.extension().and_then(|e| e.to_str()).map(str::to_lowercase);
    path.is_file() && ext.is_some_and(|e| IMAGE_EXTENSIONS.contains(&e.as_str()))
}

fn slashed(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    text.strip_prefix("//?/").unwrap_or(&text).to_string()
}

/// Text fürs Feld nach einem Ablegen: der erste passende Pfad, mit `~` gekürzt. `None`, wenn nichts passt.
pub fn dropped(kind: PathKind, paths: &[PathBuf], home: &str) -> Option<String> {
    let full = match kind {
        PathKind::Folder => paths.iter().find_map(|p| folder_for(p)),
        PathKind::Image => paths.iter().find(|p| is_image(p)).map(|p| slashed(p)),
    }?;
    Some(folders::short_path(&full, home))
}

/// Tab: erster Treffer des Kürzels, ausgeschrieben und mit `~` gekürzt (`FolderIndex.expandAbbreviated(...).first`).
pub fn tab_expansion(typed: &str, home: &str, dirs: &dyn Fn(&str) -> Vec<String>) -> Option<String> {
    let first = folders::expand_abbreviated(typed, home, 1, dirs).into_iter().next()?;
    Some(folders::short_path(&folders::normalize(&first, home), home))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(dir: &str) -> Vec<String> {
        let t: &[(&str, &[&str])] = &[("/h", &["development", "Documents"]), ("/h/development", &["projects"]), ("/h/development/projects", &["kadrell", "homelab"])];
        t.iter().find(|(d, _)| *d == dir).map(|(_, n)| n.iter().map(|s| s.to_string()).collect()).unwrap_or_default()
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kadrell-paths-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn only_start_folder_and_background_image_are_path_fields() {
        assert_eq!(kind_of("startFolder"), Some(PathKind::Folder));
        assert_eq!(kind_of("backgroundImage"), Some(PathKind::Image));
        assert_eq!(kind_of("editorCommand"), None);
        assert_eq!(kind_of("llamaBinary"), None);
    }

    #[test]
    fn tab_writes_the_abbreviation_out_and_shortens_home() {
        assert_eq!(tab_expansion("~/d/p/kad", "/h", &tree).as_deref(), Some("~/development/projects/kadrell"));
        assert_eq!(tab_expansion("~/d/p/h", "/h", &tree).as_deref(), Some("~/development/projects/homelab"));
    }

    #[test]
    fn tab_without_a_match_changes_nothing() {
        assert_eq!(tab_expansion("~/zzz", "/h", &tree), None);
        assert_eq!(tab_expansion("~/development/zzz", "/h", &tree), None);
    }

    #[test]
    fn dropping_onto_the_folder_field_takes_the_folder_or_the_folder_of_a_file() {
        let dir = scratch("folder");
        let file = dir.join("notes.txt");
        std::fs::write(&file, "x").unwrap();
        let expected = folder_for(&dir);
        assert_eq!(dropped(PathKind::Folder, std::slice::from_ref(&file), "/nirgends"), expected);
        assert_eq!(dropped(PathKind::Folder, &[PathBuf::from("Z:/gibt/es/nicht"), dir.clone()], "/nirgends"), expected);
        assert_eq!(dropped(PathKind::Folder, &[PathBuf::from("Z:/gibt/es/nicht")], "/nirgends"), None);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn dropping_onto_the_image_field_takes_only_images() {
        let dir = scratch("image");
        let (text, image) = (dir.join("a.txt"), dir.join("b.PNG"));
        std::fs::write(&text, "x").unwrap();
        std::fs::write(&image, "x").unwrap();
        assert_eq!(dropped(PathKind::Image, std::slice::from_ref(&dir), "/h"), None, "ein Ordner ist kein Bild");
        assert_eq!(dropped(PathKind::Image, std::slice::from_ref(&text), "/h"), None);
        assert_eq!(dropped(PathKind::Image, &[text, image.clone()], "/h"), Some(slashed(&image)));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_dropped_image_inside_home_is_shortened() {
        let dir = scratch("home");
        let image = dir.join("wall.jpg");
        std::fs::write(&image, "x").unwrap();
        let home = slashed(&dir);
        assert_eq!(dropped(PathKind::Image, &[image], &home).as_deref(), Some("~/wall.jpg"));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
