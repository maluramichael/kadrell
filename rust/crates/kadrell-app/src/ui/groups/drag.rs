//! Ziehen im Baum und an den Kacheln: welche Ablage was bewirkt (`target(for:at:)`, `sessionTarget` in
//! `SidebarView.swift`, `moveSession` in `AppDelegate.swift`). Die Regeln sind reine Funktionen, ausgeführt wird
//! über die Operationen des Kerns (`GroupStore::move_session`, `move_group`, `move_sessions`).

use kadrell_core::group::GroupStore;

/// Was gezogen wird.
#[derive(Clone, Debug, PartialEq)]
pub enum Dragged {
    Session(String),
    Group(String),
}

/// Nutzlast eines Zugs im Baum.
#[derive(Clone, Debug, PartialEq)]
pub struct DragRow {
    pub item: Dragged,
    /// Gruppe, in der die Zeile beim Greifen stand (leer in der flachen Liste).
    pub group: String,
    /// Laufende Nummer der Zeile im Baum beim Greifen: bestimmt, ob die Einfügelinie über oder unter dem Ziel steht.
    pub order: usize,
    pub label: String,
}

/// Nutzlast beim Ziehen einer Kachel an ihrer Titelzeile (Schlüssel der Session).
#[derive(Clone, Debug, PartialEq)]
pub struct DragTile {
    pub key: String,
    pub label: String,
}

/// Ansicht des Baums, von der abhängt, was gezogen werden darf.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TreeView {
    /// Nach Projekt gruppiert (sonst flache Liste).
    pub grouped: bool,
    /// Ohne Sortierung (A-Z, Status): nur dann gibt es eine Handreihenfolge.
    pub sort_off: bool,
}

/// Zeile, über der losgelassen wird.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Over<'a> {
    /// Session-Zeile und ihre Gruppe.
    Session { key: &'a str, group: &'a str },
    /// Kopfzeile einer Gruppe.
    Head { group: &'a str },
}

impl Over<'_> {
    fn group(&self) -> &str {
        match self {
            Over::Session { group, .. } | Over::Head { group } => group,
        }
    }
}

/// Was die Ablage bewirkt.
#[derive(Clone, Debug, PartialEq)]
pub enum Plan {
    /// Gruppiert: die Session nimmt in ihrer Gruppe den Platz der anderen ein.
    ReorderSession { id: String, target: String },
    /// Flach: die Session nimmt den Platz der anderen in der Liste ein (`sidebar.flatOrder`).
    ReorderFlat { id: String, target: String },
    /// Die Session wechselt in die Gruppe (Ordner und Prozess bleiben).
    MoveToGroup { id: String, group: String },
    /// Die ganze Gruppe nimmt den Platz der Zielgruppe ein.
    MoveGroup { id: String, target: String },
}

/// Darf diese Zeile überhaupt gegriffen werden? Sortiert (A-Z, Status) gibt es keine Handreihenfolge: eine Session
/// lässt sich dann nur in eine andere Gruppe ziehen, eine Gruppe gar nicht. Die Sammelgruppe der flachen Liste nie.
pub fn can_drag(item: &Dragged, view: TreeView) -> bool {
    match item {
        Dragged::Session(_) => view.sort_off || view.grouped,
        Dragged::Group(id) => view.grouped && view.sort_off && !id.is_empty(),
    }
}

/// Wirkung der Ablage von `drag` über `over`, `None` wenn dort nichts passiert.
pub fn plan(drag: &DragRow, over: Over, view: TreeView) -> Option<Plan> {
    if !can_drag(&drag.item, view) {
        return None;
    }
    match &drag.item {
        Dragged::Session(id) => plan_session(id, &drag.group, over, view),
        Dragged::Group(id) => {
            let other = over.group();
            (other != id && !other.is_empty()).then(|| Plan::MoveGroup { id: id.clone(), target: other.to_string() })
        }
    }
}

/// Flach nur umsortieren. Gruppiert: in der eigenen Gruppe umsortieren (nur ohne Sortierung, nicht auf den eigenen
/// Kopf), über jeder Zeile einer fremden Gruppe hineinfallen.
fn plan_session(id: &str, own: &str, over: Over, view: TreeView) -> Option<Plan> {
    if !view.grouped {
        return match over {
            Over::Session { key, .. } if view.sort_off && key != id => Some(Plan::ReorderFlat { id: id.into(), target: key.into() }),
            _ => None,
        };
    }
    if over.group() != own {
        return Some(Plan::MoveToGroup { id: id.into(), group: over.group().into() });
    }
    match over {
        Over::Session { key, .. } if view.sort_off && key != id => Some(Plan::ReorderSession { id: id.into(), target: key.into() }),
        _ => None,
    }
}

