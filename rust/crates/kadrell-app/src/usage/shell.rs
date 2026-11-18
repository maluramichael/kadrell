//! Konten, Nutzung und Statistik an der `Shell`: die Schleife, die den Hub auf dem Hintergrund-Executor treibt, die
//! Meldungen, das Konto-Menü, die Werte für die Leiste und das Statistik-Fenster. Alles in `impl Shell`, damit die
//! gemeinsamen Dateien nur wenige Zeilen Anbindung brauchen.

use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gpui::{AppContext, ClickEvent, Context, Entity, KeyDownEvent, MouseDownEvent, Pixels, Point, Window};
use kadrell_accounts::AccountSettings;
use kadrell_claude::history::{SessionHistory, cache_path};
use kadrell_core::SessionStatus;
use kadrell_i18n::trd;

use super::backend::HubGlobal;
use super::hub::{Config, Hub, Notice, Step, state_from};
use super::lang;
use super::logic::{StatusCounts, UsageShown};
use crate::Shell;
use crate::state::{AppState, now_secs};
use crate::ui::context_menu::{Cmd, Item};
use crate::ui::notice::{NoticeDialog, NoticeEvent};
use crate::ui::stats::{Counters, Loader, StatsEvent, StatsView, format};
use crate::ui::usage_bar::BarCtl;

/// Aktionen des Konto-Menüs in der Leiste.
#[derive(Clone, Debug, PartialEq)]
pub enum AccountCmd {
    Switch(String),
    ToggleAutoswitch,
    Add,
    Manage,
}

#[derive(Clone, Copy, PartialEq)]
enum NoticeKind {
    AutoSwitch,
    Pace,
    NoAccount,
}

/// Was die Shell für Konten, Nutzung und Statistik hält.
pub struct UsageUi {
    pub hub: Arc<Hub>,
    history: Arc<Mutex<SessionHistory>>,
    pub(crate) stats: Option<Entity<StatsView>>,
    pub(crate) notice: Option<Entity<NoticeDialog>>,
    notice_kind: Option<NoticeKind>,
    pending: VecDeque<Notice>,
    /// Die zuletzt gedrückte Taste der Leiste, 80 ms lang eingesunken (`bar_wire::flash_bar`).
    pub(crate) pressed: Option<&'static str>,
}

impl UsageUi {
    pub fn new(state: &AppState) -> UsageUi {
        let now = now_secs();
        let hub = if std::env::var("KADRELL_DEV_USAGE").as_deref() == Ok("fake") {
            Hub::fake(now)
        } else {
            let accounts = state_from(state.settings.accounts_index().as_deref(), state.settings.active_account_id());
            Hub::system(accounts, state.profile.dir.join("usage-history.jsonl"), &config(state), now)
        };
        UsageUi { hub: Arc::new(hub), history: Arc::new(Mutex::new(SessionHistory::default())), stats: None, notice: None, notice_kind: None, pending: VecDeque::new(), pressed: None }
    }

    /// Ein Fenster der Konten oder Statistik ist offen: das Terminal dahinter bekommt keine Tasten.
    pub fn overlay_open(&self) -> bool {
        self.stats.is_some() || self.notice.is_some()
    }
}

/// Einstellungen für den Hub, bei jedem Schritt neu gelesen.
pub fn config(state: &AppState) -> Config {
    let s = &state.settings;
    Config {
        account: AccountSettings {
            autoswitch_enabled: s.autoswitch_enabled(),
            autoswitch_threshold: s.autoswitch_threshold() as i32,
            autoswitch_on_pace: s.autoswitch_on_pace(),
            temporary_profile: state.profile.temporary,
        },
        pace_warning: s.autoswitch_pace_warning(),
        notice: s.autoswitch_notice(),
        lang: lang(),
    }
}

impl Shell {
    /// Startet die Schleife: alle zwei Sekunden ein Schritt auf dem Hintergrund-Executor (Abfragen halten nie das Zeichnen
    /// auf, `UsageService` hält Takt und Wartezeit nach 429 selbst ein).
    pub(crate) fn start_usage(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        cx.set_global(HubGlobal(self.usage.hub.clone()));
        cx.spawn_in(window, async move |this, cx| {
            loop {
                let Ok((hub, cfg)) = this.update(cx, |s, _| (s.usage.hub.clone(), config(&s.state))) else { break };
                let step = cx.background_executor().spawn(async move { hub.step(now_secs(), &cfg) }).await;
                if this.update_in(cx, |s, window, cx| s.apply_step(step, window, cx)).is_err() {
                    break;
                }
                cx.background_executor().timer(Duration::from_secs(2)).await;
            }
        })
        .detach();
        self.open_stats_from_env(window, cx);
    }

