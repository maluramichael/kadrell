//! Anbindung der Suche an die `Shell`: Momentaufnahme der Sessions beim Öffnen, Öffnen und Schließen, Wirkung der
//! Auswahl (Session zeigen, Gruppe, Layout, Menü-Aktion, Treffer im Terminal).

use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::Duration;

use gpui::{AppContext, Context, Entity, Window};

use super::model::{AccountRun, GroupRow, SessionRow, Snapshot};
use super::remote::HostSource;
use super::{HistoryAction, Known, Pick, decide_history, history_provider, problem_title};
use crate::ui::model::Status;
use kadrell_control::{ControlHandler, NewRequest};
use super::{Palette, PaletteEvent};
use crate::Shell;
use crate::ui::folders;
use crate::ui::layout::Selection;
use crate::ui::model::Group;

fn path_of(s: &crate::ui::model::Session) -> String {
    s.cwd.replace('\\', "/")
}

/// Letzte nicht leere Zeile des Bildschirms ohne Leerraum am Ende (`snapshotRows(trimTrailing: true).last`).
fn last_line(rows: &[String]) -> Option<String> {
    rows.iter().rev().map(|r| r.trim_end()).find(|r| !r.is_empty()).map(String::from)
}

/// Was die Palette vom Shell-Zustand braucht, ohne Verweis auf die Shell: Fokus, Home, laufende Sessions, letzte
/// Terminalzeile je Session und Konten.
struct Inputs {
    focused: Option<(usize, usize)>,
    home: String,
    live: HashMap<String, ((usize, usize), Status)>,
    last_lines: HashMap<(usize, usize), String>,
    accounts: Vec<(String, String)>,
}

/// Gruppen und Sessions in der Form, die die Palette braucht (ohne Verweis auf die Shell).
fn snapshot(groups: &[Group], c: Inputs) -> Snapshot {
    let Inputs { focused, home, live, last_lines, accounts } = c;
    let home = home.as_str();
    let rows = groups.iter().enumerate().map(|(g, group)| {
        let sessions: Vec<SessionRow> = group
            .sessions
            .iter()
            .enumerate()
            .map(|(s, session)| {
                let path = path_of(session);
                SessionRow {
                    id: (g, s),
                    title: session.title.clone(),
                    status: session.status,
                    short: folders::short_path(&path, home),
                    path,
                    has_pane: session.pane.is_some(),
                    last_line: last_lines.get(&(g, s)).cloned(),
                }
            })
            .collect();
        let path = if group.cwd.is_empty() { sessions.first().map(|s| s.path.clone()).unwrap_or_default() } else { group.cwd.replace('\\', "/") };
        GroupRow { index: g, name: group.name.clone(), short: folders::short_path(&path, home), path, sessions }
    });
    Snapshot { groups: rows.collect(), focused, live, accounts }
}

impl Shell {
    /// Strg+P / ⌘P: öffnet die Suche. Ist sie schon offen, schaltet es auf `>` (wie in der Mac-App); mit
    /// Präfix (Strg+Umschalt+F: `/`) wechselt die offene Suche in diesen Modus.
    pub fn open_palette(&mut self, prefix: &str, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(open) = self.palette.clone() {
            if prefix.is_empty() {
                open.update(cx, |p, cx| p.switch_to_commands(window, cx));
                return;
            }
            // Mit Präfix öffnet sie neu in diesem Modus (`dismiss(runHighlightReset: false)`, dann frisch).
            self.palette = None;
        }
        self.dismiss_sheets();
        let panes: HashMap<_, _> = self
            .groups
            .iter()
            .enumerate()
            .flat_map(|(g, group)| group.sessions.iter().enumerate().map(move |(s, session)| ((g, s), session)))
            .filter_map(|(id, session)| Some((id, session.pane.clone()?)))
            .collect();
        let snap = snapshot(&self.groups, self.palette_inputs(&panes, cx));
        let history = history_provider(&self.history);
        let hosts = self.host_source();
        let palette = cx.new(|cx| Palette::new(snap, panes, history, hosts, prefix, window, cx));
        let ext = self.ext_palette_commands();
        palette.update(cx, |p, cx| p.add_commands(ext, cx));
        self.palette_highlight = palette.read(cx).highlight().map(|ids| self.keys_of(&ids));
        cx.subscribe_in(&palette, window, Self::on_palette_event).detach();
        self.palette = Some(palette);
        self.refresh_history(cx);
        cx.notify();
    }

