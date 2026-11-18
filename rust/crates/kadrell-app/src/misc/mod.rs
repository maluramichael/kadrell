//! Restliche Funktionen der Mac-App: Hilfe (⌥F1), „Über Kadrell“ (F1), „Was ist neu“ nach einem Update, Update-Prüfung
//! mit Pille, Leerzustand, Übernahme von `claude --bg`, automatisches Trennen fertiger Sessions und Links im Terminal.
//! Die Logik steckt in kleinen Modulen ohne GPUI (mit Tests), dieses Modul verdrahtet sie mit der Shell.

pub mod adopt;
pub mod changelog;
pub mod confirm;
pub mod empty;
pub mod help;
pub mod help_view;
pub mod idle;
pub mod open;
pub mod procs;
pub mod update;
pub mod whats_new;

use std::collections::VecDeque;

use gpui::{App, AppContext, Context, Div, Entity, InteractiveElement, KeyBinding, Window, actions};
use kadrell_accounts::UreqClient;
use kadrell_claude::background_shells::{AdoptCandidate, AdoptPlan};
use kadrell_claude::cli::SystemRunner;
use kadrell_core::Settings;
use kadrell_core::hotkeys::HotkeyAction;
use kadrell_i18n::trd;

use crate::Shell;
use crate::lang::{localize_shortcuts, tx};
use crate::state::now_secs;
use crate::ui::ext::keys;
use confirm::{ConfirmDialog, ConfirmEvent, Spec};
use help_view::{HelpEvent, HelpView, Kind};
use idle::IdleTracker;
use update::UpdateManifest;

actions!(kadrell_misc, [OpenHelp, OpenAbout, OpenWhatsNew]);

/// Key-Kontext der Terminal-Kachel: F1 gehört dort dem Programm im Terminal (htop, mc).
pub const TERMINAL_CONTEXT: &str = "KadrellTerminal";

/// Text mit Mac-Kürzeln in der Schreibweise des Systems und der eingestellten Sprache; `⌘⏎` wird unter Windows
/// `Strg+Enter` (`⏎` allein kennt die Kürzelumwandlung nicht).
pub fn os_text(text: &str) -> String {
    let text = if cfg!(target_os = "macos") { text.to_string() } else { text.replace("⌘⏎", "⌘↩").replace("⌃⏎", "⌃↩") };
    tx(&text)
}

/// Eine Rückfrage, die auf ihren Platz wartet, mit dem, was bei „Ja“ geschieht.
struct Pending {
    spec: Spec,
    on_confirm: Option<Box<dyn FnOnce(&mut Shell, &mut Window, &mut Context<Shell>)>>,
}

/// Zustand der restlichen Funktionen, ein Feld der Shell.
pub struct Misc {
    pub help: Option<Entity<HelpView>>,
    pub confirm: Option<Entity<ConfirmDialog>>,
    queue: VecDeque<Pending>,
    action: Option<Box<dyn FnOnce(&mut Shell, &mut Window, &mut Context<Shell>)>>,
    /// Neuere Version laut `latest.json`, Pille in der Leiste.
    pub update: Option<UpdateManifest>,
    /// Claude-Sessions, die interaktiv in anderen Terminals laufen (Hinweis im Leerzustand).
    pub other_interactive: usize,
    idle: IdleTracker,
}

impl Misc {
    pub fn new() -> Misc {
        Misc { help: None, confirm: None, queue: VecDeque::new(), action: None, update: None, other_interactive: 0, idle: IdleTracker::default() }
    }

    /// Hält ein Dialog der restlichen Funktionen die Tastatur?
    pub fn overlay_open(&self) -> bool {
        self.help.is_some() || self.confirm.is_some()
    }
}

impl Default for Misc {
    fn default() -> Misc {
        Misc::new()
    }
}

/// Tastenbelegung: Hilfe nach den Einstellungen, F1 für „Über Kadrell“ überall außer im Terminal, dazu die Dialoge.
pub fn bind_keys(cx: &mut App, settings: &Settings) {
    help_view::bind_keys(cx);
    confirm::bind_keys(cx);
    let mac = cfg!(target_os = "macos");
    let help = keys::binding(settings, HotkeyAction::ShowHelp, mac).and_then(|k| keys::keystroke(&k, mac));
    let help = help.and_then(|stroke| KeyBinding::load(&stroke, Box::new(OpenHelp), None, false, None, cx).ok());
    cx.bind_keys(help.into_iter().chain([KeyBinding::new("f1", OpenAbout, Some("!KadrellTerminal"))]));
}