    fn apply_step(&mut self, step: Step, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(p) = step.persist {
            self.state.save_accounts(&p);
        }
        self.usage.pending.extend(step.notices);
        self.show_next_notice(window, cx);
        cx.notify();
    }

    // MARK: Leiste

    /// Zähler und Zustände für die rechte Seite der Leiste.
    pub(crate) fn bar_ctl(&self, window: &Window, cx: &mut Context<Self>) -> BarCtl {
        let counts = self.status_counts();
        let s = &self.state.settings;
        BarCtl {
            view: self.usage.hub.view(),
            now: now_secs(),
            shown: UsageShown { session: s.bar_show_session(), weekly: s.bar_show_weekly(), fable: s.bar_show_fable() },
            counts,
            detachable: self.detachable_keys().len(),
            waiting: counts.waiting,
            error: self.state.shown_error(),
            lang: lang(),
            width: window.viewport_size().width.as_f32(),
            left: 0.,
            on_account: Rc::new(cx.listener(|this, ev: &MouseDownEvent, _, cx| this.open_account_menu(ev.position, cx))),
            on_detach: Rc::new(cx.listener(|this, _: &MouseDownEvent, window, cx| this.detach_idle_pill(window, cx))),
            on_waiting: Rc::new(cx.listener(|this, _: &MouseDownEvent, window, cx| this.select_waiting(window, cx))),
            tip: self.state.tip_text(),
            on_tip: Rc::new(cx.listener(|this, _: &ClickEvent, _, cx| this.dismiss_tip(cx))),
            version_warning: self.state.version_warning.clone(),
            on_version: Rc::new(cx.listener(|this, _: &ClickEvent, _, cx| this.copy_update_command(cx))),
            announcement: self.state.announcement.clone(),
            pressed: self.usage.pressed,
            flash: self.flash_fn(cx),
            ext: Vec::new(),
            update: None,
        }
    }

    fn status_counts(&self) -> StatusCounts {
        let mut c = StatusCounts::default();
        for s in &self.state.store.sessions.sessions {
            match self.state.display_status(&s.id) {
                Some(SessionStatus::Running) => c.running += 1,
                Some(SessionStatus::Waiting) => c.waiting += 1,
                Some(SessionStatus::Idle) => c.idle += 1,
                Some(SessionStatus::Error) => c.error += 1,
                None => c.detached += 1,
            }
        }
        c
    }

    /// Fertige Claude-Sessions mit Prozess (Terminals ohne Claude bleiben).
    pub(crate) fn detachable_keys(&self) -> Vec<String> {
        self.state
            .store
            .sessions
            .sessions
            .iter()
            .filter(|s| !s.is_shell() && !s.is_remote() && self.state.display_status(&s.id) == Some(SessionStatus::Idle))
            .map(|s| s.id.clone())
            .collect()
    }

    // MARK: Konto-Menü

    fn open_account_menu(&mut self, at: Point<Pixels>, cx: &mut Context<Self>) {
        let view = self.usage.hub.view();
        let mark = |on: bool| if on { "✓ " } else { "    " };
        let mut items: Vec<Item> = view
            .menu
            .iter()
            .map(|m| {
                let label = if m.detail.is_empty() { m.title.clone() } else { format!("{}   {}", m.title, m.detail) };
                Item::Entry(format!("{}{label}", mark(m.active)), Cmd::Account(AccountCmd::Switch(m.id.clone())))
            })
            .collect();
        items.push(Item::Separator);
        let auto = self.state.settings.autoswitch_enabled();
        items.push(Item::Entry(format!("{}{}", mark(auto), trd!("Automatisch wechseln bei Limit")), Cmd::Account(AccountCmd::ToggleAutoswitch)));
        items.push(Item::Entry(format!("    {}", trd!("Konto hinzufügen …")), Cmd::Account(AccountCmd::Add)));
        items.push(Item::Entry(format!("    {}", trd!("Konten verwalten …")), Cmd::Account(AccountCmd::Manage)));
        self.open_context(gpui::point(at.x, at.y + gpui::px(14.)), items, cx);
    }

    pub(crate) fn run_account_cmd(&mut self, cmd: AccountCmd, window: &mut Window, cx: &mut Context<Self>) {
        match cmd {
            AccountCmd::Switch(id) => self.usage.hub.request(super::hub::Cmd::Switch(id)),
            AccountCmd::ToggleAutoswitch => {
                let on = !self.state.settings.autoswitch_enabled();
                self.state.set_autoswitch(on);
            }
            AccountCmd::Add => self.usage.hub.request(super::hub::Cmd::AddCurrent),
            AccountCmd::Manage => self.open_settings_action(window, cx),
        }
        cx.notify();
    }

