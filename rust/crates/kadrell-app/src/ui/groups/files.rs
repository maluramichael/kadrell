//! Ordner aus Explorer/Finder, die auf den Baum, den Dialog „Neue Session“ oder den Dialog „Gruppe bearbeiten“
//! gezogen werden (`FolderIndex.folder(for:)`): ein Ordner gilt selbst, eine Datei durch ihren Ordner.

use std::path::Path;

use gpui::ExternalPaths;

/// Ordner zu einem abgelegten Pfad (mit `/`), `None` bei Pfaden, die es nicht gibt.
pub fn folder_for(path: &Path) -> Option<String> {
    let dir = if path.is_dir() { path } else if path.exists() { path.parent()? } else { return None };
    let text = dir.to_string_lossy().replace(std::path::MAIN_SEPARATOR, "/");
    // `\?\`-Präfix der Windows-Pfade fällt weg.
    let text = text.strip_prefix("//?/").unwrap_or(&text);
    Some(if text.len() > 1 { text.trim_end_matches('/') } else { text }.to_string())
}

/// Erster brauchbarer Ordner einer Ablage.
pub fn first_folder(paths: &ExternalPaths) -> Option<String> {
    paths.paths().iter().find_map(|p| folder_for(p))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directory_and_file_map_to_a_folder() {
        let dir = std::env::temp_dir();
        let own = folder_for(&dir).unwrap();
        assert!(!own.contains('\u{5c}') &&!own.ends_with('/'));
        let file = dir.join(format!("kadrell-drop-{}.txt", std::process::id()));
        std::fs::write(&file, "x").unwrap();
        assert_eq!(folder_for(&file).as_deref(), Some(own.as_str()));
        std::fs::remove_file(&file).unwrap();
        assert_eq!(folder_for(&file), None);
    }

    #[test]
    fn first_folder_skips_missing_paths() {
        let paths = ExternalPaths(vec![std::path::PathBuf::from("Z:/gibt/es/nicht"), std::env::temp_dir()].into());
        assert_eq!(first_folder(&paths), folder_for(&std::env::temp_dir()));
    }
}
