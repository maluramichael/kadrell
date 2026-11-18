//! Piepton für eine Taste oder einen Schalter, der gerade nichts darf (`NSSound.beep()` in `ExtensionsModel`).

/// Systemton: Windows `MessageBeep`, macOS `NSBeep`; sonst nichts.
pub fn beep() {
    platform();
}

#[cfg(windows)]
fn platform() {
    #[link(name = "user32")]
    unsafe extern "system" {
        fn MessageBeep(kind: u32) -> i32;
    }
    // 0 (`MB_OK`) ist der Standardton des Systems.
    // SAFETY: reiner Systemaufruf ohne Zeiger.
    unsafe {
        MessageBeep(0);
    }
}

#[cfg(target_os = "macos")]
fn platform() {
    unsafe extern "C" {
        fn NSBeep();
    }
    // SAFETY: AppKit ist durch GPUI geladen, `NSBeep` hat keine Argumente.
    unsafe { NSBeep() }
}

#[cfg(not(any(windows, target_os = "macos")))]
fn platform() {}
