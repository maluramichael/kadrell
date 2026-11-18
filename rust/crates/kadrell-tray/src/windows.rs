//! Windows: Tray-Icon über `Shell_NotifyIconW` auf einem eigenen Thread mit verstecktem Fenster und Nachrichtenschleife.
//! Der Aufrufer-Thread stellt Befehle in eine Warteschlange und weckt den Thread mit `PostMessageW`.
//!
//! Bewusst ohne `SetForegroundWindow` (Projektregel): Das Kontextmenü ist ein `TrackPopupMenu` ohne vorherigen
//! Vordergrund-Wechsel. Windows schließt es dann nicht bei einem Klick daneben, nur bei Auswahl oder Esc. Wer das
//! ändern will, ruft in `show_menu` vor `TrackPopupMenu` `SetForegroundWindow(hwnd)` auf (Standard-Idiom).

use std::collections::VecDeque;
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::Graphics::Gdi::HBRUSH;
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Shell::{
    ITaskbarList3, NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_SHOWTIP, NIF_TIP, NIIF_INFO, NIM_ADD, NIM_DELETE, NIM_MODIFY,
    NIM_SETVERSION, NIN_BALLOONUSERCLICK, NOTIFYICON_VERSION_4, NOTIFYICONDATAW, SetCurrentProcessExplicitAppUserModelID,
    Shell_NotifyIconW, TaskbarList,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CW_USEDEFAULT, CreateIcon, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyIcon, DestroyMenu,
    DestroyWindow, DispatchMessageW, GWLP_USERDATA, GetCursorPos, GetMessageW, GetWindowLongPtrW, HICON, MB_ICONASTERISK,
    MB_OK, MESSAGEBOX_STYLE, MF_GRAYED, MF_SEPARATOR, MF_STRING, MSG, PostMessageW, PostQuitMessage,
    RegisterClassW, RegisterWindowMessageW, SPI_GETCLIENTAREAANIMATION, SetWindowLongPtrW, SystemParametersInfoW,
    TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenu, TranslateMessage, WINDOW_EX_STYLE, WM_APP, WM_CONTEXTMENU, WM_DESTROY,
    WM_LBUTTONUP, WM_RBUTTONUP, WNDCLASSW, WS_OVERLAPPED,
};
use windows::Win32::System::Diagnostics::Debug::MessageBeep;
use windows::core::{BOOL, PCWSTR, w};

use crate::icon::{badge_label, render_badge, render_tray_icon};
use crate::model::{self, ItemIcon, Notification, Sound, StatusSummary, TrayItem};
use crate::toast;
use crate::{TrayConfig, TrayEvent};

const WM_COMMANDS: u32 = WM_APP + 1;
const WM_TRAY: u32 = WM_APP + 2;
const TRAY_ID: u32 = 1;
const MENU_BASE: usize = 1000;
const NIN_KEYSELECT: u32 = 0x403;

enum Cmd {
    Status(StatusSummary),
    Menu(Vec<TrayItem>),
    Notify(Notification),
    Badge(usize),
    Window(isize),
    Quit,
}

type Queue = Arc<Mutex<VecDeque<Cmd>>>;

pub struct Backend {
    queue: Queue,
    hwnd: isize,
    thread: Option<JoinHandle<()>>,
}

impl Backend {
    pub fn new(config: TrayConfig, events: Sender<TrayEvent>) -> Self {
        let queue: Queue = Arc::default();
        let (ready_tx, ready_rx) = mpsc::channel::<isize>();
        let q = queue.clone();
        let thread = std::thread::Builder::new()
            .name("kadrell-tray".into())
            .spawn(move || run(config, events, q, ready_tx))
            .ok();
        let hwnd = ready_rx.recv().unwrap_or(0);
        Backend { queue, hwnd, thread }
    }

    fn send(&self, cmd: Cmd) {
        if self.hwnd == 0 {
            return;
        }
        self.queue.lock().unwrap_or_else(|e| e.into_inner()).push_back(cmd);
        // SAFETY: PostMessageW ist threadsicher; das Fenster gehört dem Tray-Thread, der bis zum Drop lebt.
        unsafe {
            let _ = PostMessageW(Some(HWND(self.hwnd as *mut _)), WM_COMMANDS, WPARAM(0), LPARAM(0));
        }
    }

