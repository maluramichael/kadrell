//! Dialog „Remote verbinden“ (⌘⇧N / Strg+Umschalt+N), der `@`-Modus der Palette der Mac-App: `text` filtert die Hosts
//! (zuletzt benutzte vorn, danach `~/.ssh/config`), ⏎ hängt sich per ssh an deren laufende tmux. `host:` listet die
//! tmux-Sessions des Hosts (geholt per ssh, bis dahin „lädt …“) und oben „Neu: text“, ⏎ legt die Session an oder
//! hängt sich an.

use std::collections::HashMap;

use gpui::{
    App, Context, Entity, EventEmitter, FocusHandle, Focusable, InteractiveElement, IntoElement, KeyBinding, ParentElement, Render,
    ScrollHandle, StatefulInteractiveElement, Styled, Subscription, Window, actions, div, prelude::*, px,
};
use kadrell_i18n::trd;

use super::{Query, host_choices, parse_query, system_ssh, tmux_sessions};
use crate::Shell;
use crate::lang::tx;
use crate::ui::chrome::h_seam;
use crate::ui::folder_row::{FolderRow, folder_row};
use crate::ui::folders::fuzzy;
use crate::ui::dialog_keys;
use crate::ui::instrument::KeyRole;
use crate::ui::overlay::{backdrop, dialog_foot_in, dialog_frame, dialog_title, overlay_height, overlay_width};
use crate::ui::text_input::{TextInput, TextInputEvent};
use crate::ui::theme;

actions!(kadrell_remote, [MoveUp, MoveDown, Confirm, Cancel]);

const CONTEXT: &str = "KadrellRemote";

pub fn bind_keys(cx: &mut App) {
    let c = Some(CONTEXT);
    dialog_keys::bind_keys(cx);
    cx.bind_keys([
        KeyBinding::new("up", MoveUp, c),
        KeyBinding::new("down", MoveDown, c),
        KeyBinding::new("enter", Confirm, c),
        KeyBinding::new("escape", Cancel, c),
    ]);
}

/// Stand der tmux-Sessions eines Hosts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Sessions {
    Loading,
    Found(Vec<String>),
    /// ssh ging nicht (Host nicht erreichbar, Passwort nötig).
    Unreachable,
}

/// Verbinden mit `host`, mit dieser tmux-Session (`None`: an die laufende hängen).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Connect {
    pub host: String,
    pub tmux: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    pub label: String,
    pub sub: String,
    /// `None`: nur ein Hinweis, ⏎ tut nichts.
    pub connect: Option<Connect>,
}

/// Zustand ohne Oberfläche.
#[derive(Clone, Debug)]
pub struct RemoteState {
    hosts: Vec<String>,
    pub query: String,
    pub sessions: HashMap<String, Sessions>,
    pub selected: usize,
}

impl RemoteState {
    /// `recent`: zuletzt verbundene Hosts, `config`: Hosts aus `~/.ssh/config`.
    pub fn new(recent: &[String], config: &[String]) -> RemoteState {
        RemoteState { hosts: host_choices(recent, config), query: String::new(), sessions: HashMap::new(), selected: 0 }
    }

    pub fn set_query(&mut self, query: &str) {
        self.query = query.to_string();
        self.selected = 0;
    }

    /// Host, dessen tmux-Sessions gerade gebraucht werden und noch nicht geholt sind.
    pub fn wanted_sessions(&self) -> Option<String> {
        match parse_query(&self.query) {
            Query::Sessions { host, .. } if !host.is_empty() && !self.sessions.contains_key(&host) => Some(host),
            _ => None,
        }
    }

    pub fn items(&self) -> Vec<Item> {
        match parse_query(&self.query) {
            Query::Hosts(q) => self.host_items(&q),
            Query::Sessions { host, text } => self.session_items(&host, &text),
        }
    }

    fn host_items(&self, q: &str) -> Vec<Item> {
        let hint = |h: &str| trd!("ssh · ⏎ tmux attach · „%@:“ wählt die Session", h);
        self.hosts.iter().filter(|h| q.is_empty() || fuzzy(q, h)).map(|h| Item { label: h.clone(), sub: hint(h), connect: Some(Connect { host: h.clone(), tmux: None }) }).collect()
    }

    fn session_items(&self, host: &str, text: &str) -> Vec<Item> {
        let mut list = Vec::new();
        if !text.is_empty() {
            list.push(Item { label: trd!("Neu: %@", text), sub: trd!("tmux new -As auf %@", host), connect: Some(Connect { host: host.into(), tmux: Some(text.into()) }) });
        }
        match self.sessions.get(host) {
            Some(Sessions::Found(names)) if names.is_empty() => list.push(note(trd!("kein tmux-Server auf %@", host), trd!("„Neu: …“ versucht es trotzdem"))),
            Some(Sessions::Unreachable) => list.push(note(trd!("%@ nicht erreichbar", host), trd!("„Neu: …“ versucht es trotzdem"))),
            Some(Sessions::Found(names)) => {
                let shown = names.iter().filter(|n| text.is_empty() || fuzzy(text, n));
                list.extend(shown.map(|n| Item { label: n.clone(), sub: trd!("tmux-Session auf %@", host), connect: Some(Connect { host: host.into(), tmux: Some(n.clone()) }) }));
            }
            _ => list.push(note(trd!("lädt …"), trd!("tmux ls auf %@", host))),
        }
        list
    }

