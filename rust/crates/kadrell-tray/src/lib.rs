//! Kadrell: Tray-/Menüleisten-Icon, Systembenachrichtigungen, Badge und Sounds, unabhängig vom GUI-Framework.
//!
//! - `model`: reine Regeln und Texte (Port von `StatusMenu.swift`, `Notifications.swift`, `Feedback.swift`,
//!   Badge-Zähler aus `AttentionTracker.swift`), auf jeder Plattform getestet.
//! - `icon`: zur Laufzeit gezeichnete Icons (Windows).
//! - Windows: `Shell_NotifyIconW` auf einem eigenen Thread mit verstecktem Fenster, Toasts über WinRT.
//! - macOS: `NSStatusItem`, `UNUserNotificationCenter`, Dock-Badge (nur durch Lesen geprüft, nicht gebaut).
//!
//! Der Aufrufer hält ein [`Tray`], schiebt Zustand hinein (`set_status`, `set_menu`, `notify`, `set_badge`) und
//! liest [`TrayEvent`]s aus `events()`. Das Icon verschwindet beim Drop.

pub mod icon;
pub mod model;

pub use model::{
    Display, ItemIcon, Lang, MenuSession, Notification, NotifyLevel, Sound, SoundLevel, Status, StatusSummary, TrayItem,
};

use std::sync::mpsc::{self, Receiver, Sender};

#[cfg(windows)]
mod toast;
#[cfg(windows)]
#[path = "windows.rs"]
mod platform;
#[cfg(target_os = "macos")]
#[path = "macos.rs"]
mod platform;
#[cfg(not(any(windows, target_os = "macos")))]
#[path = "stub.rs"]
mod platform;

/// Was der Nutzer am Icon oder an einer Benachrichtigung getan hat.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TrayEvent {
    /// Linksklick aufs Icon (Fenster zeigen).
    Clicked,
    /// Menüeintrag gewählt: die `id` aus `TrayItem::Entry` (Sessions als `model::session_id`).
    MenuItem(String),
    /// Klick auf eine Benachrichtigung: der `session_key` aus `Notification`.
    NotificationClicked(String),
}

#[derive(Clone, Debug)]
pub struct TrayConfig {
    /// AppUserModelID, z. B. "de.malura.kadrell". Windows: Toasts eines Programms ohne Installer brauchen sie.
    pub app_id: String,
    /// Name im Toast-Kopf und im Benachrichtigungscenter.
    pub display_name: String,
    /// Windows: Toast mit dem Systemton (Standard aus: der Ton kommt aus [`play`], sonst klingt es doppelt).
    pub toast_sound: bool,
    /// Windows: `SetCurrentProcessExplicitAppUserModelID(app_id)` setzen (gruppiert Taskleiste und Toasts).
    pub set_process_app_id: bool,
}

impl Default for TrayConfig {
    fn default() -> Self {
        Self { app_id: "de.malura.kadrell.rust".into(), display_name: "Kadrell".into(), toast_sound: false, set_process_app_id: true }
    }
}

/// Das Icon. Auf macOS nur auf dem Hauptthread anlegen und benutzen (AppKit), unter Windows von jedem Thread aus.
pub struct Tray {
    backend: platform::Backend,
    events: Receiver<TrayEvent>,
}

impl Tray {
    pub fn new(config: TrayConfig) -> Tray {
        let (tx, events): (Sender<TrayEvent>, _) = mpsc::channel();
        Tray { backend: platform::Backend::new(config, tx), events }
    }

    /// Ereignisse seit dem letzten Abruf (`try_recv`/`recv_timeout`). Der Aufrufer weckt damit sein GUI auf.
    pub fn events(&self) -> &Receiver<TrayEvent> {
        &self.events
    }

    /// Kurzstatus: Icon mit Punkt, Text (macOS neben dem Symbol, Windows als Tooltip). `Display::Hidden` blendet aus.
    pub fn set_status(&self, summary: StatusSummary) {
        self.backend.set_status(summary);
    }

    /// Menü ersetzen (Bau mit `model::build_menu`).
    pub fn set_menu(&self, items: Vec<TrayItem>) {
        self.backend.set_menu(items);
    }

    /// Systembenachrichtigung zeigen (die Entscheidung "ob" trifft `model::should_notify`). Klick liefert
    /// `TrayEvent::NotificationClicked(session_key)`.
    pub fn notify(&self, n: Notification) {
        self.backend.notify(n);
    }

    /// macOS: Dock-Badge. Windows: Zahl als Overlay-Icon auf der Taskleisten-Schaltfläche, braucht vorher
    /// [`Tray::set_window_handle`]; ohne Fenster wird nur gemerkt. 0 entfernt das Badge.
    pub fn set_badge(&self, count: usize) {
        self.backend.set_badge(count);
    }

    /// Windows: HWND des Hauptfensters (als `isize`), an dem das Taskleisten-Overlay hängt. macOS: ignoriert.
    pub fn set_window_handle(&self, hwnd: isize) {
        self.backend.set_window_handle(hwnd);
    }
}

/// Sound wie `Feedback.play`: leise System-Klänge (macOS: Tink, Glass, Pop, Bottle, Morse bei Lautstärke 0,3;
/// Windows: `MessageBeep`), nur wenn `model::should_play` es für `level` erlaubt.
pub fn play(level: SoundLevel, sound: Sound) {
    if model::should_play(level, sound) {
        platform::play(sound);
    }
}

/// "Bewegung reduzieren" der Bedienungshilfen (Windows: Animationen aus, macOS: `accessibilityDisplayShouldReduceMotion`).
pub fn reduce_motion() -> bool {
    platform::reduce_motion()
}