    pub fn set_status(&self, s: StatusSummary) {
        self.send(Cmd::Status(s));
    }
    pub fn set_menu(&self, items: Vec<TrayItem>) {
        self.send(Cmd::Menu(items));
    }
    pub fn notify(&self, n: Notification) {
        self.send(Cmd::Notify(n));
    }
    pub fn set_badge(&self, count: usize) {
        self.send(Cmd::Badge(count));
    }
    pub fn set_window_handle(&self, hwnd: isize) {
        self.send(Cmd::Window(hwnd));
    }
}

impl Drop for Backend {
    fn drop(&mut self) {
        self.send(Cmd::Quit);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

pub fn play(sound: Sound) {
    let style: MESSAGEBOX_STYLE = if sound.important() { MB_ICONASTERISK } else { MB_OK };
    // SAFETY: MessageBeep hat keine Vorbedingungen.
    unsafe {
        let _ = MessageBeep(style);
    }
}

pub fn reduce_motion() -> bool {
    let mut animations = BOOL(1);
    // SAFETY: `animations` ist ein gültiger BOOL-Zeiger für SPI_GETCLIENTAREAANIMATION.
    let ok = unsafe {
        SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            Some((&mut animations as *mut BOOL).cast()),
            Default::default(),
        )
    };
    ok.is_ok() && !animations.as_bool()
}

// ---------------------------------------------------------------------------------------------------------------
// Tray-Thread

/// Zustand des Tray-Threads; liegt über GWLP_USERDATA am Fenster.
struct Inner {
    config: TrayConfig,
    events: Sender<TrayEvent>,
    queue: Queue,
    hwnd: HWND,
    status: StatusSummary,
    icon: Option<HICON>,
    added: bool,
    menu: Vec<TrayItem>,
    balloon_key: Option<String>,
    taskbar: Option<ITaskbarList3>,
    badge: usize,
    badge_window: isize,
    badge_icon: Option<HICON>,
    taskbar_created: u32,
}

fn run(config: TrayConfig, events: Sender<TrayEvent>, queue: Queue, ready: Sender<isize>) {
    // SAFETY: Standard-Nachrichtenschleife auf dem Thread, der Fenster und COM besitzt.
    unsafe {
        let com = CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok();
        prepare_app_id(&config);
        match create_window() {
            Some(hwnd) => {
                let inner = Box::into_raw(Box::new(Inner::new(config, events, queue, hwnd)));
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, inner as isize);
                let _ = ready.send(hwnd.0 as isize);
                message_loop();
                drop(Box::from_raw(inner));
            }
            None => {
                log::error!("kadrell-tray: Fenster konnte nicht angelegt werden");
                let _ = ready.send(0);
            }
        }
        if com {
            CoUninitialize();
        }
    }
}

unsafe fn message_loop() {
    let mut msg = MSG::default();
    // SAFETY: gültiger MSG-Zeiger, Schleife endet mit WM_QUIT.
    unsafe {
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

fn prepare_app_id(config: &TrayConfig) {
    if let Err(e) = toast::register_app_id(&config.app_id, &config.display_name) {
        log::warn!("kadrell-tray: AUMID nicht registriert: {e}");
    }
    if config.set_process_app_id {
        let id = windows::core::HSTRING::from(config.app_id.as_str());
        // SAFETY: `id` lebt über den Aufruf.
        if let Err(e) = unsafe { SetCurrentProcessExplicitAppUserModelID(&id) } {
            log::warn!("kadrell-tray: Prozess-AUMID nicht gesetzt: {e}");
        }
    }
}

unsafe fn create_window() -> Option<HWND> {
    // SAFETY: Fensterklasse und Fenster nur auf diesem Thread, Klassenname statisch.
    unsafe {
        let hinstance = GetModuleHandleW(None).ok()?.into();
        let class = w!("KadrellTrayWindow");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: hinstance,
            lpszClassName: class,
            hbrBackground: HBRUSH::default(),
            ..Default::default()
        };
        RegisterClassW(&wc);
        // Normales (nie gezeigtes) Top-Level-Fenster statt Message-only: nur so kommt "TaskbarCreated" an.
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            class,
            w!("Kadrell Tray"),
            WS_OVERLAPPED,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            0,
            0,
            None,
            None,
            Some(hinstance),
            None,
        )
        .ok()
    }
}

extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // SAFETY: GWLP_USERDATA zeigt auf das `Inner`, das `run` anlegt und erst nach der Schleife freigibt.
    let inner = unsafe { (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Inner).as_mut() };
    match inner {
        Some(inner) => inner.handle(msg, wparam, lparam).unwrap_or_else(|| unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }),
        None => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

impl Inner {
    fn new(config: TrayConfig, events: Sender<TrayEvent>, queue: Queue, hwnd: HWND) -> Self {
        // SAFETY: Aufruf mit statischem Namen.
        let taskbar_created = unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) };
        let mut inner = Inner {
            config,
            events,
            queue,
            hwnd,
            status: StatusSummary::default(),
            icon: None,
            added: false,
            menu: Vec::new(),
            balloon_key: None,
            taskbar: None,
            badge: 0,
            badge_window: 0,
            badge_icon: None,
            taskbar_created,
        };
        inner.apply_status();
        inner
    }

    /// `None` = Standardbehandlung.
    fn handle(&mut self, msg: u32, wparam: WPARAM, lparam: LPARAM) -> Option<LRESULT> {
        match msg {
            WM_COMMANDS => self.drain(),
            WM_TRAY => self.tray_event(wparam, lparam),
            WM_DESTROY => {
                self.remove_icon();
                // SAFETY: beendet die Schleife dieses Threads.
                unsafe { PostQuitMessage(0) };
            }
            m if m == self.taskbar_created => {
                // Explorer wurde neu gestartet: Icon neu anmelden.
                self.added = false;
                self.apply_status();
            }
            _ => return None,
        }
        Some(LRESULT(0))
    }

    fn drain(&mut self) {
        loop {
            let cmd = self.queue.lock().unwrap_or_else(|e| e.into_inner()).pop_front();
            match cmd {
                Some(Cmd::Status(s)) => {
                    self.status = s;
                    self.apply_status();
                }
                Some(Cmd::Menu(items)) => self.menu = items,
                Some(Cmd::Notify(n)) => self.notify(&n),
                Some(Cmd::Badge(c)) => {
                    self.badge = c;
                    self.apply_badge();
                }
                Some(Cmd::Window(h)) => {
                    self.badge_window = h;
                    self.apply_badge();
                }
                Some(Cmd::Quit) => {
                    // SAFETY: das Fenster gehört diesem Thread; WM_DESTROY räumt auf.
                    unsafe {
                        let _ = DestroyWindow(self.hwnd);
                    }
                    return;
                }
                None => return,
            }
        }
    }

    fn tray_event(&mut self, wparam: WPARAM, lparam: LPARAM) {
        let event = (lparam.0 as u32) & 0xFFFF;
        match event {
            WM_LBUTTONUP | NIN_KEYSELECT => {
                let _ = self.events.send(TrayEvent::Clicked);
            }
            WM_CONTEXTMENU | WM_RBUTTONUP => {
                // Mit NOTIFYICON_VERSION_4 trägt wparam die Bildschirmkoordinaten.
                let _ = wparam;
                self.show_menu();
            }
            NIN_BALLOONUSERCLICK => {
                if let Some(key) = self.balloon_key.take() {
                    let _ = self.events.send(TrayEvent::NotificationClicked(key));
                }
            }
            _ => {}
        }
    }

    // ----- Icon ---------------------------------------------------------------------------------------------

    fn nid(&self) -> NOTIFYICONDATAW {
        NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: self.hwnd,
            uID: TRAY_ID,
            ..Default::default()
        }
    }

    fn apply_status(&mut self) {
        if self.status.display == model::Display::Hidden {
            self.remove_icon();
            return;
        }
        let new_icon = make_icon(&render_tray_icon(32, model::dot(&self.status)), 32);
        let mut nid = self.nid();
        nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP | NIF_SHOWTIP;
        nid.uCallbackMessage = WM_TRAY;
        nid.hIcon = new_icon.unwrap_or_default();
        fill(&mut nid.szTip, &model::tooltip(&self.status));
        // SAFETY: gültige NOTIFYICONDATAW, Fenster gehört diesem Thread.
        unsafe {
            if self.added {
                let _ = Shell_NotifyIconW(NIM_MODIFY, &nid);
            } else if Shell_NotifyIconW(NIM_ADD, &nid).as_bool() {
                nid.Anonymous.uVersion = NOTIFYICON_VERSION_4;
                let v4 = Shell_NotifyIconW(NIM_SETVERSION, &nid).as_bool();
                log::info!("kadrell-tray: Icon angemeldet (Version 4: {v4})");
                self.added = true;
            } else {
                log::warn!("kadrell-tray: Shell_NotifyIcon(NIM_ADD) fehlgeschlagen");
            }
        }
        if let Some(old) = std::mem::replace(&mut self.icon, new_icon) {
            // SAFETY: das alte Icon ist nach NIM_MODIFY nicht mehr in Gebrauch.
            unsafe {
                let _ = DestroyIcon(old);
            }
        }
    }

    fn remove_icon(&mut self) {
        if self.added {
            let nid = self.nid();
            // SAFETY: gültige NOTIFYICONDATAW.
            unsafe {
                let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
            }
            self.added = false;
        }
        if let Some(i) = self.icon.take() {
            // SAFETY: nach NIM_DELETE nicht mehr in Gebrauch.
            unsafe {
                let _ = DestroyIcon(i);
            }
        }
    }

    // ----- Menü ---------------------------------------------------------------------------------------------

    fn show_menu(&mut self) {
        // SAFETY: Menü wird auf diesem Thread angelegt, benutzt und zerstört.
        unsafe {
            let Ok(menu) = CreatePopupMenu() else { return };
            for (i, item) in self.menu.iter().enumerate() {
                match item {
                    TrayItem::Separator => {
                        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
                    }
                    TrayItem::Entry { label, icon, enabled, .. } => {
                        let text = windows::core::HSTRING::from(menu_label(label, *icon));
                        let flags = if *enabled { MF_STRING } else { MF_STRING | MF_GRAYED };
                        let _ = AppendMenuW(menu, flags, MENU_BASE + i, &text);
                    }
                }
            }
            let mut pt = POINT::default();
            let _ = GetCursorPos(&mut pt);
            // Ohne SetForegroundWindow (siehe Moduldoku): TPM_RETURNCMD liefert die Wahl direkt.
            let cmd = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_RIGHTBUTTON, pt.x, pt.y, Some(0), self.hwnd, None);
            let _ = DestroyMenu(menu);
            let chosen = (cmd.0 as usize).checked_sub(MENU_BASE).and_then(|i| self.menu.get(i));
            if let Some(TrayItem::Entry { id, .. }) = chosen {
                let _ = self.events.send(TrayEvent::MenuItem(id.clone()));
            }
        }
    }

    // ----- Benachrichtigung ----------------------------------------------------------------------------------

    fn notify(&mut self, n: &Notification) {
        let shown = toast::show(&self.config.app_id, n, self.config.toast_sound, self.events.clone());
        if shown.is_ok() {
            log::info!("kadrell-tray: Toast angezeigt ({})", n.session_key);
        }
        if let Err(e) = shown {
            log::warn!("kadrell-tray: Toast fehlgeschlagen ({e}), Sprechblase als Rückfall");
            self.balloon(n);
        }
    }

    /// Rückfall: Sprechblase am Tray-Icon (`NIF_INFO`). Windows 10/11 zeigt sie ebenfalls als Toast, aber unter dem
    /// Namen des Programms in der Taskleiste; ein Klick kommt als `NIN_BALLOONUSERCLICK`, nur für die letzte.
    fn balloon(&mut self, n: &Notification) {
        if !self.added {
            return;
        }
        let mut nid = self.nid();
        nid.uFlags = NIF_INFO;
        nid.dwInfoFlags = NIIF_INFO;
        fill(&mut nid.szInfoTitle, &n.title);
        fill(&mut nid.szInfo, &n.body);
        // SAFETY: gültige NOTIFYICONDATAW.
        unsafe {
            let _ = Shell_NotifyIconW(NIM_MODIFY, &nid);
        }
        self.balloon_key = Some(n.session_key.clone());
    }

    // ----- Taskleisten-Overlay -------------------------------------------------------------------------------

    fn apply_badge(&mut self) {
        if self.badge_window == 0 {
            return;
        }
        if self.taskbar.is_none() {
            // SAFETY: COM ist auf diesem Thread initialisiert.
            self.taskbar = unsafe {
                CoCreateInstance::<_, ITaskbarList3>(&TaskbarList, None, CLSCTX_INPROC_SERVER)
                    .ok()
                    .filter(|t| t.HrInit().is_ok())
            };
        }
        let Some(taskbar) = &self.taskbar else { return };
        let hwnd = HWND(self.badge_window as *mut _);
        let new_icon = (self.badge > 0).then(|| make_icon(&render_badge(16, self.badge), 16)).flatten();
        let text = windows::core::HSTRING::from(badge_label(self.badge));
        // SAFETY: HWND stammt vom Aufrufer; ein fremdes Fenster ist für ITaskbarList3 zulässig.
        unsafe {
            let _ = taskbar.SetOverlayIcon(hwnd, new_icon.unwrap_or_default(), &text);
        }
        if let Some(old) = std::mem::replace(&mut self.badge_icon, new_icon) {
            // SAFETY: Overlay ist ersetzt.
            unsafe {
                let _ = DestroyIcon(old);
            }
        }
    }
}

