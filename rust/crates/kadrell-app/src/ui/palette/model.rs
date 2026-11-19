//! Logik der Suche (⌘P / Strg+P) ohne Oberfläche: Modus aus dem Präfix, Trefferlisten für Sessions und Gruppen,
//! Kommandos, Text in allen Terminals und Verlauf. Port der Listenbildung aus `PaletteWindow.swift`.

use kadrell_i18n::trd;
use crate::lang::de;

use gpui::Action;

use crate::appearance::a11y;
use crate::menus;
use crate::remote::dialog::Connect;
use crate::ui::folders::fuzzy;
use crate::ui::layout::{Id, LayoutMode};
use crate::ui::model::Status;

use super::history::HistoryHit;

/// Obergrenze der Texttreffer, sonst wird die Liste bei Begriffen wie „e“ zäh.
pub const MAX_TEXT_HITS: usize = 300;
/// Kürzeste Eingabe der Textsuche.
pub const MIN_TEXT_QUERY: usize = 2;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    /// Sessions und Gruppen anspringen.
    Sessions,
    /// `>` Kommandos.
    Commands,
    /// `/` Text in allen laufenden Terminals.
    Text,
    /// `#` alle bisherigen Claude-Sessions.
    History,
    /// `@` ssh-Hosts, `@host:` deren tmux-Sessions.
    Remote,
}

impl Mode {
    pub fn prefix(self) -> &'static str {
        match self {
            Mode::Sessions => "",
            Mode::Commands => ">",
            Mode::Text => "/",
            Mode::History => "#",
            Mode::Remote => "@",
        }
    }
}

/// Eingabe zerlegt in Modus und Suchbegriff. Kommandos und Verlauf trimmen den Begriff, Text und Sessions nicht
/// (wie in der Mac-App).
#[derive(Clone, PartialEq, Debug)]
pub struct Query {
    pub mode: Mode,
    pub term: String,
}

pub fn parse(input: &str) -> Query {
    let rest = |n: usize| input[n..].to_string();
    match input.chars().next() {
        Some('>') => Query { mode: Mode::Commands, term: rest(1).trim().to_string() },
        Some('/') => Query { mode: Mode::Text, term: rest(1) },
        Some('#') => Query { mode: Mode::History, term: rest(1).trim().to_string() },
        Some('@') => Query { mode: Mode::Remote, term: rest(1) },
        _ => Query { mode: Mode::Sessions, term: input.to_string() },
    }
}

// --- Daten, die die Shell beim Öffnen liefert ---

#[derive(Clone)]
pub struct SessionRow {
    pub id: Id,
    pub title: String,
    pub status: Status,
    /// Ordner mit `/`, für die Suche.
    pub path: String,
    /// Ordner mit `~`, für die Anzeige.
    pub short: String,
    pub has_pane: bool,
    /// Letzte nicht leere Zeile des Terminals (Zusatzzeile der Trefferzeile), ohne Terminal `None`.
    pub last_line: Option<String>,
}

#[derive(Clone)]
pub struct GroupRow {
    pub index: usize,
    pub name: String,
    pub path: String,
    pub short: String,
    pub sessions: Vec<SessionRow>,
}

#[derive(Clone, Default)]
pub struct Snapshot {
    pub groups: Vec<GroupRow>,
    /// Fokussierte Session (Ziel der „fokussierte“-Kommandos).
    pub focused: Option<Id>,
    /// Konversationen (Claude-sessionId und Schlüssel), die in Kadrell laufen: Platz und Status, für die Verlaufssuche.
    pub live: std::collections::HashMap<String, (Id, Status)>,
    /// Konten (id, Titel) für „Zu Konto wechseln: …“.
    pub accounts: Vec<(String, String)>,
}

impl Snapshot {
    fn all_sessions(&self) -> impl Iterator<Item = (&GroupRow, &SessionRow)> {
        self.groups.iter().flat_map(|g| g.sessions.iter().map(move |s| (g, s)))
    }
}

// --- Kommandos ---