    /// Host-Gruppe: kein Claude dort, das „+“ öffnet die Auswahl der tmux-Sessions des Hosts (`@host:`).
    pub fn pick_remote_session(&mut self, group_id: &str, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(host) = self.state.store.groups.group(group_id).and_then(|g| g.host.clone()) else { return false };
        self.open_palette(&format!("@{host}:"), window, cx);
        true
    }

    /// Andere Dialoge schließen, bevor die Suche aufgeht (`sheets.dismiss()`).
    fn dismiss_sheets(&mut self) {
        self.overlay = None;
        self.quick_start = None;
        self.agent_pick = None;
        self.remote = None;
        self.settings = None;
    }

    /// Fokus, laufende Sessions, letzte Terminalzeilen und Konten für die Momentaufnahme.
    fn palette_inputs(&self, panes: &HashMap<(usize, usize), gpui::Entity<kadrell_term::TerminalPane>>, cx: &Context<Self>) -> Inputs {
        let last_lines = panes.iter().filter_map(|(id, pane)| Some((*id, last_line(&pane.read(cx).handle().visible_rows())?))).collect();
        let accounts = cx.try_global::<crate::usage::backend::HubGlobal>().map(|h| h.0.view().accounts).unwrap_or_default();
        Inputs {
            focused: self.selected,
            home: folders::home_dir(),
            live: self.live_conversations(),
            last_lines,
            accounts: accounts.into_iter().map(|a| (a.id, a.title)).collect(),
        }
    }

    /// Hosts für `@`: zuletzt verbundene vorn, danach `~/.ssh/config`; gelesen wird erst beim ersten `@`.
    fn host_source(&self) -> HostSource {
        let (home, recent) = (self.state.home(), self.state.recent_hosts());
        Rc::new(move || {
            let hosts = kadrell_claude::ssh_config::hosts(&kadrell_claude::ssh_config::default_config(&home), &home);
            (recent.clone(), hosts)
        })
    }

    /// Schlüssel der Sessions an diesen Plätzen (die Hervorhebung kennt Schlüssel, nicht Plätze).
    fn keys_of(&self, ids: &HashSet<(usize, usize)>) -> HashSet<String> {
        ids.iter().filter_map(|(g, s)| Some(self.groups.get(*g)?.sessions.get(*s)?.key.clone())).collect()
    }

    /// Ist diese Session gerade Treffer der Suche? Kacheln und Baum zeichnen sich damit hervor, solange die Suche offen
    /// ist (`None`: keine Hervorhebung, alle gleich).
    pub fn palette_highlights(&self, key: &str) -> Option<bool> {
        self.palette_highlight.as_ref().map(|keys| keys.contains(key))
    }

    /// Verlaufsindex nachlesen (beim Start und bei jedem Öffnen der Suche); danach filtert eine offene Verlaufssuche neu.
    pub fn refresh_history(&mut self, cx: &mut Context<Self>) {
        let config_dir = self.state.env.config_dir();
        let cache = super::history::cache_file(&config_dir, &self.state.home());
        self.history.refresh(config_dir, cache, cx, |shell: &mut Shell, cx| {
            if let Some(palette) = shell.palette.clone() {
                palette.update(cx, |p, cx| p.refresh_history(cx));
            }
        });
    }

    /// Konversationen, die in Kadrell laufen (sessionId und Schlüssel), mit Platz im Baum und Status.
    fn live_conversations(&self) -> HashMap<String, ((usize, usize), Status)> {
        let mut live = HashMap::new();
        for (g, group) in self.groups.iter().enumerate() {
            for (s, session) in group.sessions.iter().enumerate() {
                let entry = ((g, s), session.status);
                if let Some(core) = self.state.session(&session.key) {
                    live.insert(core.session_id.clone(), entry);
                }
                live.insert(session.key.clone(), entry);
            }
        }
        live
    }

