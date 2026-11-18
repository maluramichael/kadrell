//! Piepton für Fehlbedienung im Dialog (Swift: `NSSound.beep()`), ohne Abhängigkeit von Fensterbibliotheken.

#[cfg(windows)]
pub fn beep() {
    // SAFETY: `MessageBeep` hat keine Vorbedingungen, 0xFFFFFFFF ist der einfache Systemton.
    unsafe extern "system" {
        fn MessageBeep(kind: u32) -> i32;
    }
    unsafe {
        MessageBeep(0xFFFF_FFFF);
    }
}

#[cfg(target_os = "macos")]
pub fn beep() {
    #[link(name = "AppKit", kind = "framework")]
    unsafe extern "C" {
        fn NSBeep();
    }
    // SAFETY: `NSBeep` hat keine Vorbedingungen.
    unsafe {
        NSBeep();
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn beep() {}
