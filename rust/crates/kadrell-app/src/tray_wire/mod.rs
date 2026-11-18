//! Tray-Icon, Systembenachrichtigungen, Klänge, Badge und das Verstecken des Hauptfensters (Welle 3).
//!
//! - Das Icon (`kadrell-tray`) zeigt den Kurzstatus und ein Menü wie `StatusMenu.swift`; seine Ereignisse holt die
//!   Shell auf einem Timer ab und führt sie aus (Fenster zeigen, Session fokussieren, neue Session, beenden).
//! - Die Entscheidungen „Klang, Benachrichtigung, Hüpfen“ trifft der Tracker (`AppState::update_attention`), die
//!   Einstellungen filtern sie (`rules`), das Tray führt sie aus.
//! - Der rote Knopf, Alt+F4 und „Fenster schließen“ verstecken das Fenster nur. Kadrell und alle Sessions laufen
//!   weiter; Tray-Klick, `kadrell` ohne Befehl und ein zweiter Start holen es zurück.

pub(crate) mod rules;
pub(crate) mod view;
mod window;

use std::path::Path;
use std::time::Duration;

use gpui::{Context, Window};

use kadrell_control::exec::Outcome;
use kadrell_control::protocol::{ControlRequest, ControlResponse};
use kadrell_i18n::trd;
use kadrell_tray::model::{ID_NEXT_WAITING, ID_OPEN, ItemIcon, TrayItem, session_key_of};
use kadrell_tray::{Tray, TrayConfig, TrayEvent};

use crate::Shell;
use crate::state::now_secs;
use rules::Action;
use view::View;
use window::Native;

/// Interne Aufträge über den Steuerkanal, kein Teil des CLI-Vertrags: ein zweiter Start (`kadrell` ohne Befehl, während
/// die App läuft) holt das Fenster zurück, die Probe versteckt es wie der rote Knopf.
pub const SHOW_WINDOW: &str = "--show-window";
pub const HIDE_WINDOW: &str = "--hide-window";

const ID_NEW_SESSION: &str = "new-session";
const ID_QUIT: &str = "quit";

/// So oft schaut die Shell nach Ereignissen des Icons.
const PUMP: Duration = Duration::from_millis(150);

/// Zustand der Anbindung: das Icon, das Fenster auf Systemebene, ob es sichtbar ist, was das Icon zuletzt zeigte.
pub struct TrayWire {
    tray: Option<Tray>,
    native: Native,
    pub visible: bool,
    last: Option<View>,
    log: bool,
}

impl TrayWire {
    pub fn new() -> TrayWire {
        TrayWire { tray: None, native: Native::default(), visible: true, last: None, log: std::env::var_os("KADRELL_DEV_TRAYLOG").is_some() }
    }

    /// Das Icon verschwindet beim Beenden sofort, nicht erst mit dem Prozess.
    pub fn close(&mut self) {
        self.tray = None;
    }

    fn trace(&self, what: &str) {
        if self.log {
            eprintln!("kadrell-tray: {what}");
        }
    }
}

/// Ein zweiter Start, während die App läuft: das Fenster der laufenden Instanz zeigen.
pub fn forward_show(endpoint: &Path) {
    let req = ControlRequest::new(vec![SHOW_WINDOW.to_string()], String::new(), None);
    let _ = kadrell_control::client::send(endpoint, &req, Duration::from_secs(5));
}

/// Fenster der App, sobald es offen ist: der Dock-Klick wird vor `run` angemeldet, das Fenster gibt es erst danach.
pub type WindowSlot = std::rc::Rc<std::cell::Cell<Option<gpui::WindowHandle<Shell>>>>;

/// Nur der macOS-Dock-Klick: Fenster zurückholen, wenn die App ohne sichtbares Fenster angeklickt wird. GPUI nimmt den
/// Handler nur an der `Application` an (vor `run`), das Fenster kommt deshalb über den Slot.
#[cfg(target_os = "macos")]
pub fn install_reopen(app: &gpui::Application) -> WindowSlot {
    let slot = WindowSlot::default();
    let window = slot.clone();
    app.on_reopen(move |cx| {
        if let Some(handle) = window.get() {
            let _ = handle.update(cx, |shell, window, cx| shell.show_main_window(window, cx));
        }
    });
    slot
}