/// Was ein Kommando tut. `Unwired` steht für Befehle, deren Funktion in der Rust-App noch fehlt: sie erscheinen
/// gedämpft in der Liste und tun bei ⏎ nichts.
pub enum Run {
    /// Menü-Aktion, läuft über den Fokuspfad der Shell.
    Menu(Box<dyn Action>),
    Layout(LayoutMode),
    Group(usize),
    /// Die fokussierte Session in die Gruppe an dieser Position der Ansicht verschieben.
    MoveFocused(usize),
    /// Aktion eines Statusleisten-Eintrags einer Extension (Extension, Aktions-id).
    Ext { name: String, id: String },
    /// Wechselt in einen anderen Modus der Suche.
    Mode(Mode),
    /// „Trennlinien zurücksetzen (gleich verteilt)“.
    ResetRatios,
    /// „Reload“: sofort pollen und neu auslegen.
    Reload,
    /// Konto hinzufügen oder wechseln.
    Account(AccountRun),
    Unwired,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AccountRun {
    /// Das aktuell angemeldete Konto aufnehmen.
    AddCurrent,
    Switch(String),
}

pub struct Command {
    pub title: String,
    pub run: Run,
}

fn layout_title(mode: LayoutMode) -> &'static str {
    match mode {
        LayoutMode::Grid => de!("Grid"),
        LayoutMode::Main => de!("Haupt + Spalte"),
        LayoutMode::Spiral => de!("Spirale"),
        LayoutMode::Custom => de!("Frei"),
        LayoutMode::Scroll => de!("Scrollen"),
        LayoutMode::Row => de!("Reihe"),
        LayoutMode::Stack => de!("Stack"),
    }
}

/// Menüeinträge, aus denen sich die Kommandos bedienen: Beschriftung und Aktion bleiben an einer Stelle
/// (`menus.rs`). Was ein Kommando schon übernommen hat, fehlt in `rest`.
struct MenuPool(Vec<Option<MenuEntry>>);