    pub fn move_by(&mut self, delta: isize) {
        let last = self.items().len().saturating_sub(1) as isize;
        self.selected = (self.selected as isize + delta).clamp(0, last) as usize;
    }

    /// Was ⏎ verbindet; `None` bei einem Hinweis oder leerer Liste.
    pub fn target(&self) -> Option<Connect> {
        self.items().get(self.selected).and_then(|i| i.connect.clone())
    }
}

fn note(label: String, sub: String) -> Item {
    Item { label, sub, connect: None }
}

// MARK: Oberfläche

pub enum RemoteEvent {
    Close,
    Connect(Connect),
}

impl EventEmitter<RemoteEvent> for RemoteDialog {}

pub struct RemoteDialog {
    input: Entity<TextInput>,
    state: RemoteState,
    scroll: ScrollHandle,
    _typing: Subscription,
}

impl RemoteDialog {
    pub fn new(state: RemoteState, window: &mut Window, cx: &mut Context<Self>) -> RemoteDialog {
        let input = cx.new(|cx| TextInput::new(&trd!("Remote verbinden …"), cx));
        let typing = cx.subscribe(&input, |this, input, _: &TextInputEvent, cx| {
            let text = input.read(cx).text().to_string();
            this.state.set_query(&text);
            this.load_sessions(cx);
            cx.notify();
        });
        let focus = input.read(cx).focus.clone();
        focus.focus(window, cx);
        RemoteDialog { input, state, scroll: ScrollHandle::new(), _typing: typing }
    }