#[cfg(not(target_os = "macos"))]
pub fn install_reopen(_app: &gpui::Application) -> WindowSlot {
    WindowSlot::default()
}

fn ok() -> Option<Outcome> {
    Some(Outcome { response: ControlResponse::ok(""), followup: None })
}

impl Shell {
    /// Legt das Icon an und hängt die Fensterereignisse ein. Einmal beim Start, auf dem Hauptthread (AppKit).
    pub(crate) fn start_tray(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.tray.native = Native::of(window);
        let config = TrayConfig { display_name: format!("Kadrell{}", self.state.profile.label()), ..TrayConfig::default() };
        let tray = Tray::new(config);
        if let Some(hwnd) = self.tray.native.hwnd {
            tray.set_window_handle(hwnd);
        }
        self.tray.tray = Some(tray);
        let shell = cx.entity();
        window.on_window_should_close(cx, move |window, cx| shell.update(cx, |shell, cx| shell.close_requested(window, cx)));
        cx.observe_window_activation(window, |shell, window, cx| shell.on_activation(window, cx)).detach();
        self.refresh_tray(window, cx);
        self.start_dev_script(window, cx);
        cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor().timer(PUMP).await;
                if this.update_in(cx, |shell, window, cx| shell.pump_tray(window, cx)).is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    /// Für Proben ohne Eingabe: `KADRELL_DEV_TRAY="hide@3000,show@6000"` versteckt bzw. zeigt das Fenster nach so
    /// vielen Millisekunden nach dem Start (der CLI-Client kennt die Aufträge `--hide-window` nicht).
    fn start_dev_script(&self, window: &mut Window, cx: &mut Context<Self>) {
        let Ok(script) = std::env::var("KADRELL_DEV_TRAY") else { return };
        for step in script.split(',') {
            let Some((what, ms)) = step.split_once('@') else { continue };
            let (show, ms) = (what.trim() == "show", ms.trim().parse::<u64>().unwrap_or(0));
            cx.spawn_in(window, async move |this, cx| {
                cx.background_executor().timer(Duration::from_millis(ms)).await;
                let _ = this.update_in(cx, |shell, window, cx| if show { shell.show_main_window(window, cx) } else { shell.hide_main_window(window, cx) });
            })
            .detach();
        }
    }

    /// Aufträge des Steuerkanals, die das Fenster betreffen (`wire::run_job`); `None` für alles andere.
    pub(crate) fn window_command(&mut self, req: &ControlRequest, window: &mut Window, cx: &mut Context<Self>) -> Option<Outcome> {
        match req.argv.as_slice() {
            [arg] if arg == SHOW_WINDOW => self.show_main_window(window, cx),
            [arg] if arg == HIDE_WINDOW => self.hide_main_window(window, cx),
            _ => return None,
        }
        ok()
    }

    /// Der Nutzer will das Fenster schließen (roter Knopf, Alt+F4): verstecken statt schließen. Gibt zurück, ob es
    /// wirklich zugehen soll (nur, wenn es sich nicht verstecken lässt).
    fn close_requested(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.down {
            return true;
        }
        if !self.tray.native.can_hide() {
            return true;
        }
        self.hide_main_window(window, cx);
        false
    }

    /// Fenster verstecken. Ist das Icon ausgeschaltet („Menüleiste: aus“), käme der Nutzer nicht mehr hin: dann nur
    /// in die Taskleiste legen (Windows).
    pub(crate) fn hide_main_window(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if cfg!(not(target_os = "macos")) && self.state.settings.status_bar() == kadrell_core::settings::StatusBarDisplay::Hidden {
            window.minimize_window();
            return;
        }
        self.tray.native.hide(cx);
        self.tray.visible = false;
        self.tray.trace("Fenster versteckt");
        self.window_state_changed(window, cx);
    }

    /// Fenster zeigen und nach vorn holen.
    pub(crate) fn show_main_window(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.tray.native.show(window, cx);
        self.tray.visible = true;
        self.tray.trace("Fenster gezeigt");
        self.window_state_changed(window, cx);
    }

    fn on_activation(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.window_state_changed(window, cx);
    }

    /// „Fenster vorn“ für die Marke „neu“ und die Klangregeln: sichtbar und aktiv.
    fn window_state_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.state.window_active = self.tray.visible && window.is_window_active();
        self.state.update_attention();
        self.refresh_tray(window, cx);
        cx.notify();
    }