/// Schlüssel (deutscher Quelltext), Beschriftung, Kürzel und Aktion eines Menüeintrags.
type MenuEntry = (&'static str, String, String, Option<Box<dyn Action>>);

impl MenuPool {
    fn new() -> Self {
        let mut all = Vec::new();
        for def in menus::menus() {
            for e in def.entries {
                if let menus::Entry::Item { key, label, shortcut, action } = e {
                    all.push(Some((key, label, shortcut, action)));
                }
            }
        }
        MenuPool(all)
    }

    /// Entnimmt den Eintrag mit diesem Schlüssel (deutscher Quelltext der Beschriftung, sprachunabhängig).
    fn take(&mut self, key: &str) -> Option<Option<Box<dyn Action>>> {
        let slot = self.0.iter_mut().find(|s| s.as_ref().is_some_and(|e| e.0 == key))?;
        slot.take().map(|(_, _, _, action)| action)
    }

}

/// Kommando, das ein Menüeintrag trägt (Kürzel und Aktion von dort); fehlt der Eintrag, ist es nicht verdrahtet.
fn via_menu(pool: &mut MenuPool, title: String, menu_key: &str) -> Command {
    let action = pool.take(menu_key).flatten();
    Command { title, run: action.map_or(Run::Unwired, Run::Menu) }
}

fn plain(title: String, run: Run) -> Command {
    Command { title, run }
}

fn focused_group(snap: &Snapshot) -> Option<usize> {
    snap.focused.map(|f| f.0)
}

/// Alle Kommandos in der Reihenfolge der Mac-App (`togglePalette`); die Kommandos der Extensions hängt die Shell an.
pub fn build_commands(snap: &Snapshot) -> Vec<Command> {
    let mut pool = MenuPool::new();
    let mut list: Vec<Command> =
        LayoutMode::ALL.iter().map(|m| plain(trd!("Layout: %@", trd!(layout_title(*m))), Run::Layout(*m))).collect();
    list.push(plain(trd!("Trennlinien zurücksetzen (gleich verteilt)"), Run::ResetRatios));
    list.push(via_menu(&mut pool, trd!("Zoom ein/aus (fokussierte)"), de!("Kachel zoomen")));
    list.push(via_menu(&mut pool, trd!("Neue Session"), de!("Neue Session")));
    list.push(via_menu(&mut pool, trd!("Schnellstart: Idee rein, Session raus"), de!("Schnellstart: Idee rein, Session raus")));
    list.push(via_menu(&mut pool, trd!("Alle Claude-Sessions durchsuchen, auch beendete"), de!("Alle Claude-Sessions durchsuchen, auch beendete")));
    list.push(via_menu(&mut pool, trd!("Neues Terminal ohne Claude"), de!("Neues Terminal ohne Claude")));
    list.push(via_menu(&mut pool, trd!("Remote verbinden (ssh → tmux)"), de!("Remote verbinden …")));
    list.push(via_menu(&mut pool, trd!("Neue Gruppe (leer)"), de!("Neue Gruppe …")));
    list.push(via_menu(&mut pool, trd!("Session stoppen (fokussierte)"), de!("Stoppen")));
    list.push(via_menu(&mut pool, trd!("Session fortsetzen (fokussierte)"), de!("Fortsetzen")));
    list.push(via_menu(&mut pool, trd!("Session umbenennen (fokussierte)"), de!("Umbenennen")));
    list.push(via_menu(&mut pool, trd!("Session schließen (fokussierte)"), de!("Session schließen")));
    list.push(via_menu(&mut pool, trd!("Gruppe bearbeiten (der fokussierten Session)"), de!("Gruppe umbenennen")));
    list.push(via_menu(&mut pool, trd!("Statistik"), de!("Statistik")));
    list.push(via_menu(&mut pool, trd!("Extensions"), de!("Extensions …")));
    list.push(plain(trd!("Reload"), Run::Reload));
    list.push(plain(trd!("Neu starten"), Run::Menu(Box::new(menus::RestartApp))));
    list.push(plain(trd!("Konto hinzufügen (aktuell angemeldetes)"), Run::Account(AccountRun::AddCurrent)));
    list.extend(snap.accounts.iter().map(|(id, title)| plain(trd!("Zu Konto wechseln: %@", title), Run::Account(AccountRun::Switch(id.clone())))));
    list.extend(snap.groups.iter().map(|g| plain(trd!("Alle Sessions von %@", &g.name), Run::Group(g.index))));
    if let Some(own) = focused_group(snap) {
        let others = snap.groups.iter().filter(|g| g.index != own);
        list.extend(others.map(|g| plain(trd!("Fokussierte Session verschieben nach: %@", &g.name), Run::MoveFocused(g.index))));
    }
    list
}

// --- Trefferzeilen ---

#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    Session(Id),
    Group(usize),
    /// Index in die Kommandoliste.
    Command(usize),
    /// Treffer in einem Terminal: Session, Gridzeile, Suchbegriff.
    Hit { id: Id, line: i32, query: String },
    /// ⏎ im `@`-Modus: mit dem Host verbinden (und der tmux-Session).
    Connect(Connect),
    /// Frühere Claude-Session (sessionId, Ordner, Titel), zum Fortsetzen oder Anspringen.
    History { session_id: String, cwd: String, title: String },
    /// Zeile ohne Wirkung (Hinweis).
    Nothing,
}

#[derive(Clone)]
pub struct Item {
    pub label: String,
    pub sub: String,
    /// Rechts oben: Gruppe, Kürzel oder Alter.
    pub right: Option<String>,
    pub status: Option<Status>,
    pub indent: u8,
    /// Gedämpft: Befehl ohne Funktion.
    pub dim: bool,
    /// Die Session der Zeile (`sessionKey` der Mac-App): Grundlage der Hervorhebung in Kacheln und Baum.
    pub session: Option<Id>,
    pub target: Target,
}