fn menu_label(label: &str, icon: ItemIcon) -> String {
    match icon {
        ItemIcon::None => label.to_string(),
        ItemIcon::Waiting => format!("● {label}"),
        ItemIcon::Done => format!("✓ {label}"),
    }
}

/// Schreibt `text` nullterminiert und gekürzt in ein festes UTF-16-Feld.
fn fill(dst: &mut [u16], text: &str) {
    let mut n = 0;
    for u in text.encode_utf16().take(dst.len() - 1) {
        dst[n] = u;
        n += 1;
    }
    dst[n] = 0;
}

/// HICON aus BGRA-Pixeln (nicht vormultipliziert), `size` x `size`.
fn make_icon(bgra: &[u8], size: u32) -> Option<HICON> {
    let row_bytes = size.div_ceil(32) as usize * 4;
    let mut and_mask = vec![0u8; row_bytes * size as usize];
    for y in 0..size as usize {
        for x in 0..size as usize {
            if bgra[(y * size as usize + x) * 4 + 3] == 0 {
                and_mask[y * row_bytes + x / 8] |= 0x80 >> (x % 8);
            }
        }
    }
    // SAFETY: Puffer haben die vom Aufruf verlangten Größen; Windows kopiert sie.
    unsafe {
        let hinstance = GetModuleHandleW(None).ok()?.into();
        CreateIcon(Some(hinstance), size as i32, size as i32, 1, 32, and_mask.as_ptr(), bgra.as_ptr()).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Dot;

    #[test]
    fn fill_truncates_and_terminates() {
        let mut buf = [0xFFFFu16; 5];
        fill(&mut buf, "abcdefg");
        assert_eq!(&buf, &[97, 98, 99, 100, 0]);
        fill(&mut buf, "x");
        assert_eq!(&buf[..2], &[120, 0]);
    }

    #[test]
    fn menu_labels_get_prefix() {
        assert_eq!(menu_label("A", ItemIcon::None), "A");
        assert_eq!(menu_label("A", ItemIcon::Waiting), "● A");
        assert_eq!(menu_label("A", ItemIcon::Done), "✓ A");
    }

    #[test]
    fn icon_creation_works() {
        let icon = make_icon(&render_tray_icon(32, Dot::Waiting), 32).expect("icon");
        // SAFETY: eben erzeugt.
        unsafe { DestroyIcon(icon).unwrap() };
    }
}
