//! Titelleiste und Systemdialoge folgen dem Farbschema der App, nicht dem des Systems (wie `window.appearance =
//! Theme.appearance` in der Swift-App). Ohne das bleibt die Titelleiste im hellen System hell, auch im dunklen Mocha.
//! Nur macOS; anderswo gibt es nichts zu tun.

use gpui::Window;

/// Setzt das Erscheinungsbild des Fensters auf dunkel oder hell.
#[cfg(target_os = "macos")]
pub fn apply(window: &Window, dark: bool) {
    use objc2_app_kit::{NSAppearance, NSAppearanceCustomization, NSAppearanceNameAqua, NSAppearanceNameDarkAqua, NSView};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let Ok(handle) = HasWindowHandle::window_handle(window) else { return };
    let RawWindowHandle::AppKit(appkit) = handle.as_raw() else { return };
    // SAFETY: `ns_view` ist die NSView des Fensters dieses Prozesses, wir sind auf dem Hauptthread (Render).
    let view: &NSView = unsafe { appkit.ns_view.cast::<NSView>().as_ref() };
    let Some(ns_window) = view.window() else { return };
    // SAFETY: die Namen sind konstante System-Strings.
    let name = unsafe { if dark { NSAppearanceNameDarkAqua } else { NSAppearanceNameAqua } };
    ns_window.setAppearance(NSAppearance::appearanceNamed(name).as_deref());
}

#[cfg(not(target_os = "macos"))]
pub fn apply(_window: &Window, _dark: bool) {}
