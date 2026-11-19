//! Systemdialog für Ordner oder Bilder (`chooseFolder(start:images:)` in `FolderInput.swift`): Ordner, oder mit `images`
//! genau eine Bilddatei. Ist die App seit dem Start ersetzt, kommt statt des Dialogs die Warnung (`warnReplaced`).
//!
//! Abweichung: GPUI kennt weder einen Startordner noch einen Dateityp-Filter des Dialogs. Der Dialog öffnet dort, wo das
//! System es für richtig hält; eine gewählte Datei, die kein Bild ist, wird danach verworfen (mit Piepen).

use std::path::{Path, PathBuf};

use gpui::{Context, PathPromptOptions, Window};
use kadrell_i18n::trd;

use super::overlay_queue::warn_if_replaced;
use kadrell_tray::beep;

/// Endungen, die als Bild gelten (`UTType.image`, soweit sie auf dem Rechner vorkommen).
const IMAGE_EXTENSIONS: [&str; 13] = ["png", "jpg", "jpeg", "gif", "bmp", "webp", "tif", "tiff", "heic", "heif", "ico", "icns", "svg"];

/// Ist die Datei ein Bild? Nach Endung, Groß- und Kleinschreibung egal.
pub fn is_image(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| IMAGE_EXTENSIONS.iter().any(|known| known.eq_ignore_ascii_case(e)))
}

/// Was der Dialog zurückgibt, wenn es passt: bei Bildern nur Bilddateien, sonst der Pfad wie gewählt.
pub fn accept(path: &Path, images: bool) -> Option<&Path> {
    (!images || is_image(path)).then_some(path)
}

/// Optionen des Systemdialogs: Ordner, oder mit `images` eine Datei; nie mehrere.
pub fn options(images: bool) -> PathPromptOptions {
    PathPromptOptions { files: images, directories: !images, multiple: false, prompt: Some(trd!("Wählen").into()) }
}

/// Öffnet den Dialog und ruft `done` mit dem gewählten Pfad. Bricht der Nutzer ab oder passt die Wahl nicht, geschieht
/// nichts (ein falscher Dateityp piept).
pub fn choose<V: 'static>(images: bool, window: &mut Window, cx: &mut Context<V>, done: fn(&mut V, String, &mut Window, &mut Context<V>)) {
    choose_with(images, window, cx, done);
}

/// Wie `choose`, mit einem Abschluss, der etwas mitnehmen darf (etwa die Zeile, deren Feld gefüllt wird).
pub fn choose_with<V: 'static>(
    images: bool,
    window: &mut Window,
    cx: &mut Context<V>,
    done: impl FnOnce(&mut V, String, &mut Window, &mut Context<V>) + 'static,
) {
    choose_in(None, images, window, cx, done);
}

/// Wie `choose_with`, der Dialog öffnet in `start` (`chooseFolder(start:)`). Unter Windows gilt das für Ordner; ohne
/// vorhandenen Startordner und für Bilder (dort ohne Startordner) öffnet der Dialog, wo das System es will.
pub fn choose_in<V: 'static>(
    start: Option<&str>,
    images: bool,
    window: &mut Window,
    cx: &mut Context<V>,
    done: impl FnOnce(&mut V, String, &mut Window, &mut Context<V>) + 'static,
) {
    if warn_if_replaced(window, cx) {
        return;
    }
    #[cfg(windows)]
    if let Some(dir) = start_dir(start, images) {
        return choose_native(dir, window, cx, done);
    }
    #[cfg(not(windows))]
    let _ = start;
    let picked = cx.prompt_for_paths(options(images));
    cx.spawn_in(window, async move |this, cx| {
        let Ok(Ok(Some(paths))) = picked.await else { return };
        let Some(path) = paths.first() else { return };
        let Some(path) = accept(path, images) else { return beep() };
        let path = path.to_string_lossy().into_owned();
        let _ = this.update_in(cx, |this, window, cx| done(this, path, window, cx));
    })
    .detach();
}

/// Startordner des Dialogs: `~` aufgelöst, nur wenn es ihn gibt (sonst wählt das System), nie bei Bildern.
pub fn start_dir(start: Option<&str>, images: bool) -> Option<PathBuf> {
    let raw = start?.trim();
    if images || raw.is_empty() {
        return None;
    }
    let home = || std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")).unwrap_or_default();
    let expanded = match raw.strip_prefix('~') {
        Some(rest) if rest.is_empty() || rest.starts_with(['/', '\\']) => format!("{}{rest}", home()),
        _ => raw.to_string(),
    };
    let path = PathBuf::from(expanded);
    path.is_dir().then_some(path)
}

#[cfg(windows)]
fn choose_native<V: 'static>(
    dir: PathBuf,
    window: &mut Window,
    cx: &mut Context<V>,
    done: impl FnOnce(&mut V, String, &mut Window, &mut Context<V>) + 'static,
) {
    let owner = native::owner_of(window);
    let (tx, rx) = async_channel::bounded(1);
    let prompt = trd!("Wählen");
    std::thread::spawn(move || {
        let _ = tx.send_blocking(native::pick_folder(&dir.to_string_lossy(), &prompt, owner));
    });
    cx.spawn_in(window, async move |this, cx| {
        let Ok(Some(path)) = rx.recv().await else { return };
        let path = path.to_string_lossy().into_owned();
        let _ = this.update_in(cx, |this, window, cx| done(this, path, window, cx));
    })
    .detach();
}

#[cfg(windows)]
mod native;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_dir_needs_an_existing_folder_and_no_images() {
        let here = std::env::current_dir().unwrap();
        let here = here.to_string_lossy();
        assert!(start_dir(Some(&here), false).is_some());
        assert!(start_dir(Some(&here), true).is_none());
        assert!(start_dir(Some("/gibt/es/nicht/kadrell"), false).is_none());
        assert!(start_dir(Some("  "), false).is_none());
        assert!(start_dir(None, false).is_none());
        assert!(start_dir(Some("~"), false).is_some());
    }

    #[test]
    fn images_are_recognised_by_extension() {
        assert!(is_image(Path::new("C:/bilder/hintergrund.PNG")));
        assert!(is_image(Path::new("a.jpeg")));
        assert!(!is_image(Path::new("notizen.txt")));
        assert!(!is_image(Path::new("ordner")));
    }

    #[test]
    fn only_images_pass_when_images_are_wanted() {
        assert!(accept(Path::new("a.png"), true).is_some());
        assert!(accept(Path::new("a.txt"), true).is_none());
        assert!(accept(Path::new("C:/Projekte"), false).is_some());
    }

    #[test]
    fn the_dialog_wants_folders_or_one_file() {
        let folders = options(false);
        assert!(folders.directories && !folders.files && !folders.multiple);
        let image = options(true);
        assert!(image.files && !image.directories && !image.multiple);
    }
}
