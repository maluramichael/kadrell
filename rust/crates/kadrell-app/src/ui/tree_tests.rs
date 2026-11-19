//! Ports der Swift-Tests zum Baum, die sich ohne Fenster prüfen lassen: `AccessibilityTests` (Klick, Tastatur),
//! `GroupMoveTests` (Ziehen, Tasten), `SidebarFlatTests` (flache Liste), `SidebarRendererTests` (Label).
//! Die Zeilenlogik steckt in `tree_rules`, `groups::drag`, `workspace::tree_keys` und `kadrell_core::group`; hier
//! stehen die Fälle der Mac-Tests mit deren Fixtures.

use std::collections::HashMap;

use kadrell_core::group::{Group as CoreGroup, SidebarFlat};
use kadrell_core::{Session as CoreSession, SidebarSort};

use super::groups::drag::{DragRow, Dragged, Over, Plan, TreeView, plan};
use super::groups::rows::{Rows, focus_row_top};
use super::layout::Selection;
use super::model::{flat_index, test_group};
use super::tree_rules::click_selection;
use crate::appearance::metrics::{GroupShape, RowKind, RowMetrics, tree_rows};
use crate::ui::settings::shortcut::{KeyInput, Platform};
use crate::workspace::tree_keys::{TreeKey, classify};

fn key(name: &str, control: bool, alt: bool, shift: bool, platform: bool) -> KeyInput {
    KeyInput { control, alt, shift, platform, key: name.to_string() }
}

// MARK: AccessibilityTests

/// `testSidebarClickSelectedIsolates`: ein Klick auf eine noch nicht ausgewählte Session wählt nur diese, ein normaler
/// Klick auf eine schon ausgewählte isoliert sie (seit 1.56.3), statt sie aus der Auswahl zu nehmen.
#[test]
fn click_on_a_selected_session_isolates_it() {
    let flat = [(0, 0), (0, 1)];
    let none = Selection { items: Vec::new(), focus: (0, 0), anchor: (0, 0) };
    let first = click_selection(&none, (0, 0), false, false, &flat);
    assert_eq!(first.items, [(0, 0)]);
    let both = Selection { items: vec![(0, 0), (0, 1)], focus: (0, 1), anchor: (0, 0) };
    let again = click_selection(&both, (0, 1), false, false, &flat);
    assert_eq!((again.items, again.focus), (vec![(0, 1)], (0, 1)), "isoliert statt abgewählt");
    let only = click_selection(&first, (0, 0), false, false, &flat);
    assert_eq!(only.items, [(0, 0)], "die einzige Session bleibt gewählt");
}

/// Cmd/Strg-Klick schaltet um, Umschalt markiert den Bereich seit dem Anker.
#[test]
fn modified_clicks_toggle_and_extend() {
    let flat = [(0, 0), (0, 1), (0, 2)];
    let start = Selection::single((0, 0));
    let added = click_selection(&start, (0, 2), false, true, &flat);
    assert_eq!(added.items, [(0, 0), (0, 2)]);
    let removed = click_selection(&added, (0, 2), false, true, &flat);
    assert_eq!(removed.items, [(0, 0)]);
    let range = click_selection(&start, (0, 2), true, false, &flat);
    assert_eq!(range.items, [(0, 0), (0, 1), (0, 2)]);
}

/// `testSidebarCollapseExpandKeyboard`: ← klappt zu (die Session-Zeile fällt weg), → klappt wieder auf; die
/// Fokus-Session bleibt gemerkt, die Gruppenzeile steht dann für sie.
#[test]
fn collapse_and_expand_keys_change_the_rows() {
    let c = |k: &str| classify(&key(k, false, false, false, false), Platform::Mac);
    assert_eq!((c("left"), c("right")), (Some(TreeKey::Collapse), Some(TreeKey::Expand)));
    let m = RowMetrics::CLASSIC;
    let rows = |open: bool| tree_rows(m, &[GroupShape { sessions: 1, open }], &|_, _| false).0;
    assert_eq!(rows(true).len(), 2, "Gruppe und Session");
    assert_eq!(rows(false).len(), 1, "Session-Zeile ist mit eingeklappter Gruppe weg");
    assert_eq!(rows(false)[0].kind, RowKind::Group(0));
    let mut group = test_group("g", &["a"]);
    group.collapsed = true;
    let at = focus_row_top(&[group], "a", Rows { top_pad: 6., group_row: 38., session_row: 24., group_gap: 6. });
    assert_eq!(at, Some((6., true)), "die Gruppenzeile steht für die zugeklappte Fokus-Session");
}

