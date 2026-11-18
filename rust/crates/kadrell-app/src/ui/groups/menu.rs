//! Inhalt der Kontextmenüs von Session- und Gruppenzeilen, Eintrag für Eintrag wie `AppDelegate+SessionMenu.swift` und
//! `AppDelegate+GroupMenu.swift`. Die Bauer sind reine Funktionen über Daten (Gruppen, Session, Auswahl), damit die
//! Regeln (was ist wann ausgegraut, wie heißen gleichnamige Gruppen) ohne Oberfläche testbar sind.

use kadrell_core::group::Group;
use kadrell_core::Session;
use kadrell_i18n::trd;

use crate::ui::context_menu::{Cmd, Item};
use crate::ui::folders::short_path;
use crate::ui::model::parse_color;

/// Alles, was das Menü einer Session wissen muss.
pub struct SessionMenu<'a> {
    pub session: &'a Session,
    /// Läuft ihr Prozess?
    pub running: bool,
    /// Gerade sichtbare Sessions (Auswahl).
    pub selected: &'a [String],
    /// Gruppen in Baumreihenfolge.
    pub groups: &'a [Group],
    /// Hat die Gruppe einen Editor-Befehl in den Einstellungen?
    pub editor: bool,
    /// Terminal der Gruppe darin sichtbar (für „Alle Sessions zeigen“): Schlüssel, die es noch gibt.
    pub known: &'a dyn Fn(&str) -> bool,
    pub home: &'a str,
}

fn entry(label: String, cmd: Cmd) -> Item {
    Item::Entry(label, cmd)
}

/// Ein Eintrag, der nur unter einer Bedingung wirkt, sonst ausgegraut.
fn when(enabled: bool, label: String, cmd: Cmd) -> Item {
    if enabled { Item::Entry(label, cmd) } else { Item::Off(label) }
}

/// Eintrag „Im Finder/Explorer zeigen“.
pub fn reveal_label() -> String {
    if cfg!(target_os = "macos") { trd!("Im Finder zeigen") } else { trd!("Im Explorer zeigen") }
}

/// Beschriftung der Gruppe in „Verschieben nach“: gleichnamige tragen ihren Ordner dahinter, sonst wären sie nicht
/// zu unterscheiden.
fn move_label(g: &Group, groups: &[Group], home: &str) -> String {
    let same = groups.iter().filter(|o| o.name == g.name).count();
    if same < 2 {
        return g.name.clone();
    }
    let place = g.host.as_ref().map_or_else(|| short_path(&g.cwd, home), |h| format!("ssh {h}"));
    format!("{} · {place}", g.name)
}

/// Einträge für „Verschieben nach“: alle Gruppen in Baumreihenfolge. Wo die Sessions schon stehen, ist der Eintrag
/// abgehakt und aus.
pub fn move_targets(ids: &[String], groups: &[Group], home: &str) -> Vec<Item> {
    groups
        .iter()
        .map(|g| {
            let here = !ids.is_empty() && ids.iter().all(|id| g.session_ids.contains(id));
            let cmd = Cmd::MoveMany { keys: ids.to_vec(), group: g.id.clone() };
            Item::Flag { label: move_label(g, groups, home), cmd, checked: here, enabled: !here, dot: Some(parse_color(&g.color)) }
        })
        .collect()
}

/// Untermenü „Verschieben nach“; ohne Sessions ein grauer Hinweis.
pub fn move_submenu(label: String, ids: &[String], groups: &[Group], home: &str) -> Item {
    if ids.is_empty() {
        return Item::Sub(label, vec![Item::Off(trd!("Keine Session fokussiert"))]);
    }
    Item::Sub(label, move_targets(ids, groups, home))
}