/// Fällt die Session in die Zielgruppe (Rahmen statt Linie)?
pub fn drops_into_group(plan: &Option<Plan>) -> bool {
    matches!(plan, Some(Plan::MoveToGroup { .. }))
}

/// Neue Reihenfolge der flachen Liste: `id` nimmt den Platz von `target` ein.
pub fn flat_reordered(order: &[String], id: &str, target: &str) -> Option<Vec<String>> {
    let from = order.iter().position(|k| k == id)?;
    let to = order.iter().position(|k| k == target)?;
    if from == to {
        return None;
    }
    let mut out = order.to_vec();
    let moved = out.remove(from);
    out.insert(to, moved);
    Some(out)
}

/// Kachel auf Kachel (`moveSession` in `AppDelegate.swift`): in derselben Gruppe tauscht die Session den Platz, über
/// Gruppengrenzen hinweg rückt die ganze Gruppe an die Stelle der Zielgruppe.
pub fn plan_tile(groups: &GroupStore, src: &str, target: &str) -> Option<Plan> {
    let (g, t) = (groups.group_for_session(src)?, groups.group_for_session(target)?);
    if g.id == t.id {
        return (src != target).then(|| Plan::ReorderSession { id: src.into(), target: target.into() });
    }
    Some(Plan::MoveGroup { id: g.id.clone(), target: t.id.clone() })
}