impl Item {
    pub(super) fn new(label: impl Into<String>, sub: impl Into<String>, target: Target) -> Self {
        Item { label: label.into(), sub: sub.into(), right: None, status: None, indent: 0, dim: false, session: None, target }
    }

    /// Wie VoiceOver die Zeile liest: Titel, Status, Gruppe, Zusatzzeile, mit „, “ getrennt (`Item.spoken`).
    pub fn spoken(&self) -> String {
        let status = self.status.map(|s| a11y::spoken(s, false, false));
        let parts = [Some(self.label.clone()), status, self.right.clone(), Some(self.sub.clone())];
        parts.into_iter().flatten().collect::<Vec<_>>().join(", ")
    }
}

/// Sessions, die die Palette hervorhebt (`onHighlight`): Treffer der Sessions-Suche mit Suchbegriff, Zeilen der
/// Textsuche und laufende Sessions im Verlauf. Kommandos, `@` und die leere Sessions-Liste heben nichts hervor.
pub fn highlight(query: &Query, items: &[Item]) -> Option<std::collections::HashSet<Id>> {
    let on = match query.mode {
        Mode::Commands | Mode::Remote => false,
        Mode::Sessions => !query.term.is_empty(),
        Mode::Text | Mode::History => true,
    };
    on.then(|| items.iter().filter_map(|i| i.session).collect())
}

// --- Sessions und Gruppen ---

/// Zusatzzeile einer Session: die letzte Zeile des Terminals (höchstens 70 Zeichen), sonst der Kurzpfad.
fn session_sub(s: &SessionRow) -> String {
    match &s.last_line {
        Some(line) => line.chars().take(70).collect(),
        None => s.short.clone(),
    }
}

fn session_item(g: &GroupRow, s: &SessionRow, indent: u8, show_group: bool) -> Item {
    let mut it = Item::new(s.title.clone(), session_sub(s), Target::Session(s.id));
    it.status = Some(s.status);
    it.session = Some(s.id);
    it.indent = indent;
    it.right = show_group.then(|| g.name.clone());
    it
}

fn group_item(g: &GroupRow) -> Item {
    Item::new(format!("▾ {}", g.name), g.short.clone(), Target::Group(g.index))
}

fn session_matches(q: &str, s: &SessionRow) -> bool {
    fuzzy(q, &format!("{} {}", s.title, s.path))
}

/// Ohne Suchbegriff: flache Liste, wer auf dich wartet zuerst (stabil sortiert).
fn flat_sessions(snap: &Snapshot) -> Vec<Item> {
    let mut all: Vec<_> = snap.all_sessions().collect();
    all.sort_by_key(|(_, s)| u8::from(s.status != Status::Waiting));
    all.into_iter().map(|(g, s)| session_item(g, s, 0, true)).collect()
}

/// Mit Suchbegriff: nach Gruppen gebündelt. Passt der Gruppenname, stehen alle ihre Sessions eingerückt darunter,
/// sonst nur die selbst passenden.
fn grouped_sessions(snap: &Snapshot, q: &str) -> Vec<Item> {
    let mut out = Vec::new();
    let named: Vec<bool> = snap.groups.iter().map(|g| fuzzy(q, &g.name)).collect();
    for (g, _) in snap.groups.iter().zip(&named).filter(|(_, n)| **n) {
        out.push(group_item(g));
        out.extend(g.sessions.iter().map(|s| session_item(g, s, 1, false)));
    }
    for (g, _) in snap.groups.iter().zip(&named).filter(|(_, n)| !**n) {
        let hits: Vec<_> = g.sessions.iter().filter(|s| session_matches(q, s)).collect();
        if hits.is_empty() {
            continue;
        }
        out.push(group_item(g));
        out.extend(hits.into_iter().map(|s| session_item(g, s, 1, false)));
    }
    out
}

pub fn session_items(snap: &Snapshot, q: &str) -> Vec<Item> {
    if q.is_empty() { flat_sessions(snap) } else { grouped_sessions(snap, q) }
}

// --- Kommandos filtern ---