    /// Platz einer Session im Baum.
    fn position_of(&self, key: &str) -> Option<(usize, usize)> {
        self.groups.iter().enumerate().find_map(|(g, group)| Some((g, group.sessions.iter().position(|s| s.key == key)?)))
    }

    /// ⏎ auf einem Treffer des Verlaufs: zur Session springen, wenn sie in Kadrell läuft, sonst `claude --resume` im Ordner
    /// ihrer ersten Eingabe (derselbe Weg wie `kadrell new --resume`). Geht es nicht, bleibt die Suche mit Meldung offen.
    fn resume_history(&mut self, pick: &Pick, window: &mut Window, cx: &mut Context<Self>) {
        let known: Vec<Known> = self.state.store.sessions.sessions.iter().map(|s| Known { key: s.id.clone(), session_id: s.session_id.clone() }).collect();
        let folder_ok = !pick.cwd.is_empty() && std::path::Path::new(&pick.cwd).is_dir();
        let blocker = self.state.resume_blocker(&pick.session_id);
        match decide_history(pick, &known, folder_ok, blocker, &self.state.home()) {
            HistoryAction::Jump(key) => {
                self.close_palette(window, cx);
                if let Some(id) = self.position_of(&key) {
                    self.show_only(id, window, cx);
                }
            }
            HistoryAction::Resume { session_id, cwd } => {
                self.close_palette(window, cx);
                self.start_resumed(&session_id, &cwd, window, cx);
            }
            HistoryAction::Problem(message) => {
                if let Some(palette) = self.palette.clone() {
                    palette.update(cx, |p, cx| p.show_notice(&problem_title(), &message, cx));
                }
            }
        }
    }

    fn start_resumed(&mut self, session_id: &str, cwd: &str, window: &mut Window, cx: &mut Context<Self>) {
        let request = NewRequest { cwd: cwd.replace('\\', "/"), show: true, resume: Some(session_id.to_string()), ..NewRequest::default() };
        if let Err(e) = self.state.new_session(request) {
            self.report(&e.to_string(), window, cx);
        }
        self.sync(window, cx);
    }

