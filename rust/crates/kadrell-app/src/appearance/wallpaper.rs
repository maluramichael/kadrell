//! Grund hinter Baum und Arbeitsfläche (`Workspace/WallpaperView.swift`): Farbe des Schemas, darüber das Hintergrundbild
//! füllend über die ganze Fläche (ohne Balken). Baum und Kacheln liegen mit ihrer Deckkraft darüber, unter 100 % scheint
//! das Bild durch.

use std::path::{Path, PathBuf};

use gpui::{AnyElement, Hsla, IntoElement, ObjectFit, ParentElement, Styled, StyledImage, div, img};

/// Das Bild als „Aspect fill“ (kürzere Seite passt, der Rest wird mittig abgeschnitten) hinter allem anderen. `None`,
/// wenn kein Bild gewählt ist oder die Datei fehlt: dann zeigt die Fläche nur die Farbe.
pub fn layer(path: &str, base: Hsla) -> Option<AnyElement> {
    let file = usable(path)?;
    Some(div().absolute().top_0().left_0().size_full().bg(base).child(img(file).size_full().object_fit(ObjectFit::Cover)).into_any_element())
}

/// Pfad des Bilds, wenn es eine Datei ist.
pub fn usable(path: &str) -> Option<PathBuf> {
    let p = path.trim();
    (!p.is_empty() && Path::new(p).is_file()).then(|| PathBuf::from(p))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_and_missing_paths_show_no_image() {
        assert!(usable("").is_none());
        assert!(usable("   ").is_none());
        assert!(usable("Z:/gibt/es/nicht.png").is_none());
    }

    #[test]
    fn an_existing_file_is_used() {
        let file = std::env::temp_dir().join(format!("kadrell-wallpaper-test-{}.png", std::process::id()));
        std::fs::write(&file, b"x").unwrap();
        assert_eq!(usable(file.to_str().unwrap()), Some(file.clone()));
        let _ = std::fs::remove_file(file);
    }
}