/// Aktionen des Fensters (Menü, Tastatur).
pub fn attach_actions(root: Div, cx: &mut Context<Shell>) -> Div {
    root.on_action(cx.listener(|this, _: &OpenHelp, window, cx| this.toggle_help(Kind::Help, window, cx)))
        .on_action(cx.listener(|this, _: &OpenAbout, window, cx| this.toggle_help(Kind::About, window, cx)))
        .on_action(cx.listener(|this, _: &OpenWhatsNew, window, cx| this.open_whats_new(true, window, cx)))
}

/// Wie `KADRELL_DEV_OVERLAY` die Dialoge für Screenshots öffnet: `help`, `about`, `whatsnew`, `adopt`.
fn dev_overlay() -> Option<String> {
    std::env::var("KADRELL_DEV_OVERLAY").ok()
}

/// Ein Plan zum Ansehen, ohne `claude` aufzurufen (`KADRELL_DEV_OVERLAY=adopt`).
fn dev_plan() -> AdoptPlan {
    let agent = |id: &str, name: &str| kadrell_claude::agent::AgentInfo {
        short_id: Some(id.into()),
        cwd: "C:/development/projects/kadrell".into(),
        kind: "background".into(),
        started_at: 0.,
        session_id: format!("dev-{id}"),
        name: name.into(),
        state: None,
        status: None,
        pid: Some(1),
        waiting_for: None,
        status_updated_at: None,
    };
    let candidates = vec![
        AdoptCandidate { agent: agent("a1b2c3d4", "api-umbau"), busy: true },
        AdoptCandidate { agent: agent("e5f6a7b8", "docs-pruefen"), busy: false },
    ];
    AdoptPlan { other_interactive_count: 2, candidates }
}

impl Shell {
    /// Beschriftete Belegung von „Hilfe“ (`⌥F1`, unter Windows `Alt+F1`), `None` ohne Belegung.
    pub(crate) fn help_shortcut(&self) -> Option<String> {
        let mac = cfg!(target_os = "macos");
        keys::binding(&self.state.settings, HotkeyAction::ShowHelp, mac).map(|k| keys::label(&k, mac))
    }

    // MARK: Hilfe und Über

    pub(crate) fn toggle_help(&mut self, kind: Kind, window: &mut Window, cx: &mut Context<Self>) {
        if self.misc.help.as_ref().is_some_and(|h| h.read(cx).kind() == kind) {
            return self.close_help(window, cx);
        }
        if self.misc.help.is_some() {
            self.close_help(window, cx);
        }
        self.open_help_view(kind, window, cx);
    }

    fn open_help_view(&mut self, kind: Kind, window: &mut Window, cx: &mut Context<Self>) {
        let settings = self.state.settings.clone();
        let view = cx.new(|cx| HelpView::new(kind, &settings, window, cx));
        cx.subscribe_in(&view, window, |this, _, _: &HelpEvent, window, cx| this.close_help(window, cx)).detach();
        self.misc.help = Some(view);
        cx.notify();
    }

    fn close_help(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.misc.help = None;
        self.sync(window, cx);
    }

    // MARK: Rückfragen

    /// Zeigt die Rückfrage, sobald keine andere offen ist; `on_confirm` läuft bei Bestätigung.
    pub(crate) fn ask(&mut self, spec: Spec, on_confirm: Option<Box<dyn FnOnce(&mut Shell, &mut Window, &mut Context<Shell>)>>, window: &mut Window, cx: &mut Context<Self>) {
        self.misc.queue.push_back(Pending { spec, on_confirm });
        self.show_next_confirm(window, cx);
    }