    /// Für Screenshots ohne Eingabe: `KADRELL_DEV_OVERLAY=palette`, Text aus `KADRELL_DEV_QUERY` (mit Präfix),
    /// `KADRELL_DEV_DELAY_MS` wartet vorher, bis die Terminals etwas ausgegeben haben.
    pub fn open_palette_from_env(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if std::env::var("KADRELL_DEV_OVERLAY").as_deref() != Ok("palette") {
            return;
        }
        let query = std::env::var("KADRELL_DEV_QUERY").unwrap_or_default();
        let delay = std::env::var("KADRELL_DEV_DELAY_MS").ok().and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
        if delay == 0 {
            self.open_for_dev(&query, window, cx);
            return;
        }
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(delay)).await;
            let _ = this.update_in(cx, |shell, window, cx| shell.open_for_dev(&query, window, cx));
        })
        .detach();
    }

    /// Öffnet die Suche und führt auf Wunsch gleich die Zeile `KADRELL_DEV_ACTIVATE=<n>` aus (⏎ ohne Tastatur).
    fn open_for_dev(&mut self, query: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.open_palette(query, window, cx);
        let row = std::env::var("KADRELL_DEV_ACTIVATE").ok().and_then(|v| v.parse::<usize>().ok());
        if let (Some(row), Some(open)) = (row, self.palette.clone()) {
            open.update(cx, |p, cx| p.activate(row, window, cx));
        }
    }

    /// Schließt die Suche; die Tastatur geht zurück an das Terminal der gewählten Session.
    fn close_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.palette = None;
        self.palette_highlight = None;
        self.sync(window, cx);
    }

    /// Zeigt nur diese Session (wie `workspace.select([key], add: false)`).
    fn show_only(&mut self, id: (usize, usize), window: &mut Window, cx: &mut Context<Self>) {
        self.selection = Selection::single(id);
        self.apply_selection(window, cx);
    }

    fn on_palette_event(&mut self, _: &Entity<Palette>, ev: &PaletteEvent, window: &mut Window, cx: &mut Context<Self>) {
        match ev {
            PaletteEvent::Close => self.close_palette(window, cx),
            PaletteEvent::Session(id) => {
                self.close_palette(window, cx);
                self.show_only(*id, window, cx);
            }
            PaletteEvent::Group(g) => {
                self.close_palette(window, cx);
                if self.groups.get(*g).is_some_and(|gr| !gr.sessions.is_empty()) {
                    self.select_group(*g, window, cx);
                }
            }
            PaletteEvent::MoveFocused(g) => {
                self.close_palette(window, cx);
                self.move_focused_to(*g, window, cx);
            }
            PaletteEvent::Layout(mode) => {
                self.close_palette(window, cx);
                self.set_mode(*mode, cx);
            }
            PaletteEvent::Menu(action) => {
                self.close_palette(window, cx);
                window.dispatch_action(action.boxed_clone(), cx);
            }
            PaletteEvent::Hit { id, line, query } => {
                self.close_palette(window, cx);
                self.show_only(*id, window, cx);
                self.reveal_hit(*id, query, *line, cx);
            }
            PaletteEvent::Resume(pick) => self.resume_history(pick, window, cx),
            PaletteEvent::Ext { name, id } => {
                self.close_palette(window, cx);
                self.ext_action(name, id);
            }
            PaletteEvent::Connect(c) => {
                self.close_palette(window, cx);
                self.state.new_remote_session(&c.host, c.tmux.as_deref(), true);
                self.sync(window, cx);
            }
            PaletteEvent::ResetRatios => {
                self.close_palette(window, cx);
                self.reset_ratios(cx);
            }
            PaletteEvent::Reload => {
                self.close_palette(window, cx);
                self.reload_now(window, cx);
            }
            PaletteEvent::Account(run) => {
                self.close_palette(window, cx);
                self.account_command(run, cx);
            }
            PaletteEvent::Highlight(ids) => self.palette_highlight = ids.as_ref().map(|ids| self.keys_of(ids)),
        }
    }

    /// „Trennlinien zurücksetzen“: alle gezogenen Verhältnisse weg, auch aus den Einstellungen (`resetRatios`).
    fn reset_ratios(&mut self, cx: &mut Context<Self>) {
        for key in self.layout.reset_all() {
            self.persist_ratio(&key);
        }
        cx.notify();
    }

    /// „Reload“: sofort pollen und neu auslegen (`registry.pollNow()`, `workspace.relayout()`).
    fn reload_now(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let job = self.state.poll_job();
        cx.spawn_in(window, async move |this, cx| {
            let (change, messages) = cx.background_executor().spawn(async move { job.run_counted() }).await;
            let _ = this.update_in(cx, |shell, window, cx| {
                shell.state.count_stat(kadrell_core::stats::StatKey::Messages, messages as i64);
                shell.state.apply_registry(change);
                shell.sync(window, cx);
            });
        })
        .detach();
    }

    /// „Konto hinzufügen“ und „Zu Konto wechseln“: Aufträge an den Hub der Konten.
    fn account_command(&mut self, run: &AccountRun, cx: &mut Context<Self>) {
        let Some(hub) = cx.try_global::<crate::usage::backend::HubGlobal>().map(|h| h.0.clone()) else { return };
        hub.request(match run {
            AccountRun::AddCurrent => crate::usage::hub::Cmd::AddCurrent,
            AccountRun::Switch(id) => crate::usage::hub::Cmd::Switch(id.clone()),
        });
    }

    /// Scrollt das Terminal zur Treffer-Zeile und öffnet dort seine Suchleiste mit dem Begriff.
    fn reveal_hit(&mut self, id: (usize, usize), query: &str, line: i32, cx: &mut Context<Self>) {
        let pane = self.groups.get(id.0).and_then(|g| g.sessions.get(id.1)).and_then(|s| s.pane.clone());
        if let Some(pane) = pane {
            pane.update(cx, |p, cx| p.reveal_line(query, line, cx));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn last_line_skips_blank_rows_and_trailing_space() {
        let rows: Vec<String> = ["alt", "  fertig  ", "", "   "].map(String::from).to_vec();
        assert_eq!(last_line(&rows).as_deref(), Some("  fertig"));
        assert_eq!(last_line(&[String::new()]), None);
        assert_eq!(last_line(&[]), None);
    }
}