/// `testSidebarKeyboardOpenCloseMove`: ⏎ und Leertaste öffnen, ⌫ schließt, ⌥⌘↓ tauscht mit dem Nachbarn.
#[test]
fn open_close_and_move_keys() {
    let c = |k: &str, alt: bool, platform: bool| classify(&key(k, false, alt, false, platform), Platform::Mac);
    assert_eq!(c("enter", false, false), Some(TreeKey::Open));
    assert_eq!(c("space", false, false), Some(TreeKey::Open));
    assert_eq!(c("backspace", false, false), Some(TreeKey::Close));
    assert_eq!(c("down", true, true), Some(TreeKey::Move(1)));
    assert_eq!(c("up", true, true), Some(TreeKey::Move(-1)));
    assert_eq!(c("down", false, false), Some(TreeKey::Step(1)), "↑↓ wandern nur");
}

// MARK: GroupMoveTests

const GROUPED: TreeView = TreeView { grouped: true, sort_off: true };
const SORTED: TreeView = TreeView { grouped: true, sort_off: false };

fn drag(id: &str, group: &str) -> DragRow {
    DragRow { item: Dragged::Session(id.into()), group: group.into(), order: 1, label: id.into() }
}

/// Zeilen: 0 Gruppe g, 1 a, 2 b, 3 Gruppe h, 4 c.
#[test]
fn dragging_a_session_onto_another_group_moves_it() {
    let into = |group: &str| Some(Plan::MoveToGroup { id: "a".into(), group: group.into() });
    assert_eq!(plan(&drag("a", "g"), Over::Head { group: "h" }, GROUPED), into("h"), "a auf den Kopf von Q");
    let b = plan(&drag("b", "g"), Over::Session { key: "c", group: "h" }, GROUPED);
    assert_eq!(b, Some(Plan::MoveToGroup { id: "b".into(), group: "h".into() }), "b auf eine Session von Q landet ebenso in Q");
    let own = plan(&drag("a", "g"), Over::Session { key: "b", group: "g" }, GROUPED);
    assert_eq!(own, Some(Plan::ReorderSession { id: "a".into(), target: "b".into() }), "in der eigenen Gruppe: umsortieren");
}

/// `testDragWhileSortedOnlyMovesAcrossGroups`.
#[test]
fn sorted_drag_only_moves_across_groups() {
    assert_eq!(plan(&drag("a", "g"), Over::Session { key: "b", group: "g" }, SORTED), None);
    let across = plan(&drag("a", "g"), Over::Head { group: "h" }, SORTED);
    assert_eq!(across, Some(Plan::MoveToGroup { id: "a".into(), group: "h".into() }));
}

/// `testControlReturnPassesThroughAndShiftF10OnCollapsedGroup`: ⌃⏎ geht an die Session (Mac und Windows Strg+⏎), ⇧F10 ist
/// die Menütaste, bei zugeklappter Gruppe steht die Gruppenzeile für die Fokus-Session (siehe oben).
#[test]
fn control_return_passes_through_and_shift_f10_opens_the_menu() {
    assert_eq!(classify(&key("enter", true, false, false, false), Platform::Mac), Some(TreeKey::PassToSession));
    assert_eq!(classify(&key("enter", true, false, false, false), Platform::Windows), Some(TreeKey::PassToSession));
    assert_eq!(classify(&key("f10", false, false, true, false), Platform::Mac), Some(TreeKey::ContextMenu));
    assert_ne!(classify(&key("enter", false, false, false, false), Platform::Mac), Some(TreeKey::PassToSession));
}

// MARK: SidebarFlatTests