pub fn command_items(commands: &[Command], term: &str) -> Vec<Item> {
    let shown = commands.iter().enumerate().filter(|(_, c)| term.is_empty() || fuzzy(term, &c.title));
    shown
        .map(|(i, c)| {
            let mut it = Item::new(c.title.clone(), trd!("Kommando"), Target::Command(i));
            it.dim = matches!(c.run, Run::Unwired);
            it
        })
        .collect()
}

// --- Text in allen Terminals ---

/// Zeilen eines Terminals, die den Begriff enthalten (Gridzeile, Text).
pub struct SessionHits {
    pub id: Id,
    pub lines: Vec<(i32, String)>,
}

/// Eine Zeile je Treffer wie in der Mac-App: die Zeile als Titel, darunter die Session, rechts die Gruppe.
fn text_row(g: &GroupRow, s: &SessionRow, line: i32, text: &str, query: &str) -> Item {
    let mut it = Item::new(text.trim(), s.title.clone(), Target::Hit { id: s.id, line, query: query.to_string() });
    it.status = Some(s.status);
    it.session = Some(s.id);
    it.right = Some(g.name.clone());
    it
}

/// Treffer zeilenweise in der Reihenfolge der Sessions, Gesamtzahl auf `MAX_TEXT_HITS` begrenzt.
pub fn text_items(snap: &Snapshot, hits: &[SessionHits], query: &str) -> Vec<Item> {
    let mut out = Vec::new();
    for h in hits {
        let Some((g, s)) = snap.all_sessions().find(|(_, s)| s.id == h.id) else { continue };
        let room = MAX_TEXT_HITS - out.len();
        out.extend(h.lines.iter().take(room).map(|(line, text)| text_row(g, s, *line, text, query)));
        if out.len() >= MAX_TEXT_HITS {
            break;
        }
    }
    out
}

// --- Verlauf ---

/// „vor 3 Std.“ wie `RelativeDateTimeFormatter` (kurz), in der eingestellten Sprache.
pub fn relative_de(secs_ago: u64) -> String {
    const UNITS: [(u64, &str); 6] = [
        (31_536_000, de!("vor %lld J.")),
        (2_592_000, de!("vor %lld Mon.")),
        (604_800, de!("vor %lld Wo.")),
        (86_400, de!("vor %lld Tg.")),
        (3_600, de!("vor %lld Std.")),
        (60, de!("vor %lld Min.")),
    ];
    for (size, text) in UNITS {
        if secs_ago >= size {
            return trd!(text, secs_ago / size);
        }
    }
    trd!("gerade eben")
}

pub fn history_item(hit: &HistoryHit, now_secs: u64, live: Option<(Id, Status)>) -> Item {
    let sub = hit.snippet.clone().unwrap_or_else(|| hit.short.clone());
    let target = Target::History { session_id: hit.session_id.clone(), cwd: hit.cwd.clone(), title: hit.title.clone() };
    let mut it = Item::new(hit.title.clone(), sub, target);
    it.right = Some(format!("{} · {}", hit.project, relative_de(now_secs.saturating_sub(hit.last_active))));
    it.status = live.map(|l| l.1);
    it.session = live.map(|l| l.0);
    it
}

/// Einzelne Zeile ohne Wirkung mit Meldung.
pub fn notice_item(title: &str, message: &str) -> Item {
    Item::new(title, message, Target::Nothing)
}

