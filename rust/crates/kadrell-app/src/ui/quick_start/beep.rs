//! Warnton für eine Eingabe, die nichts bewirkt (Swift: `NSSound.beep()`). Folgt dem Alarmton des Systems, nicht der
//! Einstellung „Klänge“, wie in der Mac-App. Nur die Oberfläche piept, das Modell des Schnellstarts bleibt stumm (Tests).

/// Windows: `MessageBeep(MB_OK)`, der Standardton der Systemsteuerung.
#[cfg(windows)]
pub fn beep() {
    #[link(name = "user32")]
    unsafe extern "system" {
        fn MessageBeep(kind: u32) -> i32;
    }
    // SAFETY: MessageBeep hat keine Vorbedingungen.
    unsafe {
        MessageBeep(0);
    }
}

/// macOS: `NSBeep()` aus AppKit (dasselbe wie `NSSound.beep()`), nicht auf Windows gebaut.
#[cfg(target_os = "macos")]
pub fn beep() {
    #[link(name = "AppKit", kind = "framework")]
    unsafe extern "C" {
        fn NSBeep();
    }
    // SAFETY: NSBeep hat keine Vorbedingungen und darf vom Hauptthread aufgerufen werden.
    unsafe {
        NSBeep();
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn beep() {}
