//! Hauptfenster verstecken und zurückholen. GPUI kennt kein Verstecken eines einzelnen Fensters: unter Windows
//! geht es nur über den HWND (`ShowWindow`), auf dem Mac versteckt `App::hide` die ganze App (wie der rote Knopf
//! in der Swift-App) und `App::activate` holt sie zurück (macOS: nur durch Lesen geprüft, nicht gebaut).

use gpui::{App, Window};

/// Das Fenster auf Ebene des Systems.
#[derive(Clone, Copy, Debug, Default)]
pub struct Native {
    /// Windows: der HWND des Hauptfensters (auch für das Taskleisten-Overlay).
    pub hwnd: Option<isize>,
}

impl Native {
    pub fn of(window: &Window) -> Native {
        Native { hwnd: hwnd_of(window) }
    }

    /// Lässt sich das Fenster verstecken? Sonst bleibt der rote Knopf beim Schließen.
    pub fn can_hide(&self) -> bool {
        cfg!(target_os = "macos") || self.hwnd.is_some()
    }

    pub fn hide(&self, cx: &mut App) {
        hide(self.hwnd, cx);
    }

    pub fn show(&self, window: &mut Window, cx: &mut App) {
        show(self.hwnd, window, cx);
    }
}

#[cfg(windows)]
fn hwnd_of(window: &Window) -> Option<isize> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    match HasWindowHandle::window_handle(window).ok()?.as_raw() {
        RawWindowHandle::Win32(handle) => Some(handle.hwnd.get()),
        _ => None,
    }
}

#[cfg(not(windows))]
fn hwnd_of(_window: &Window) -> Option<isize> {
    None
}

#[cfg(windows)]
fn hide(hwnd: Option<isize>, _cx: &mut App) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{SW_HIDE, ShowWindow};
    if let Some(hwnd) = hwnd {
        // SAFETY: `hwnd` ist das Fenster dieses Prozesses; wir sind auf dessen Thread (GPUI-Hauptthread).
        unsafe { ShowWindow(hwnd as _, SW_HIDE) };
    }
}

/// Zeigt das Fenster und holt es nach vorn. Wird nur auf Klick des Nutzers hin aufgerufen (Tray, Benachrichtigung),
/// dann erlaubt Windows `SetForegroundWindow`.
#[cfg(windows)]
fn show(hwnd: Option<isize>, _window: &mut Window, _cx: &mut App) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{SW_RESTORE, SetForegroundWindow, ShowWindow};
    if let Some(hwnd) = hwnd {
        // SAFETY: wie `hide`.
        unsafe {
            ShowWindow(hwnd as _, SW_RESTORE);
            SetForegroundWindow(hwnd as _);
        }
    }
}

#[cfg(target_os = "macos")]
fn hide(_hwnd: Option<isize>, cx: &mut App) {
    cx.hide();
}

#[cfg(target_os = "macos")]
fn show(_hwnd: Option<isize>, window: &mut Window, cx: &mut App) {
    cx.activate(true);
    window.activate_window();
}

#[cfg(not(any(windows, target_os = "macos")))]
fn hide(_hwnd: Option<isize>, _cx: &mut App) {}

#[cfg(not(any(windows, target_os = "macos")))]
fn show(_hwnd: Option<isize>, window: &mut Window, _cx: &mut App) {
    window.activate_window();
}