pub fn history_placeholder(ready: bool) -> Vec<Item> {
    if ready {
        return Vec::new();
    }
    vec![Item::new(
        trd!("Verlauf wird eingelesen …"),
        trd!("beim ersten Mal einmalig alle Transcripts, danach nur neue"),
        Target::Nothing,
    )]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(g: usize, s: usize, title: &str, status: Status, path: &str) -> SessionRow {
        SessionRow {
            id: (g, s),
            title: title.into(),
            status,
            path: path.into(),
            short: path.replace("C:/dev", "~"),
            has_pane: true,
            last_line: None,
        }
    }

    fn snap() -> Snapshot {
        let kad = GroupRow {
            index: 0,
            name: "kadrell".into(),
            path: "C:/dev/kadrell".into(),
            short: "~/kadrell".into(),
            sessions: vec![
                session(0, 0, "claude", Status::Running, "C:/dev/kadrell"),
                session(0, 1, "shell", Status::Idle, "C:/dev/kadrell"),
            ],
        };
        let spike = GroupRow {
            index: 1,
            name: "spike".into(),
            path: "C:/dev/spike".into(),
            short: "~/spike".into(),
            sessions: vec![
                session(1, 0, "claude-zwei", Status::Waiting, "C:/dev/spike"),
                session(1, 1, "zweite shell", Status::Detached, "C:/dev/spike"),
            ],
        };
        Snapshot { groups: vec![kad, spike], focused: Some((0, 0)), ..Snapshot::default() }
    }

    fn labels(items: &[Item]) -> Vec<&str> {
        items.iter().map(|i| i.label.as_str()).collect()
    }

    #[test]
    fn parse_picks_mode_from_prefix() {
        assert_eq!(parse("").mode, Mode::Sessions);
        assert_eq!(parse("kad").mode, Mode::Sessions);
        assert_eq!(parse("> neue gruppe"), Query { mode: Mode::Commands, term: "neue gruppe".into() });
        assert_eq!(parse("/ Claude"), Query { mode: Mode::Text, term: " Claude".into() });
        assert_eq!(parse("#  x "), Query { mode: Mode::History, term: "x".into() });
        assert_eq!(parse(">").term, "");
        assert_eq!(parse("@vault: x"), Query { mode: Mode::Remote, term: "vault: x".into() });
    }

    #[test]
    fn first_character_decides_the_mode() {
        // Nur das erste Zeichen zählt: ein späteres Präfix ist normaler Text.
        assert_eq!(parse("a>b").mode, Mode::Sessions);
        assert_eq!(parse(">/#@").mode, Mode::Commands);
        assert_eq!(parse("/#@").mode, Mode::Text);
        assert_eq!(parse("#@").mode, Mode::History);
        assert_eq!(parse(" @x").mode, Mode::Sessions);
    }

    #[test]
    fn prefix_round_trips() {
        for m in [Mode::Commands, Mode::Text, Mode::History, Mode::Remote] {
            assert_eq!(parse(&format!("{}abc", m.prefix())).mode, m);
        }
    }

    #[test]
    fn empty_query_lists_waiting_first() {
        let items = session_items(&snap(), "");
        assert_eq!(labels(&items), ["claude-zwei", "claude", "shell", "zweite shell"]);
        assert_eq!(items[0].right.as_deref(), Some("spike"));
        assert!(items.iter().all(|i| i.indent == 0));
    }

    #[test]
    fn group_name_match_shows_all_its_sessions_indented() {
        let items = session_items(&snap(), "kadr");
        assert_eq!(labels(&items), ["▾ kadrell", "claude", "shell"]);
        assert_eq!(items[1].indent, 1);
        assert_eq!(items[1].right, None);
        assert_eq!(items[0].target, Target::Group(0));
    }

    #[test]
    fn session_match_shows_group_header_and_only_hits() {
        let items = session_items(&snap(), "zwei");
        assert_eq!(labels(&items), ["▾ spike", "claude-zwei", "zweite shell"]);
        let items = session_items(&snap(), "claude");
        assert_eq!(labels(&items), ["▾ kadrell", "claude", "▾ spike", "claude-zwei"]);
        assert!(session_items(&snap(), "qqq").is_empty());
    }

    #[test]
    fn commands_filter_fuzzy_and_show_no_shortcut() {
        let cmds = build_commands(&snap());
        let items = command_items(&cmds, "neue gruppe");
        assert!(labels(&items).contains(&"Neue Gruppe (leer)"));
        let new = command_items(&cmds, "neue session");
        assert_eq!(new[0].label, "Neue Session");
        assert!(new[0].right.is_none(), "Swift zeigt rechts kein Kürzel");
        assert!(!new[0].dim);
        let all = command_items(&cmds, "");
        assert_eq!(all.len(), cmds.len());
    }

    #[test]
    fn commands_cover_layouts_groups_and_moves() {
        let cmds = build_commands(&snap());
        let titles: Vec<&str> = cmds.iter().map(|c| c.title.as_str()).collect();
        assert!(titles.contains(&"Layout: Spirale"));
        assert!(titles.contains(&"Alle Sessions von spike"));
        assert!(titles.contains(&"Fokussierte Session verschieben nach: spike"));
        assert!(!titles.contains(&"Fokussierte Session verschieben nach: kadrell"));
    }

    #[test]
    fn commands_follow_the_mac_order_and_nothing_else() {
        let mut snap = snap();
        snap.accounts = vec![("a1".into(), "Privat".into()), ("a2".into(), "Arbeit".into())];
        let titles: Vec<String> = build_commands(&snap).into_iter().map(|c| c.title).collect();
        let layouts: Vec<String> = LayoutMode::ALL.iter().map(|m| format!("Layout: {}", layout_title(*m))).collect();
        let fixed = [
            "Trennlinien zurücksetzen (gleich verteilt)",
            "Zoom ein/aus (fokussierte)",
            "Neue Session",
            "Schnellstart: Idee rein, Session raus",
            "Alle Claude-Sessions durchsuchen, auch beendete",
            "Neues Terminal ohne Claude",
            "Remote verbinden (ssh → tmux)",
            "Neue Gruppe (leer)",
            "Session stoppen (fokussierte)",
            "Session fortsetzen (fokussierte)",
            "Session umbenennen (fokussierte)",
            "Session schließen (fokussierte)",
            "Gruppe bearbeiten (der fokussierten Session)",
            "Statistik",
            "Extensions",
            "Reload",
            "Neu starten",
            "Konto hinzufügen (aktuell angemeldetes)",
            "Zu Konto wechseln: Privat",
            "Zu Konto wechseln: Arbeit",
            "Alle Sessions von kadrell",
            "Alle Sessions von spike",
            "Fokussierte Session verschieben nach: spike",
        ];
        let expected: Vec<String> = layouts.into_iter().chain(fixed.map(String::from)).collect();
        assert_eq!(titles, expected);
    }

    #[test]
    fn every_command_is_wired_and_restart_runs_the_shell_restart() {
        let cmds = build_commands(&snap());
        let dim: Vec<&str> = cmds.iter().filter(|c| matches!(c.run, Run::Unwired)).map(|c| c.title.as_str()).collect();
        assert!(dim.is_empty(), "{dim:?}");
        assert!(cmds.iter().any(|c| c.title == "Neu starten" && matches!(c.run, Run::Menu(_))));
        assert!(command_items(&cmds, "Reload").iter().all(|i| !i.dim));
        let run_of = |t: &str| cmds.iter().find(|c| c.title == t).map(|c| &c.run);
        assert!(matches!(run_of("Trennlinien zurücksetzen (gleich verteilt)"), Some(Run::ResetRatios)));
        assert!(matches!(run_of("Reload"), Some(Run::Reload)));
        assert!(matches!(run_of("Konto hinzufügen (aktuell angemeldetes)"), Some(Run::Account(AccountRun::AddCurrent))));
    }

    #[test]
    fn session_sub_is_the_last_terminal_line_cut_to_70_chars() {
        let mut snap = snap();
        snap.groups[0].sessions[0].last_line = Some("x".repeat(90));
        let items = session_items(&snap, "");
        let item = items.iter().find(|i| i.label == "claude").unwrap();
        assert_eq!(item.sub.chars().count(), 70);
        let other = items.iter().find(|i| i.label == "shell").unwrap();
        assert_eq!(other.sub, "~/kadrell", "ohne Terminalzeile der Kurzpfad");
    }

    #[test]
    fn spoken_joins_title_status_group_and_sub() {
        let items = session_items(&snap(), "");
        assert_eq!(items[0].spoken(), "claude-zwei, wartet, spike, ~/spike");
        let note = notice_item("Titel", "Meldung");
        assert_eq!(note.spoken(), "Titel, Meldung");
    }

    #[test]
    fn highlight_follows_mode_and_query() {
        let snap = snap();
        let ids = |q: &str, items: &[Item]| highlight(&parse(q), items);
        let flat = session_items(&snap, "");
        assert_eq!(ids("", &flat), None, "leere Suche hebt nichts hervor");
        let hits = session_items(&snap, "zwei");
        assert_eq!(ids("zwei", &hits), Some([(1, 0), (1, 1)].into_iter().collect()), "Gruppenzeilen haben keinen Schlüssel");
        assert_eq!(ids(">neu", &flat), None);
        assert_eq!(ids("@vault", &flat), None);
        assert_eq!(ids("/xx", &[]), Some(Default::default()), "Textsuche ohne Treffer: leere Menge, nicht nil");
        let lines = vec![SessionHits { id: (1, 0), lines: vec![(0, "claude".into())] }];
        let text = text_items(&snap, &lines, "claude");
        assert_eq!(ids("/claude", &text), Some([(1, 0)].into_iter().collect()));
    }

    #[test]
    fn history_rows_of_live_sessions_are_highlighted() {
        let hit = HistoryHit {
            session_id: "s1".into(),
            title: "Titel".into(),
            cwd: "C:/dev/x".into(),
            short: "~/x".into(),
            project: "x".into(),
            last_active: 0,
            snippet: None,
        };
        let live = history_item(&hit, 100, Some(((0, 1), Status::Running)));
        let gone = history_item(&hit, 100, None);
        assert_eq!(highlight(&parse("#"), &[live, gone]), Some([(0, 1)].into_iter().collect()));
    }

    #[test]
    fn text_hits_are_one_row_per_line() {
        let hits = vec![
            SessionHits { id: (0, 0), lines: vec![(-3, "  hello Claude here \t".into()), (5, "claude again".into())] },
            SessionHits { id: (0, 1), lines: vec![] },
            SessionHits { id: (1, 0), lines: vec![(2, "Claude!".into())] },
        ];
        let items = text_items(&snap(), &hits, "claude");
        assert_eq!(labels(&items), ["hello Claude here", "claude again", "Claude!"]);
        assert_eq!((items[0].sub.as_str(), items[0].right.as_deref()), ("claude", Some("kadrell")));
        assert_eq!((items[2].sub.as_str(), items[2].right.as_deref()), ("claude-zwei", Some("spike")));
        assert_eq!(items[0].status, Some(Status::Running));
        assert_eq!(items[0].target, Target::Hit { id: (0, 0), line: -3, query: "claude".into() });
        assert!(items.iter().all(|i| i.indent == 0));
    }

    #[test]
    fn text_hits_are_capped_at_300_rows() {
        let lines: Vec<(i32, String)> = (0..400).map(|i| (i, "x claude".to_string())).collect();
        let hits = vec![SessionHits { id: (0, 0), lines }, SessionHits { id: (1, 0), lines: vec![(0, "claude".into())] }];
        let items = text_items(&snap(), &hits, "claude");
        assert_eq!(items.len(), MAX_TEXT_HITS);
        let split = vec![
            SessionHits { id: (0, 0), lines: (0..200).map(|i| (i, "claude".to_string())).collect() },
            SessionHits { id: (1, 0), lines: (0..200).map(|i| (i, "claude".to_string())).collect() },
        ];
        assert_eq!(text_items(&snap(), &split, "claude").len(), MAX_TEXT_HITS, "zweiter Treffersatz füllt nur den Rest");
    }

    #[test]
    fn relative_time_is_short_german() {
        assert_eq!(relative_de(5), "gerade eben");
        assert_eq!(relative_de(300), "vor 5 Min.");
        assert_eq!(relative_de(3 * 3600 + 5), "vor 3 Std.");
        assert_eq!(relative_de(2 * 86_400), "vor 2 Tg.");
    }
}