    // MARK: Meldungen

    fn show_next_notice(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.usage.notice.is_some() {
            return;
        }
        let Some(next) = self.usage.pending.pop_front() else { return };
        let l = lang();
        let (kind, title, text) = match next {
            Notice::AutoSwitched(name) => (NoticeKind::AutoSwitch, trd!("Account automatisch gewechselt"), trd!("Jetzt aktiv: „%@“. Grund: Nutzungslimit erreicht.", name.as_str())),
            Notice::Pace(w) => {
                let (when, until) = (format::weekday_clock(w.lockout, l), format::weekday_clock(w.resets, l));
                (NoticeKind::Pace, trd!("7-Tage-Limit läuft zu schnell voll"), trd!("Bei diesem Tempo ist das 7-Tage-Limit am %@ voll. Reset ist aber erst am %@, bis dahin wärst du gesperrt.", when.as_str(), until.as_str()))
            }
            Notice::NoAccount => (NoticeKind::NoAccount, trd!("Kein angemeldeter Claude-Account gefunden"), trd!("Melde dich zuerst in einer Session mit „claude“ an, dann versuch es erneut.")),
        };
        let dialog = cx.new(|cx| NoticeDialog::new(title, text, kind != NoticeKind::NoAccount, window, cx));
        cx.subscribe_in(&dialog, window, move |this, _, ev: &NoticeEvent, window, cx| {
            let NoticeEvent::Close { suppress } = ev;
            this.close_notice(kind, *suppress, window, cx);
        })
        .detach();
        self.usage.notice = Some(dialog);
        self.usage.notice_kind = Some(kind);
        cx.notify();
    }

    fn close_notice(&mut self, kind: NoticeKind, suppress: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.usage.notice = None;
        self.usage.notice_kind = None;
        if suppress {
            match kind {
                NoticeKind::AutoSwitch => self.state.set_notice_off("autoswitch.notice"),
                NoticeKind::Pace => self.state.set_notice_off("autoswitch.paceWarning"),
                NoticeKind::NoAccount => {}
            }
        }
        self.sync(window, cx);
        self.show_next_notice(window, cx);
    }

    // MARK: Statistik

    pub fn open_stats(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.usage.stats.is_some() || self.overlay.is_some() || self.palette.is_some() || self.settings.is_some() || self.rename.is_some() {
            return;
        }
        let config_dir = self.state.env.config_dir();
        let loader = Loader {
            hub: self.usage.hub.clone(),
            cache_file: cache_path(&self.state.profile.dir.to_string_lossy(), &config_dir),
            config_dir,
            settings_path: self.state.profile.settings_path(),
            history: self.usage.history.clone(),
        };
        let counters = Counters::from_settings(&self.state.settings);
        let view = cx.new(|cx| StatsView::new(loader, counters, window, cx));
        cx.subscribe_in(&view, window, |this, _, _: &StatsEvent, window, cx| this.close_stats(window, cx)).detach();
        self.usage.stats = Some(view);
        cx.notify();
    }

    pub fn close_stats(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.usage.stats.take().is_some() {
            self.sync(window, cx);
        }
    }

    /// Aktion „Statistik“ (Menü, ⌥F3 / Alt+F3): öffnet oder schließt.
    pub fn toggle_stats(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.usage.stats.is_some() {
            self.close_stats(window, cx);
        } else {
            self.open_stats(window, cx);
        }
    }

    /// Das Kürzel schließt die Seite auch, wenn sie den Fokus hat (die Tasten der Shell sind dann gesperrt).
    pub(crate) fn usage_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let m = ev.keystroke.modifiers;
        if self.usage.stats.is_some() && m.alt && !m.control && !m.platform && ev.keystroke.key == "f3" {
            self.close_stats(window, cx);
            cx.stop_propagation();
            return true;
        }
        false
    }

    /// Für Screenshots ohne Eingabe: `KADRELL_DEV_OVERLAY=stats`; `KADRELL_DEV_NOTICE=autoswitch|pace|none` zeigt eine Meldung.
    fn open_stats_from_env(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if std::env::var("KADRELL_DEV_OVERLAY").as_deref() == Ok("stats") {
            self.open_stats(window, cx);
        }
        let notice = match std::env::var("KADRELL_DEV_NOTICE").as_deref() {
            Ok("autoswitch") => Some(Notice::AutoSwitched("ben@example.org".into())),
            Ok("none") => Some(Notice::NoAccount),
            _ => None,
        };
        if let Some(n) = notice {
            self.usage.pending.push_back(n);
            self.show_next_notice(window, cx);
        }
    }
}