/// Führt einen gruppierten Plan auf `groups.json` aus. Die flache Liste liegt in den Einstellungen und läuft über
/// [`flat_reordered`]. Gibt zurück, ob sich etwas geändert hat.
pub fn apply(groups: &mut GroupStore, plan: &Plan) -> bool {
    match plan {
        Plan::ReorderSession { id, target } => {
            let before = groups.ordered_keys();
            groups.move_session(id, target);
            before != groups.ordered_keys()
        }
        Plan::MoveToGroup { id, group } => groups.move_sessions(std::slice::from_ref(id), group),
        Plan::MoveGroup { id, target } => {
            let before: Vec<String> = groups.groups.iter().map(|g| g.id.clone()).collect();
            groups.move_group(id, target);
            before != groups.groups.iter().map(|g| g.id.clone()).collect::<Vec<_>>()
        }
        Plan::ReorderFlat { .. } => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kadrell_core::group::Group;

    const GROUPED: TreeView = TreeView { grouped: true, sort_off: true };
    const SORTED: TreeView = TreeView { grouped: true, sort_off: false };
    const FLAT: TreeView = TreeView { grouped: false, sort_off: true };
    const FLAT_SORTED: TreeView = TreeView { grouped: false, sort_off: false };

    fn session(id: &str, group: &str) -> DragRow {
        DragRow { item: Dragged::Session(id.into()), group: group.into(), order: 1, label: id.into() }
    }

    fn group(id: &str) -> DragRow {
        DragRow { item: Dragged::Group(id.into()), group: id.into(), order: 0, label: id.into() }
    }

    #[test]
    fn session_sorts_in_its_group_and_falls_into_foreign_ones() {
        let s = session("a", "g1");
        assert_eq!(plan(&s, Over::Session { key: "b", group: "g1" }, GROUPED), Some(Plan::ReorderSession { id: "a".into(), target: "b".into() }));
        assert_eq!(plan(&s, Over::Session { key: "a", group: "g1" }, GROUPED), None);
        assert_eq!(plan(&s, Over::Head { group: "g1" }, GROUPED), None);
        let into = Some(Plan::MoveToGroup { id: "a".into(), group: "g2".into() });
        assert_eq!(plan(&s, Over::Head { group: "g2" }, GROUPED), into);
        assert_eq!(plan(&s, Over::Session { key: "c", group: "g2" }, GROUPED), into);
        assert!(drops_into_group(&into));
    }

    #[test]
    fn sorted_tree_only_moves_between_groups() {
        let s = session("a", "g1");
        assert_eq!(plan(&s, Over::Session { key: "b", group: "g1" }, SORTED), None);
        assert!(plan(&s, Over::Head { group: "g2" }, SORTED).is_some());
        assert!(!can_drag(&Dragged::Group("g1".into()), SORTED));
        assert_eq!(plan(&group("g1"), Over::Head { group: "g2" }, SORTED), None);
    }

    #[test]
    fn group_drops_onto_any_row_of_another_group() {
        let g = group("g1");
        let expected = Some(Plan::MoveGroup { id: "g1".into(), target: "g2".into() });
        assert_eq!(plan(&g, Over::Head { group: "g2" }, GROUPED), expected);
        assert_eq!(plan(&g, Over::Session { key: "x", group: "g2" }, GROUPED), expected);
        assert_eq!(plan(&g, Over::Head { group: "g1" }, GROUPED), None);
        assert_eq!(plan(&g, Over::Session { key: "x", group: "g1" }, GROUPED), None);
    }

    #[test]
    fn flat_list_reorders_only_without_sort_and_never_moves_groups() {
        let s = session("a", "");
        assert_eq!(plan(&s, Over::Session { key: "b", group: "" }, FLAT), Some(Plan::ReorderFlat { id: "a".into(), target: "b".into() }));
        assert_eq!(plan(&s, Over::Session { key: "b", group: "" }, FLAT_SORTED), None);
        assert!(!can_drag(&Dragged::Session("a".into()), FLAT_SORTED));
        assert!(!can_drag(&Dragged::Group(String::new()), FLAT));
        assert_eq!(plan(&s, Over::Head { group: "" }, FLAT), None);
    }

    #[test]
    fn flat_reorder_takes_the_place_of_the_target() {
        let order: Vec<String> = ["a", "b", "c", "d"].iter().map(|s| s.to_string()).collect();
        assert_eq!(flat_reordered(&order, "a", "c").unwrap(), ["b", "c", "a", "d"]);
        assert_eq!(flat_reordered(&order, "d", "b").unwrap(), ["a", "d", "b", "c"]);
        assert_eq!(flat_reordered(&order, "a", "a"), None);
        assert_eq!(flat_reordered(&order, "a", "zzz"), None);
    }

    fn store(ids: [&[&str]; 3]) -> (GroupStore, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("kadrell-drag-{}-{}", std::process::id(), ids.iter().map(|g| g.len()).sum::<usize>()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut s = GroupStore::open(dir.join("groups.json"));
        for (i, members) in ids.iter().enumerate() {
            s.add(Group::new(&format!("g{i}"), &format!("n{i}"), "#fab387", &format!("C:/p{i}"), members));
        }
        (s, dir)
    }

    #[test]
    fn tile_drop_swaps_in_group_and_moves_whole_groups_across() {
        let (mut s, dir) = store([&["a", "b"], &["c"], &["d"]]);
        assert_eq!(plan_tile(&s, "a", "b"), Some(Plan::ReorderSession { id: "a".into(), target: "b".into() }));
        assert_eq!(plan_tile(&s, "a", "a"), None);
        let across = plan_tile(&s, "a", "d").unwrap();
        assert_eq!(across, Plan::MoveGroup { id: "g0".into(), target: "g2".into() });
        assert!(apply(&mut s, &across));
        assert_eq!(s.ordered_keys(), ["c", "d", "a", "b"]);
        assert_eq!(plan_tile(&s, "a", "zzz"), None);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn apply_reports_changes_and_empty_group_without_heart_disappears() {
        let (mut s, dir) = store([&["a"], &["b"], &["c"]]);
        s.toggle_favorite("g2");
        let into = Plan::MoveToGroup { id: "a".into(), group: "g1".into() };
        assert!(apply(&mut s, &into));
        assert!(s.group("g0").is_none(), "leere Gruppe ohne Herz verschwindet");
        assert!(!apply(&mut s, &into), "schon dort");
        assert!(apply(&mut s, &Plan::MoveToGroup { id: "c".into(), group: "g1".into() }));
        assert!(s.group("g2").is_some(), "mit Herz bleibt sie leer stehen");
        assert!(apply(&mut s, &Plan::ReorderSession { id: "c".into(), target: "a".into() }));
        assert_eq!(s.ordered_keys(), ["b", "c", "a"].map(String::from));
        let _ = std::fs::remove_dir_all(dir);
    }
}