    fn pump_tray(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let events: Vec<TrayEvent> = self.tray.tray.as_ref().map(|t| t.events().try_iter().collect()).unwrap_or_default();
        for event in events {
            self.on_tray_event(event, window, cx);
        }
        self.refresh_tray(window, cx);
    }

    fn on_tray_event(&mut self, event: TrayEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.tray.trace(&format!("Ereignis {event:?}"));
        match event {
            TrayEvent::Clicked => self.show_main_window(window, cx),
            TrayEvent::NotificationClicked(key) => self.focus_session(&key, window, cx),
            TrayEvent::MenuItem(id) => self.on_tray_menu(&id, window, cx),
        }
    }

    fn on_tray_menu(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(key) = session_key_of(id) {
            return self.focus_session(key, window, cx);
        }
        match id {
            ID_OPEN => self.show_main_window(window, cx),
            ID_NEXT_WAITING => self.focus_next_waiting(window, cx),
            ID_NEW_SESSION => {
                self.show_main_window(window, cx);
                self.new_session_action(window, cx);
            }
            ID_QUIT => cx.quit(),
            _ => {}
        }
    }

    /// Fenster nach vorn, Session zeigen und fokussieren (Tray-Menü, Klick auf eine Benachrichtigung). Ist sie noch
    /// nicht sichtbar, kommt sie zur Auswahl dazu; `select_keys` würde eine sichtbare sonst abwählen.
    pub(crate) fn focus_session(&mut self, key: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.state.session(key).is_none() {
            return;
        }
        self.show_main_window(window, cx);
        if self.state.selected.iter().any(|k| k == key) {
            self.state.focus_key(key);
        } else {
            self.state.select_keys(&[key.to_string()], true);
        }
        self.sync(window, cx);
    }

    pub(crate) fn focus_next_waiting(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let view = self.tray_view();
        match rules::next_waiting(&view.waiting, self.state.focused.as_deref()).cloned() {
            Some(key) => self.focus_session(&key, window, cx),
            None => self.show_main_window(window, cx),
        }
    }

    fn tray_view(&self) -> View {
        let lang = rules::lang(kadrell_i18n::global().lang());
        let mut view = view::build(&self.state, lang, now_secs());
        view.menu = with_app_entries(view.menu);
        view
    }

    /// Führt die Wirkungen des Trackers aus und bringt Icon, Menü und Badge auf den Stand des Zustands.
    pub(crate) fn refresh_tray(&mut self, window: &mut Window, _cx: &mut Context<Self>) {
        let effects = self.state.take_effects();
        let settings = (self.state.settings.sounds(), self.state.settings.notifications());
        let lang = rules::lang(kadrell_i18n::global().lang());
        for action in rules::plan(&effects, settings.0, settings.1) {
            self.run_action(action, lang, window);
        }
        // Ohne Icon (die Oberfläche zeichnet vor `start_tray`) nichts merken: sonst hält der erste Aufruf mit Icon den
        // Stand für schon gesendet, und Statustext, Menü und Badge kommen nie an.
        let Some(tray) = &self.tray.tray else { return };
        let view = self.tray_view();
        if self.tray.last.as_ref() == Some(&view) {
            return;
        }
        tray.set_status(view.summary);
        tray.set_menu(view.menu.clone());
        tray.set_badge(view.badge);
        self.tray.trace(&describe(&view));
        self.tray.last = Some(view);
    }

