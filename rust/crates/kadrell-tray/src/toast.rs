//! Windows-Toasts über WinRT (`ToastNotificationManager`) für ein Programm ohne Installer.
//!
//! Was ein unpaketiertes Programm dafür braucht (Stand Windows 10/11, nicht von Microsoft für diesen Fall
//! zugesichert, aber so im Einsatz bei vielen Desktop-Programmen):
//! 1. Eine AppUserModelID (AUMID). Hier `TrayConfig::app_id`.
//! 2. Ihre Registrierung unter `HKCU\Software\Classes\AppUserModelId\<AUMID>` mit `DisplayName`. Ohne sie bleibt
//!    der Toast je nach Windows-Stand unsichtbar. Das ist der Weg ohne Installer und ohne Startmenü-Verknüpfung
//!    (die sonst die AUMID trägt); ein Installer sollte stattdessen die Verknüpfung mit AUMID anlegen.
//! 3. Der Aufruf `CreateToastNotifierWithId(AUMID)`.
//!
//! Grenzen: Der Klick (`Activated`) erreicht uns nur, solange der Prozess läuft und seine Nachrichtenschleife
//! dreht (der Tray-Thread). Ein Toast, der im Info-Center liegen bleibt und nach einem Neustart angeklickt wird,
//! startet höchstens das Programm: ohne COM-Aktivator gibt es keine Weitergabe der Session. "Nicht stören" und
//! Focus Assist unterdrücken Toasts ohne Fehlermeldung; sie landen dann nur im Info-Center. Schlägt `Show` selbst
//! fehl, fällt `windows.rs` auf eine Sprechblase (`NIF_INFO`) zurück.

use std::sync::mpsc::Sender;

use windows::Data::Xml::Dom::XmlDocument;
use windows::Foundation::TypedEventHandler;
use windows::UI::Notifications::{ToastNotification, ToastNotificationManager};
use windows::Win32::System::Registry::{HKEY_CURRENT_USER, REG_SZ, RegSetKeyValueW};
use windows::core::{HSTRING, Result, w};

use crate::TrayEvent;
use crate::model::Notification;

/// Toast-Tag und -Gruppe sind auf 64 Zeichen begrenzt.
fn clip64(s: &str) -> String {
    s.chars().take(64).collect()
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&apos;")
}

pub fn toast_xml(n: &Notification, sound: bool) -> String {
    let audio = if sound { "" } else { "<audio silent=\"true\"/>" };
    format!(
        "<toast><visual><binding template=\"ToastGeneric\"><text>{}</text><text>{}</text></binding></visual>{audio}</toast>",
        escape(&n.title),
        escape(&n.body)
    )
}

/// Registriert die AUMID mit Anzeigenamen (idempotent, pro Nutzer, keine Adminrechte).
pub fn register_app_id(app_id: &str, display_name: &str) -> Result<()> {
    let subkey = HSTRING::from(format!("Software\\Classes\\AppUserModelId\\{app_id}"));
    let value: Vec<u16> = display_name.encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: `value` lebt über den Aufruf, die Längenangabe zählt Bytes einschließlich Nullterminator.
    unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            &subkey,
            w!("DisplayName"),
            REG_SZ.0,
            Some(value.as_ptr().cast()),
            (value.len() * 2) as u32,
        )
        .ok()
    }
}

/// Zeigt den Toast. Gleicher Tag (= Session) ersetzt den vorigen, die Gruppe bündelt im Info-Center.
pub fn show(app_id: &str, n: &Notification, sound: bool, events: Sender<TrayEvent>) -> Result<()> {
    let doc = XmlDocument::new()?;
    doc.LoadXml(&HSTRING::from(toast_xml(n, sound)))?;
    let toast = ToastNotification::CreateToastNotification(&doc)?;
    toast.SetTag(&HSTRING::from(clip64(&n.session_key)))?;
    if !n.group.is_empty() {
        toast.SetGroup(&HSTRING::from(clip64(&n.group)))?;
    }
    let key = n.session_key.clone();
    toast.Activated(&TypedEventHandler::new(move |_, _| {
        let _ = events.send(TrayEvent::NotificationClicked(key.clone()));
        Ok(())
    }))?;
    ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(app_id))?.Show(&toast)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(title: &str, body: &str) -> Notification {
        Notification { title: title.into(), body: body.into(), session_key: "k".into(), group: "g".into() }
    }

    #[test]
    fn xml_escapes_and_is_silent_by_default() {
        let x = toast_xml(&n("A & B › <C>", "\"x\""), false);
        assert!(x.contains("A &amp; B › &lt;C&gt;"));
        assert!(x.contains("&quot;x&quot;"));
        assert!(x.contains("<audio silent=\"true\"/>"));
        assert!(!toast_xml(&n("a", "b"), true).contains("audio"));
    }

    #[test]
    fn tag_is_clipped_at_64_chars() {
        assert_eq!(clip64(&"ä".repeat(100)).chars().count(), 64);
    }
}