    /// `host:` tippen: die tmux-Sessions per ssh holen (Hintergrund), die Liste füllt sich danach.
    fn load_sessions(&mut self, cx: &mut Context<Self>) {
        let Some(host) = self.state.wanted_sessions() else { return };
        self.state.sessions.insert(host.clone(), Sessions::Loading);
        cx.spawn(async move |this, cx| {
            let ask = host.clone();
            let found = cx.background_executor().spawn(async move { tmux_sessions(&system_ssh(), &ask) }).await;
            this.update(cx, |this, cx| {
                let result = found.map_or(Sessions::Unreachable, Sessions::Found);
                this.state.sessions.insert(host, result);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn confirm(&mut self, _: &Confirm, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(target) = self.state.target() {
            cx.emit(RemoteEvent::Connect(target));
        }
    }

    fn move_by(&mut self, delta: isize, cx: &mut Context<Self>) {
        self.state.move_by(delta);
        self.scroll.scroll_to_item(self.state.selected);
        cx.notify();
    }
}

impl Render for RemoteDialog {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme(cx);
        let items = self.state.items();
        let rows: Vec<_> = items
            .iter()
            .enumerate()
            .map(|(i, item)| {
                let row = FolderRow { name: item.label.clone(), path: item.sub.clone(), right: Vec::new() };
                let target = item.connect.clone();
                folder_row(("remote-row", i), row, i == self.state.selected, &t, cx.listener(move |_, _, _, cx| {
                    if let Some(c) = target.clone() {
                        cx.emit(RemoteEvent::Connect(c));
                    }
                }))
            })
            .collect();
        let empty = items.is_empty().then(|| div().px(px(16.)).py(px(16.)).text_color(t.muted).child(tx(&trd!("Nichts gefunden."))));
        let list = div().id("remote-list").flex().flex_col().flex_1().min_h_0().overflow_y_scroll().track_scroll(&self.scroll).children(empty).children(rows);
        let on_foot = cx.listener(|this, _, w, cx| this.confirm(&Confirm, w, cx));
        let foot = dialog_foot_in(trd!("⏎ starten · ↑↓ wählt · Esc abbrechen"), trd!("Starten"), KeyRole::Primary, &t, window, cx, on_foot);
        let dialog = overlay_height(overlay_width(dialog_frame(&t), 560.), 420.)
            .key_context(CONTEXT)
            .text_color(t.fg)
            .on_action(cx.listener(|this, _: &MoveUp, _, cx| this.move_by(-1, cx)))
            .on_action(cx.listener(|this, _: &MoveDown, _, cx| this.move_by(1, cx)))
            .on_action(cx.listener(Self::confirm))
            .on_action(cx.listener(|_, _: &Cancel, _, cx| cx.emit(RemoteEvent::Close)))
            .child(dialog_title(trd!("Remote verbinden (ssh → tmux)"), &t))
            .child(div().flex_none().p(px(14.)).child(self.input.clone()))
            .child(h_seam(&t))
            .child(list)
            .child(foot);
        dialog_keys::nav(backdrop(cx.listener(|_, _, _, cx| cx.emit(RemoteEvent::Close)))).child(dialog)
    }
}

impl Focusable for RemoteDialog {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.input.read(cx).focus.clone()
    }
}

// MARK: Anbindung an die Shell

impl Shell {
    /// ⌘⇧N / Strg+Umschalt+N: öffnet den Dialog, solange kein anderes Overlay offen ist.
    pub fn open_remote(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.remote.is_some() || self.overlay.is_some() || self.palette.is_some() || self.settings.is_some() || self.quick_start.is_some() || self.agent_pick.is_some() {
            return;
        }
        let hosts = self.state.ssh_host_list();
        let state = RemoteState::new(&hosts.recent, &hosts.hosts);
        let dialog = cx.new(|cx| RemoteDialog::new(state, window, cx));
        cx.subscribe_in(&dialog, window, |this, _, ev: &RemoteEvent, window, cx| {
            this.remote = None;
            if let RemoteEvent::Connect(c) = ev {
                this.state.new_remote_session(&c.host, c.tmux.as_deref(), true);
            }
            this.sync(window, cx);
        })
        .detach();
        self.remote = Some(dialog);
        cx.notify();
    }

    /// Für Screenshots ohne Eingabe: `KADRELL_DEV_OVERLAY=remote`, Hosts aus `~/.ssh/config`, Eingabe aus `KADRELL_DEV_QUERY`.
    pub fn open_remote_from_env(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if std::env::var("KADRELL_DEV_OVERLAY").as_deref() != Ok("remote") {
            return;
        }
        self.open_remote(window, cx);
        if let (Some(dialog), Ok(query)) = (self.remote.clone(), std::env::var("KADRELL_DEV_QUERY")) {
            let input = dialog.read(cx).input.clone();
            input.update(cx, |i, cx| i.set_text(&query, window, cx));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    fn state(query: &str) -> RemoteState {
        let mut s = RemoteState::new(&v(&["vault"]), &v(&["box", "vault", "ragasa"]));
        s.set_query(query);
        s
    }

    #[test]
    fn hosts_list_has_recent_first_and_filters_fuzzily() {
        let s = state("");
        assert_eq!(s.items().iter().map(|i| i.label.as_str()).collect::<Vec<_>>(), ["vault", "box", "ragasa"]);
        assert_eq!(state("rg").items().iter().map(|i| i.label.as_str()).collect::<Vec<_>>(), ["ragasa"]);
        assert_eq!(s.target(), Some(Connect { host: "vault".into(), tmux: None }));
        assert!(state("zzz").target().is_none());
    }

    #[test]
    fn colon_asks_for_the_sessions_once() {
        let mut s = state("vault:");
        assert_eq!(s.wanted_sessions().as_deref(), Some("vault"));
        s.sessions.insert("vault".into(), Sessions::Loading);
        assert_eq!(s.wanted_sessions(), None, "nur einmal je Host");
        let items = s.items();
        assert_eq!(items[0].label, "lädt …");
        assert!(items[0].connect.is_none());
        assert_eq!(state("vault").wanted_sessions(), None, "ohne Doppelpunkt kein ssh");
    }

    #[test]
    fn sessions_are_filtered_and_new_comes_first() {
        let mut s = state("vault: ma");
        s.sessions.insert("vault".into(), Sessions::Found(v(&["main", "work", "mail"])));
        let items = s.items();
        assert_eq!(items.iter().map(|i| i.label.as_str()).collect::<Vec<_>>(), ["Neu: ma", "main", "mail"]);
        assert_eq!(s.target(), Some(Connect { host: "vault".into(), tmux: Some("ma".into()) }));
        s.move_by(1);
        assert_eq!(s.target(), Some(Connect { host: "vault".into(), tmux: Some("main".into()) }));
        s.move_by(99);
        assert_eq!(s.selected, 2);
    }

    #[test]
    fn empty_and_unreachable_hosts_say_so_but_new_still_works() {
        let mut s = state("vault:x");
        s.sessions.insert("vault".into(), Sessions::Found(vec![]));
        let items = s.items();
        assert_eq!(items[1].label, "kein tmux-Server auf vault");
        assert!(items[1].connect.is_none());
        assert!(items[0].connect.is_some(), "„Neu: x“ versucht es trotzdem");
        s.sessions.insert("vault".into(), Sessions::Unreachable);
        assert_eq!(s.items()[1].label, "vault nicht erreichbar");
        let mut bare = state("vault:");
        bare.sessions.insert("vault".into(), Sessions::Unreachable);
        assert!(bare.target().is_none(), "ohne Text gibt es nur den Hinweis");
    }
}