    fn run_action(&mut self, action: Action, lang: kadrell_tray::Lang, window: &mut Window) {
        self.tray.trace(&format!("Wirkung {action:?}"));
        match action {
            Action::Play(sound) => kadrell_tray::play(rules::sound_level(self.state.settings.sounds()), sound),
            Action::RequestAttention => window.request_attention(),
            Action::Notify { key, waiting } => {
                let notification = view::notification(&self.state, &key, waiting, lang);
                if let (Some(tray), Some(n)) = (&self.tray.tray, notification) {
                    tray.notify(n);
                }
            }
        }
    }
}

/// Hängt „Neue Session“ und „Kadrell beenden“ an das Menü der Statusanzeige (Swift hat sie im Dock- und App-Menü).
fn with_app_entries(mut menu: Vec<TrayItem>) -> Vec<TrayItem> {
    let entry = |id: &str, label: String| TrayItem::Entry { id: id.into(), label, icon: ItemIcon::None, enabled: true };
    menu.push(entry(ID_NEW_SESSION, trd!("Neue Session")));
    menu.push(TrayItem::Separator);
    menu.push(entry(ID_QUIT, trd!("Kadrell beenden")));
    menu
}

/// Eine Zeile fürs Protokoll (`KADRELL_DEV_TRAYLOG=1`): was das Icon jetzt zeigt.
fn describe(view: &View) -> String {
    let labels: Vec<String> = view
        .menu
        .iter()
        .map(|item| match item {
            TrayItem::Separator => "-".to_string(),
            TrayItem::Entry { label, enabled, .. } => if *enabled { label.clone() } else { format!("({label})") },
        })
        .collect();
    format!(
        "Status {} | Anzeige {:?} | Badge {} | Menü [{}]",
        kadrell_tray::model::tooltip(&view.summary),
        view.summary.display,
        view.badge,
        labels.join(" | ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_entries_come_last_and_are_translated() {
        for source in ["Neue Session", "Kadrell beenden"] {
            assert!(kadrell_i18n::id_for_de(source).is_some(), "{source} fehlt im Katalog");
        }
        let menu = with_app_entries(kadrell_tray::model::build_menu(&[], &[], kadrell_tray::Lang::De));
        let ids: Vec<&str> = menu.iter().filter_map(|i| if let TrayItem::Entry { id, .. } = i { Some(id.as_str()) } else { None }).collect();
        assert_eq!(ids, ["", ID_OPEN, ID_NEW_SESSION, ID_QUIT]);
        assert_eq!(menu.last(), Some(&TrayItem::Entry { id: ID_QUIT.into(), label: trd!("Kadrell beenden"), icon: ItemIcon::None, enabled: true }));
    }

    #[test]
    fn second_start_asks_the_running_instance_to_show_its_window() {
        use std::sync::{Arc, Mutex};
        let dir = std::env::temp_dir().join(format!("kadrell-forward-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let endpoint = kadrell_control::endpoint::endpoint_path(&dir);
        let seen = Arc::new(Mutex::new(Vec::new()));
        let recorder = seen.clone();
        let service: kadrell_control::Service = Arc::new(move |req, _| {
            recorder.lock().unwrap().push(req.argv.clone());
            ControlResponse::ok("")
        });
        let server = kadrell_control::ControlServer::start(&endpoint, service).unwrap();
        forward_show(&endpoint);
        drop(server);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(*seen.lock().unwrap(), [vec![SHOW_WINDOW.to_string()]]);
    }

    #[test]
    fn window_tokens_are_not_cli_commands() {
        for token in [SHOW_WINDOW, HIDE_WINDOW] {
            assert!(kadrell_control::command::parse(&[token.to_string()]).is_err(), "{token} darf kein Befehl des Vertrags sein");
        }
    }
}