/// Kontextmenü einer Session samt „Auswahl verschieben“ und dem Untermenü ihrer Gruppe.
pub fn session_menu(m: &SessionMenu) -> Vec<Item> {
    let key = m.session.id.clone();
    let has_cwd = !m.session.cwd.is_empty();
    let in_view = m.selected.contains(&key);
    let mut items = vec![
        when(!m.running, trd!("Fortsetzen"), Cmd::Resume(key.clone())),
        when(m.running, trd!("Stoppen"), Cmd::Stop(key.clone())),
        entry(trd!("Entfernen …"), Cmd::Remove(key.clone())),
        Item::Separator,
        entry(trd!("Nur diese zeigen"), Cmd::ShowOnly(key.clone())),
        when(in_view, trd!("Aus Ansicht nehmen"), Cmd::HideFromView(key.clone())),
        when(has_cwd, trd!("Neue Session hier"), Cmd::NewSessionHere(key.clone())),
        move_submenu(trd!("Verschieben nach"), std::slice::from_ref(&key), m.groups, m.home),
    ];
    if m.selected.len() > 1 && in_view {
        let label = trd!("Auswahl verschieben nach (%lld)", m.selected.len());
        items.push(move_submenu(label, m.selected, m.groups, m.home));
    }
    items.push(Item::Separator);
    items.push(when(has_cwd, trd!("Pfad kopieren"), Cmd::CopyPath(m.session.cwd.clone())));
    items.push(when(has_cwd, reveal_label(), Cmd::Reveal(m.session.cwd.clone())));
    if let Some(g) = m.groups.iter().find(|g| g.session_ids.contains(&key)) {
        items.push(Item::Separator);
        items.push(Item::Sub(trd!("Gruppe „%@“", &g.name), group_menu(g, m.editor, m.known)));
    }
    items
}

