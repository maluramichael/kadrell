//! Farbraum des Fensters: Werte unverändert ausgeben wie die Swift-App.
//!
//! GPUI (`gpui-pre-apple/src/metal_renderer.rs:190`, `layer.setColorspace(Display P3)`) markiert den Metal-Layer als
//! Display P3. Das System rechnet unsere sRGB-Hex-Werte (Catppuccin & Co.) dadurch in den Farbraum des Bildschirms um;
//! gesättigte Töne weichen ab (Akzentblau Mocha: Swift `89b4fa`, Rust im Bild `92b3f4`). Die Swift-App zeichnet ohne
//! Farbraum-Umrechnung, die Werte kommen unverändert auf den Bildschirm. Gemessen auf dem Mac (Pixelwerte im
//! Bildschirmfoto): Layer ohne Farbraum (`nil`) und Layer im Farbraum des Bildschirms ergeben `89b4fa`, sRGB `92b3f4`.
//! Wir setzen den Farbraum deshalb nach dem Öffnen jedes Fensters auf `nil`, im eigenen Code, ohne die GPUI-Quelle zu
//! ändern. `nil` folgt auch einem Wechsel auf einen anderen Bildschirm. Nur macOS; anderswo gibt es nichts zu tun.

use gpui::Window;

/// Nimmt dem Layer des Fensters einmal den Farbraum (idempotent, kostet pro Aufruf einen Vergleich).
#[cfg(target_os = "macos")]
pub fn ensure_native(window: &Window) {
    use objc2::runtime::AnyObject;
    use objc2::{Encode, Encoding, RefEncode, msg_send};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use std::cell::RefCell;
    use std::collections::HashSet;

    /// `CGColorSpaceRef` mit der Kodierung, die `setColorspace:` erwartet (objc2 prüft sie in Debug-Builds).
    #[repr(C)]
    struct CGColorSpace {
        _opaque: [u8; 0],
    }
    // SAFETY: Kodierung wie in CoreGraphics (`^{CGColorSpace=}`), nur als Zeiger verwendet.
    unsafe impl Encode for CGColorSpace {
        const ENCODING: Encoding = Encoding::Struct("CGColorSpace", &[]);
    }
    // SAFETY: wie oben.
    unsafe impl RefEncode for CGColorSpace {
        const ENCODING_REF: Encoding = Encoding::Pointer(&<Self as Encode>::ENCODING);
    }

    thread_local! {
        /// Schon umgestellte Ansichten (Adressen), damit nicht jeder Frame den Layer anfasst.
        static DONE: RefCell<HashSet<usize>> = RefCell::new(HashSet::new());
    }

    let Ok(handle) = HasWindowHandle::window_handle(window) else { return };
    let RawWindowHandle::AppKit(appkit) = handle.as_raw() else { return };
    let view = appkit.ns_view.as_ptr() as usize;
    if DONE.with(|d| d.borrow().contains(&view)) {
        return;
    }
    // SAFETY: `ns_view` ist die NSView dieses Fensters (Hauptthread, Render). `layer` liefert den Metal-Layer, den
    // GPUI der Ansicht gibt; `setColorspace:` kennt jede CAMetalLayer und nimmt `NULL`.
    unsafe {
        let view_obj = appkit.ns_view.as_ptr().cast::<AnyObject>();
        let layer: *mut AnyObject = msg_send![view_obj, layer];
        if layer.is_null() {
            return;
        }
        let none: *const CGColorSpace = std::ptr::null();
        let _: () = msg_send![layer, setColorspace: none];
    }
    DONE.with(|d| d.borrow_mut().insert(view));
}

#[cfg(not(target_os = "macos"))]
pub fn ensure_native(_window: &Window) {}