    fn show_next_confirm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.misc.confirm.is_some() {
            return;
        }
        let Some(Pending { spec, on_confirm }) = self.misc.queue.pop_front() else { return };
        let dialog = cx.new(|cx| ConfirmDialog::new(spec, window, cx));
        cx.subscribe_in(&dialog, window, |this, _, ev: &ConfirmEvent, window, cx| this.answered(*ev, window, cx)).detach();
        self.misc.confirm = Some(dialog);
        self.misc.action = on_confirm;
        cx.notify();
    }

    fn answered(&mut self, answer: ConfirmEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.misc.confirm = None;
        let action = self.misc.action.take();
        if let (ConfirmEvent::Confirmed, Some(action)) = (answer, action) {
            action(self, window, cx);
        }
        self.sync(window, cx);
        self.show_next_confirm(window, cx);
    }

    /// Meldung mit einer Taste, ohne Abbrechen.
    fn tell(&mut self, title: String, text: String, button: String, window: &mut Window, cx: &mut Context<Self>) {
        self.ask(Spec { title, text, button, destructive: false, notice: true }, None, window, cx);
    }

    // MARK: Was ist neu

    /// Nach dem Start: nach einem Versionssprung einmal zeigen, den ersten Start nur merken.
    fn show_whats_new_if_due(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let version = changelog::version();
        let due = whats_new::due_for(&self.state.settings, version, self.state.profile.temporary);
        if due == whats_new::Due::No {
            return;
        }
        whats_new::remember(&mut self.state.settings, version);
        let _ = self.state.profile.save_settings(&self.state.settings);
        if due == whats_new::Due::Show {
            self.open_whats_new(false, window, cx);
        }
    }

    /// `manual`: aus dem Menü, dann gibt es auch ohne Einträge eine Antwort.
    pub(crate) fn open_whats_new(&mut self, manual: bool, window: &mut Window, cx: &mut Context<Self>) {
        let version = changelog::version();
        let notes = if manual { whats_new::content(version, changelog::BUNDLED) } else { Some((version.to_string(), changelog::notes(version, changelog::BUNDLED))) };
        let (shown, lines) = notes.unwrap_or_default();
        if !lines.is_empty() {
            self.tell(trd!("Neu in %@", shown), lines.join("\n\n"), trd!("Super"), window, cx);
        } else if manual {
            self.tell(trd!("Neu in %@", version), trd!("Keine Einträge gefunden."), trd!("OK"), window, cx);
        }
    }

    // MARK: Update

    /// Prüft beim Start und danach alle 24 Stunden, ob eine neuere Version bereitsteht (nur die Prüfung, kein Download).
    fn start_update_check(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Ok(fake) = std::env::var("KADRELL_DEV_UPDATE") {
            self.misc.update = Some(UpdateManifest { version: fake, url: "https://kadrell.malura.de/download".into(), sha256: String::new(), notes: vec!["Feature: Beispiel-Eintrag".into()] });
            return;
        }
        cx.spawn_in(window, async move |this, cx| {
            loop {
                let Ok(allowed) = this.update(cx, |shell, _| shell.state.settings.check_for_updates()) else { break };
                let result = cx.background_executor().spawn(async move { update::check(&UreqClient, allowed, changelog::version()) }).await;
                let applied = this.update(cx, |shell, cx| {
                    match result {
                        Ok(found) if found != shell.misc.update => shell.misc.update = found,
                        Ok(_) => {}
                        Err(e) => eprintln!("kadrell: Update-Prüfung: {e}"),
                    }
                    cx.notify();
                });
                if applied.is_err() {
                    break;
                }
                cx.background_executor().timer(update::INTERVAL).await;
            }
        })
        .detach();
    }

    /// Klick auf die Pille: Changelog-Zeilen der neuen Version, „Herunterladen“ öffnet die Seite im Browser.
    pub(crate) fn show_update(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(m) = self.misc.update.clone() else { return };
        let url = m.url.clone();
        let spec = Spec { title: trd!("Kadrell %@ verfügbar", m.version.clone()), text: update::dialog_text(&m), button: trd!("Herunterladen"), destructive: false, notice: false };
        let open = Box::new(move |_: &mut Shell, _: &mut Window, _: &mut Context<Shell>| {
            if let Err(e) = open::run(&open::Opening::Url(url)) {
                eprintln!("kadrell: {e}");
            }
        });
        self.ask(spec, Some(open), window, cx);
    }

    /// Die Pille für die Leiste, `None` ohne neuere Version.
    pub(crate) fn update_pill(&self, cx: &mut Context<Self>) -> Option<crate::ui::usage_bar::UpdatePill> {
        let m = self.misc.update.as_ref()?;
        Some(crate::ui::usage_bar::UpdatePill { version: m.version.clone(), on_click: std::rc::Rc::new(cx.listener(|this, _, window, cx| this.show_update(window, cx))) })
    }

    // MARK: Hintergrund-Sessions übernehmen

    /// Beim Start: fremde Claude-Sessions erfassen. Hintergrund-Sessions bietet Kadrell zur Übernahme an (nur im
    /// Standardprofil), die anderen zählt der Leerzustand.
    fn start_adopt(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if dev_overlay().as_deref() == Some("adopt") {
            return self.offer_adopt(dev_plan(), None, window, cx);
        }
        if let Some(n) = std::env::var("KADRELL_DEV_ELSEWHERE").ok().and_then(|v| v.parse().ok()) {
            self.misc.other_interactive = n;
            return;
        }
        let Some(cli) = self.state.env.claude.clone() else { return };
        let owned = adopt::owned_ids(&self.state.store.sessions.sessions);
        let default_profile = self.state.profile.name.is_none() && !self.state.profile.temporary;
        let scan_cli = cli.clone();
        cx.spawn_in(window, async move |this, cx| {
            let plan = cx.background_executor().spawn(async move { adopt::scan(&scan_cli, &SystemRunner, &owned, default_profile) }).await;
            let _ = this.update_in(cx, |shell, window, cx| match plan {
                Ok(plan) => shell.offer_adopt(plan, Some(cli), window, cx),
                Err(e) => eprintln!("kadrell: agents: {e}"),
            });
        })
        .detach();
    }

    fn offer_adopt(&mut self, plan: AdoptPlan, cli: Option<kadrell_claude::cli::ClaudeCli>, window: &mut Window, cx: &mut Context<Self>) {
        self.misc.other_interactive = plan.other_interactive_count;
        cx.notify();
        let Some(offer) = adopt::offer(&plan) else { return };
        let spec = Spec { title: offer.title, text: offer.text, button: trd!("Übernehmen"), destructive: offer.destructive, notice: false };
        // Ohne `cli` (Ansicht mit `KADRELL_DEV_OVERLAY=adopt`) bleibt die Bestätigung folgenlos.
        let run = cli.map(|cli| -> Box<dyn FnOnce(&mut Shell, &mut Window, &mut Context<Shell>)> { Box::new(move |shell, window, cx| shell.adopt_now(cli, plan, window, cx)) });
        self.ask(spec, run, window, cx);
    }

    /// `claude stop` je Session im Hintergrund, danach landen sie als eigene Kacheln im Baum (`--resume`).
    fn adopt_now(&mut self, cli: kadrell_claude::cli::ClaudeCli, plan: AdoptPlan, window: &mut Window, cx: &mut Context<Self>) {
        cx.spawn_in(window, async move |this, cx| {
            let (adopted, failures) = cx.background_executor().spawn(async move { kadrell_claude::background_shells::adopt(&cli, &SystemRunner, &plan) }).await;
            let _ = this.update_in(cx, |shell, window, cx| {
                for session in adopt::sessions_of(&adopted) {
                    shell.state.add_session(session, None, false, None);
                }
                if let Some((title, body)) = adopt::failure_report(&failures) {
                    shell.tell(title, body, trd!("OK"), window, cx);
                }
                shell.sync(window, cx);
            });
        })
        .detach();
    }

    // MARK: Fertige Sessions trennen

    /// Alle 30 Sekunden: was länger als die eingestellten Minuten fertig ist, verliert seinen Prozess.
    fn start_auto_detach(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor().timer(std::time::Duration::from_secs(30)).await;
                if this.update_in(cx, |shell, window, cx| shell.auto_detach_tick(window, cx)).is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    pub(crate) fn auto_detach_tick(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let keys = self.detachable_keys();
        let minutes = self.state.settings.auto_detach_minutes();
        let due = self.misc.idle.due(&keys, self.state.focused.as_deref(), now_secs(), minutes);
        if due.is_empty() {
            return;
        }
        eprintln!("kadrell: automatisch getrennt nach {minutes} min: {} Session(s)", due.len());
        for key in due {
            self.state.stop_tile(&key);
        }
        self.sync(window, cx);
    }

    // MARK: Links im Terminal

    /// Strg+Klick (Mac ⌘-Klick) auf Text in der Kachel von `key`.
    pub(crate) fn on_link(&mut self, key: &str, link: &str) {
        let cwd = self.state.session(key).map(|s| s.cwd.clone()).unwrap_or_default();
        let home = self.state.home();
        let editor = self.state.settings.editor_command();
        if let Some(opening) = open::plan(link, &open::base_for(&cwd, &home), &home, &editor)
            && let Err(e) = open::run(&opening)
        {
            eprintln!("kadrell: {e}");
        }
    }

    // MARK: Start

    /// Einmal nach dem Aufbau der Shell: Prüfungen und Schleifen starten, Dialoge für Screenshots öffnen.
    pub(crate) fn start_misc(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.start_update_check(window, cx);
        self.start_adopt(window, cx);
        self.start_auto_detach(window, cx);
        match dev_overlay().as_deref() {
            Some("help") => self.open_help_view(Kind::Help, window, cx),
            Some("about") => self.open_help_view(Kind::About, window, cx),
            Some("whatsnew") => self.open_whats_new(true, window, cx),
            _ => self.show_whats_new_if_due(window, cx),
        }
    }

    /// Beschriftung für die Menüeinträge: das Kürzel von „Über Kadrell“.
    pub(crate) fn about_shortcut() -> String {
        localize_shortcuts("F1")
    }
}