fn session(id: &str, cwd: &str, started: f64, status: &str) -> CoreSession {
    let mut s = CoreSession::new(id, cwd, started, id, id);
    s.raw_status = Some(status.to_string());
    s
}

fn fixture() -> (Vec<CoreGroup>, HashMap<String, CoreSession>) {
    let list = [
        session("alt", "/x", 1_000., "busy"),
        session("neu", "/x", 9_000., "busy"),
        session("fertig", "/y", 5_000., "idle"),
        session("wartet", "/y", 2_000., "waiting"),
    ];
    let groups = vec![CoreGroup::new("1", "x", "", "/x", &["alt", "neu"]), CoreGroup::new("2", "y", "", "/y", &["fertig", "wartet"])];
    (groups, list.into_iter().map(|s| (s.id.clone(), s)).collect())
}

fn ids<'a>(rows: &[kadrell_core::group::FlatRow<'a>]) -> Vec<&'a str> {
    rows.iter().map(|r| r.session.id.as_str()).collect()
}

/// `testFlatSortIgnoresSavedOrder`: Sortierung greift auch flach, über Gruppengrenzen hinweg, die gespeicherte Ordnung
/// zählt dann nicht.
#[test]
fn flat_sort_ignores_the_saved_order() {
    let (groups, sessions) = fixture();
    let order: Vec<String> = ["fertig", "alt", "neu", "wartet"].iter().map(|s| s.to_string()).collect();
    let by_status = SidebarFlat::rows(&groups, &sessions, SidebarSort::Status, &order);
    assert_eq!(ids(&by_status), ["wartet", "neu", "alt", "fertig"]);
    let by_name = SidebarFlat::rows(&groups, &sessions, SidebarSort::Alpha, &order);
    assert_eq!(ids(&by_name), ["alt", "fertig", "neu", "wartet"]);
}

/// `testTreeShowsEverySessionWithoutGroupRows`: flach alle Sessions, keine Gruppenzeilen, auch aus eingeklappten Gruppen.
#[test]
fn flat_tree_shows_every_session_without_group_rows() {
    let (groups, sessions) = fixture();
    let order: Vec<String> = ["alt", "neu", "fertig", "wartet"].iter().map(|s| s.to_string()).collect();
    let rows = SidebarFlat::rows(&groups, &sessions, SidebarSort::Off, &order);
    assert_eq!(ids(&rows), ["alt", "neu", "fertig", "wartet"]);
    // Die Ansicht legt alle Sessions in eine Sammelgruppe ohne Kopfzeile, eingeklappte Gruppen ändern daran nichts.
    let mut pseudo = test_group("", &["alt", "neu", "fertig", "wartet"]);
    pseudo.collapsed = false;
    assert_eq!(flat_index(&[pseudo]).len(), 4);
    assert_eq!(rows.len(), 4, "keine Gruppenzeilen");
}

/// `testGroupedTreeIsUnchanged`: gruppiert bleibt alles wie bisher, Gruppenzeilen dazu, Handreihenfolge aus `groups.json`.
#[test]
fn grouped_tree_is_unchanged() {
    let (groups, sessions) = fixture();
    let from_groups: Vec<&str> = groups.iter().flat_map(|g| g.session_ids.iter().map(String::as_str)).filter(|id| sessions.contains_key(*id)).collect();
    assert_eq!(from_groups, ["alt", "neu", "fertig", "wartet"]);
    let shapes: Vec<GroupShape> = groups.iter().map(|g| GroupShape { sessions: g.session_ids.len(), open: true }).collect();
    let (rows, _) = tree_rows(RowMetrics::CLASSIC, &shapes, &|_, _| false);
    assert_eq!(rows.len(), 6, "zwei Gruppenzeilen dazu");
}

// MARK: SidebarRendererTests

/// `testGroupLabelIsUppercaseWithCount`.
#[test]
fn group_label_is_uppercase_with_count() {
    let (name, count) = super::tree_rules::group_label_parts("Kadrell", 2);
    assert_eq!(format!("{name}{count}"), "KADRELL · 2");
}