/// Kontextmenü einer Gruppe. Remote-Gruppen haben keinen lokalen Ordner: dort fehlen Terminal, Pfad, Explorer und Editor.
pub fn group_menu(g: &Group, editor: bool, known: &dyn Fn(&str) -> bool) -> Vec<Item> {
    let local = g.host.is_none();
    let has_sessions = g.session_ids.iter().any(|id| known(id));
    let mut items = Vec::new();
    if local {
        items.push(entry(trd!("Neue Session in dieser Gruppe"), Cmd::NewSessionIn(g.id.clone())));
        items.push(entry(trd!("Neues Terminal in dieser Gruppe"), Cmd::NewTerminalIn(g.id.clone())));
    } else {
        // Öffnet die Suche mit `@host:` (tmux-Sessions des Hosts): `NewSessionIn` einer Host-Gruppe führt dorthin
        // (`pick_remote_session`).
        items.push(entry(trd!("Remote-Session wählen …"), Cmd::NewSessionIn(g.id.clone())));
    }
    items.push(when(has_sessions, trd!("Alle Sessions zeigen"), Cmd::ShowGroup(g.id.clone())));
    items.push(Item::Separator);
    items.push(entry(trd!("Bearbeiten …"), Cmd::EditGroup(g.id.clone())));
    items.push(Item::Flag { label: trd!("Favorit"), cmd: Cmd::ToggleFavorite(g.id.clone()), checked: g.favorite, enabled: true, dot: None });
    if local {
        items.push(Item::Separator);
        items.push(entry(trd!("Pfad kopieren"), Cmd::CopyPath(g.cwd.clone())));
        items.push(entry(reveal_label(), Cmd::Reveal(g.cwd.clone())));
        items.push(when(editor, trd!("Im Editor öffnen"), Cmd::OpenEditor(g.cwd.clone())));
    }
    items.push(Item::Separator);
    items.push(entry(trd!("Gruppe schließen …"), Cmd::CloseGroup(g.id.clone())));
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    fn group(id: &str, name: &str, cwd: &str, ids: &[&str]) -> Group {
        Group::new(id, name, "#fab387", cwd, ids)
    }

    fn labels(items: &[Item]) -> Vec<String> {
        items
            .iter()
            .map(|i| match i {
                Item::Separator => "-".to_string(),
                Item::Entry(l, _) => format!("+{l}"),
                Item::Off(l) => format!("({l})"),
                Item::Flag { label, checked, enabled, .. } => format!("{}{}{label}", if *checked { "v" } else { "" }, if *enabled { "+" } else { "(" }),
                Item::Sub(l, sub) => format!("{l} > {}", sub.len()),
            })
            .collect()
    }

    fn session(id: &str, cwd: &str) -> Session {
        Session::new(id, cwd, 1.0, id, "Titel")
    }

    fn known(_: &str) -> bool {
        true
    }

    #[test]
    fn session_menu_entries_and_enabling() {
        let groups = vec![group("g1", "kadrell", "C:/p/kadrell", &["a", "b"]), group("g2", "spike", "C:/p/spike", &[])];
        let s = session("a", "C:/p/kadrell");
        let selected = vec!["a".to_string()];
        let m = SessionMenu { session: &s, running: true, selected: &selected, groups: &groups, editor: false, known: &known, home: "C:/Users/m" };
        let l = labels(&session_menu(&m));
        assert_eq!(&l[..3], ["(Fortsetzen)", "+Stoppen", "+Entfernen …"]);
        assert_eq!(&l[4..8], ["+Nur diese zeigen", "+Aus Ansicht nehmen", "+Neue Session hier", "Verschieben nach > 2"]);
        assert!(l.contains(&"Gruppe „kadrell“ > 12".to_string()), "{l:?}");
        let stopped = SessionMenu { running: false, selected: &[], ..m };
        let l = labels(&session_menu(&stopped));
        assert_eq!(&l[..2], ["+Fortsetzen", "(Stoppen)"]);
        assert_eq!(l[5], "(Aus Ansicht nehmen)");
    }

    #[test]
    fn move_targets_mark_current_group_and_disambiguate_names() {
        let groups = vec![group("g1", "app", "C:/Users/m/a/app", &["a"]), group("g2", "app", "C:/Users/m/b/app", &[]), group("g3", "x", "C:/x", &[])];
        let items = move_targets(&["a".to_string()], &groups, "C:/Users/m");
        assert_eq!(labels(&items), ["v(app · ~/a/app", "+app · ~/b/app", "+x"]);
        let Item::Flag { cmd, dot, .. } = &items[1] else { panic!() };
        assert_eq!(cmd, &Cmd::MoveMany { keys: vec!["a".into()], group: "g2".into() });
        assert_eq!(*dot, Some(0xfab387));
    }

    #[test]
    fn multi_selection_adds_its_own_move_submenu() {
        let groups = vec![group("g1", "kadrell", "C:/p/kadrell", &["a", "b"]), group("g2", "spike", "C:/p/spike", &["c"])];
        let s = session("a", "C:/p/kadrell");
        let selected: Vec<String> = ["a", "b", "c"].iter().map(|s| s.to_string()).collect();
        let m = SessionMenu { session: &s, running: false, selected: &selected, groups: &groups, editor: false, known: &known, home: "" };
        let items = session_menu(&m);
        let l = labels(&items);
        assert_eq!(l[7], "Verschieben nach > 2");
        assert_eq!(l[8], "Auswahl verschieben nach (3) > 2");
        let Item::Sub(_, sub) = &items[8] else { panic!() };
        // Nur die Gruppe, in der schon alle stehen, wäre abgehakt: hier keine.
        assert!(sub.iter().all(|i| matches!(i, Item::Flag { checked: false, enabled: true, .. })));
    }

    #[test]
    fn group_menu_local_and_remote() {
        let local = group("g1", "kadrell", "C:/p/kadrell", &["a"]);
        let l = labels(&group_menu(&local, true, &known));
        assert_eq!(
            l,
            ["+Neue Session in dieser Gruppe", "+Neues Terminal in dieser Gruppe", "+Alle Sessions zeigen", "-", "+Bearbeiten …", "+Favorit", "-", "+Pfad kopieren", l[8].as_str(), "+Im Editor öffnen", "-", "+Gruppe schließen …"]
        );
        let none = labels(&group_menu(&local, false, &|_| false));
        assert_eq!(none[2], "(Alle Sessions zeigen)");
        assert_eq!(none[9], "(Im Editor öffnen)");
        let mut remote = group("r", "box", "C:/Users/m", &[]);
        remote.host = Some("box".into());
        let l = labels(&group_menu(&remote, true, &known));
        assert_eq!(l, ["+Remote-Session wählen …", "(Alle Sessions zeigen)", "-", "+Bearbeiten …", "+Favorit", "-", "+Gruppe schließen …"]);
    }

    #[test]
    fn favorite_is_checked() {
        let mut g = group("g1", "kadrell", "C:/p/kadrell", &[]);
        g.favorite = true;
        let items = group_menu(&g, false, &known);
        assert!(items.iter().any(|i| matches!(i, Item::Flag { checked: true, enabled: true, cmd: Cmd::ToggleFavorite(_), .. })));
    }
}
