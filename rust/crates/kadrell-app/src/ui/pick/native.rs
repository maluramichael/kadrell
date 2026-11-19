//! Ordnerdialog mit Startordner (Windows). GPUI reicht keinen Startordner an `IFileOpenDialog` durch; hier öffnet der
//! Dialog direkt dort, wie `NSOpenPanel.directoryURL` in `chooseFolder(start:)`.

use std::path::PathBuf;

use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{CLSCTX_ALL, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize};
use windows::Win32::UI::Shell::{FOS_FILEMUSTEXIST, FOS_PICKFOLDERS, FileOpenDialog, IFileOpenDialog, IShellItem, SIGDN_FILESYSPATH, SHCreateItemFromParsingName};
use windows::core::HSTRING;

/// Fenster der aktiven Ansicht als Besitzer des Dialogs (Zahl, damit sie zwischen Threads wandern darf).
pub fn owner_of(window: &gpui::Window) -> isize {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    match HasWindowHandle::window_handle(window).map(|h| h.as_raw()) {
        Ok(RawWindowHandle::Win32(h)) => h.hwnd.get(),
        _ => 0,
    }
}

/// Zeigt den Ordnerdialog, geöffnet in `start`. `None` bei Abbruch oder Fehler.
pub fn pick_folder(start: &str, prompt: &str, owner: isize) -> Option<PathBuf> {
    // SAFETY: COM wird auf diesem eigenen Thread gestartet und am Ende wieder abgebaut.
    unsafe {
        let init = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let result = show(start, prompt, owner);
        if init.is_ok() {
            CoUninitialize();
        }
        result
    }
}

unsafe fn show(start: &str, prompt: &str, owner: isize) -> Option<PathBuf> {
    // SAFETY: nur COM-Aufrufe mit gültigen, hier erzeugten Objekten; `owner` ist ein Fenster dieses Prozesses oder 0.
    unsafe {
        let dialog: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_ALL).ok()?;
        dialog.SetOptions(FOS_PICKFOLDERS | FOS_FILEMUSTEXIST).ok()?;
        dialog.SetOkButtonLabel(&HSTRING::from(prompt)).ok()?;
        if let Ok(item) = SHCreateItemFromParsingName::<_, _, IShellItem>(&HSTRING::from(start), None) {
            let _ = dialog.SetFolder(&item);
        }
        dialog.Show(Some(HWND(owner as *mut core::ffi::c_void))).ok()?;
        let item = dialog.GetResult().ok()?;
        let name = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
        let path = name.to_string().ok();
        windows::Win32::System::Com::CoTaskMemFree(Some(name.as_ptr().cast()));
        path.map(PathBuf::from)
    }
}
