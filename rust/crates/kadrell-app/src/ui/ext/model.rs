//! Ansichtsmodell der Extension-Panels: macht aus dem `PanelTree` einer Extension die Zeilen der Seitenleiste
//! (`ExtensionPanelView.swift`: `flatten`, `step`, `switchTab`, `activate`). Ohne GPUI, damit es testbar bleibt.

use std::collections::HashSet;

use kadrell_ext::panel::{PanelAction, PanelNode, PanelTree, ThemeColor};

/// Höhen der Zeilen in px (`ExtensionPanelView.height`).
pub const TAB_HEIGHT: f32 = 30.;
pub const SECTION_HEIGHT: f32 = 26.;
pub const ITEM_HEIGHT: f32 = 24.;
pub const TEXT_HEIGHT: f32 = 20.;
pub const BUTTON_HEIGHT: f32 = 32.;

/// Ein Panel einer Extension: Name der Extension und ihr Baum.
pub type PanelEntry = (String, PanelTree);

/// Was eine Zeile zeigt.
#[derive(Debug, Clone, PartialEq)]
pub enum LineKind {
    Section { title: String, open: bool },
    Item { text: String, detail: Option<String>, color: Option<ThemeColor>, actions: Vec<PanelAction> },
    Text { text: String, color: Option<ThemeColor> },
    Button { label: String, action: String },
}

/// Eine sichtbare Zeile des gewählten Tabs. `key` (Tab plus Pfad) erkennt Abschnitte über `panel.set` hinweg wieder.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub kind: LineKind,
    pub key: String,
    pub depth: usize,
    pub height: f32,
}

impl Line {
    /// Text ist die einzige Zeile, die die Auswahl überspringt.
    pub fn selectable(&self) -> bool {
        !matches!(self.kind, LineKind::Text { .. })
    }

    /// Was Screenreader vorlesen.
    pub fn spoken(&self) -> String {
        match &self.kind {
            LineKind::Section { title, .. } => title.clone(),
            LineKind::Item { text, detail, .. } => [Some(text.as_str()), detail.as_deref()].into_iter().flatten().collect::<Vec<_>>().join(", "),
            LineKind::Text { text, .. } => text.clone(),
            LineKind::Button { label, .. } => label.clone(),
        }
    }
}

/// Beschriftung eines Tabs: Titel des Baums, sonst der Name der Extension.
pub fn tab_label(entry: &PanelEntry) -> &str {
    if entry.1.title.is_empty() { &entry.0 } else { &entry.1.title }
}

fn height(node: &PanelNode) -> f32 {
    match node {
        PanelNode::Section { .. } => SECTION_HEIGHT,
        PanelNode::Item { .. } => ITEM_HEIGHT,
        PanelNode::Text(..) => TEXT_HEIGHT,
        PanelNode::Button { .. } => BUTTON_HEIGHT,
    }
}

/// Was `activate` bewirkt hat.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Nothing,
    /// Ein Abschnitt wurde auf- oder zugeklappt, die Zeilen sind neu.
    Toggled,
    /// Aktion `id` der Extension `tab` auslösen (`ui.action`).
    Fire { tab: String, id: String },
}

/// Zustand einer Seitenleiste: die Panels, der gewählte Tab, handaufgeklappte Abschnitte und die Tastaturzeile.
#[derive(Debug, Clone, Default)]
pub struct PanelModel {
    panels: Vec<PanelEntry>,
    selected_tab: Option<String>,
    /// Abschnitte, die von Hand anders auf- oder zugeklappt sind, als die Extension sie liefert.
    toggled: HashSet<String>,
    cursor: Option<usize>,
    lines: Vec<Line>,
}

impl PanelModel {
    pub fn panels(&self) -> &[PanelEntry] {
        &self.panels
    }

    pub fn lines(&self) -> &[Line] {
        &self.lines
    }

    pub fn cursor(&self) -> Option<usize> {
        self.cursor
    }

    pub fn selected_tab(&self) -> Option<&str> {
        self.selected_tab.as_deref()
    }

    pub fn is_empty(&self) -> bool {
        self.panels.is_empty()
    }

    /// Neue Panels. `false`, wenn sich nichts geändert hat (der Manager meldet bis zu 50-mal pro Sekunde).
    /// Verschwindet der gewählte Tab, gilt der erste.
    pub fn set_panels(&mut self, panels: Vec<PanelEntry>) -> bool {
        if panels == self.panels {
            return false;
        }
        self.panels = panels;
        if !self.panels.iter().any(|p| Some(&p.0) == self.selected_tab.as_ref()) {
            self.selected_tab = self.panels.first().map(|p| p.0.clone());
            self.cursor = None;
        }
        self.relayout();
        true
    }

    pub fn select_tab(&mut self, name: &str) {
        if self.selected_tab.as_deref() == Some(name) || !self.panels.iter().any(|p| p.0 == name) {
            return;
        }
        self.selected_tab = Some(name.to_string());
        self.cursor = None;
        self.relayout();
    }

    /// ←→: ein Tab weiter, am Rand bleibt es stehen.
    pub fn switch_tab(&mut self, delta: isize) {
        let Some(i) = self.panels.iter().position(|p| Some(&p.0) == self.selected_tab.as_ref()) else { return };
        let next = (i as isize + delta).clamp(0, self.panels.len() as isize - 1) as usize;
        let name = self.panels[next].0.clone();
        self.select_tab(&name);
    }

    fn is_open(&self, kind: &LineKind) -> bool {
        matches!(kind, LineKind::Section { open: true, .. })
    }

    /// Zeilen neu aufbauen, nach neuen Panels, einem gewählten Tab oder einem geklappten Abschnitt.
    fn relayout(&mut self) {
        let mut lines = Vec::new();
        let tree = self.selected_tab.as_ref().and_then(|tab| self.panels.iter().find(|p| &p.0 == tab));
        if let Some((tab, tree)) = tree {
            self.flatten(&tree.nodes, 0, tab, &mut lines);
        }
        self.lines = lines;
        if let Some(c) = self.cursor
            && !self.lines.get(c).is_some_and(Line::selectable)
        {
            self.cursor = None;
        }
    }

    fn flatten(&self, nodes: &[PanelNode], depth: usize, path: &str, out: &mut Vec<Line>) {
        for (i, node) in nodes.iter().enumerate() {
            let key = format!("{path}/{i}");
            let kind = self.kind(node, &key);
            let open = self.is_open(&kind);
            out.push(Line { kind, key: key.clone(), depth, height: height(node) });
            if let PanelNode::Section { children, .. } = node
                && open
            {
                self.flatten(children, depth + 1, &key, out);
            }
        }
    }

    /// Offen ist ein Abschnitt, wenn er so geliefert wird und nicht von Hand zugeklappt ist, oder umgekehrt.
    fn kind(&self, node: &PanelNode, key: &str) -> LineKind {
        match node {
            PanelNode::Section { title, collapsed, .. } => {
                LineKind::Section { title: title.clone(), open: *collapsed == self.toggled.contains(key) }
            }
            PanelNode::Item { text, detail, color, actions } => {
                LineKind::Item { text: text.clone(), detail: detail.clone(), color: *color, actions: actions.clone() }
            }
            PanelNode::Text(text, color) => LineKind::Text { text: text.clone(), color: *color },
            PanelNode::Button { label, action } => LineKind::Button { label: label.clone(), action: action.clone() },
        }
    }

    /// Gesamthöhe in px: Tab-Reihe, Zeilen und etwas Luft.
    pub fn content_height(&self) -> f32 {
        TAB_HEIGHT + 4. + self.lines.iter().map(|l| l.height).sum::<f32>() + 6.
    }

    // MARK: Tastatur und Maus

    pub fn set_cursor(&mut self, index: Option<usize>) {
        self.cursor = index.filter(|i| self.lines.get(*i).is_some_and(Line::selectable));
    }

    /// ↑↓: über wählbare Zeilen, am Rand bleibt es stehen. Ohne Auswahl: erste bzw. letzte.
    pub fn step(&mut self, delta: isize) {
        let rows: Vec<usize> = (0..self.lines.len()).filter(|i| self.lines[*i].selectable()).collect();
        if rows.is_empty() {
            return;
        }
        let pos = match self.cursor.and_then(|c| rows.iter().position(|r| *r == c)) {
            Some(p) => (p as isize + delta).clamp(0, rows.len() as isize - 1) as usize,
            None if delta > 0 => 0,
            None => rows.len() - 1,
        };
        self.cursor = Some(rows[pos]);
    }

    /// Abschnitt klappt, Eintrag löst seine erste Aktion aus, Knopf seine.
    pub fn activate(&mut self, index: usize) -> Outcome {
        let Some(line) = self.lines.get(index) else { return Outcome::Nothing };
        let tab = self.selected_tab.clone().unwrap_or_default();
        match &line.kind {
            LineKind::Section { .. } => {
                let key = line.key.clone();
                if !self.toggled.remove(&key) {
                    self.toggled.insert(key);
                }
                self.relayout();
                Outcome::Toggled
            }
            LineKind::Item { actions, .. } => actions.first().map_or(Outcome::Nothing, |a| Outcome::Fire { tab, id: a.id.clone() }),
            LineKind::Button { action, .. } => Outcome::Fire { tab, id: action.clone() },
            LineKind::Text { .. } => Outcome::Nothing,
        }
    }

    /// Alle Aktionen des Eintrags (Kontextmenü, ⌥⏎), mit Tab festgehalten: ein `panel.set` bei offenem Menü darf
    /// sie nicht umlenken.
    pub fn actions(&self, index: usize) -> Option<(String, Vec<PanelAction>)> {
        let LineKind::Item { actions, .. } = &self.lines.get(index)?.kind else { return None };
        (!actions.is_empty()).then(|| (self.selected_tab.clone().unwrap_or_default(), actions.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(text: &str, actions: &[&str]) -> PanelNode {
        PanelNode::Item {
            text: text.into(),
            detail: Some("d".into()),
            color: Some(ThemeColor::Ok),
            actions: actions.iter().map(|id| PanelAction { id: (*id).into(), label: (*id).into() }).collect(),
        }
    }

    fn tree(title: &str, nodes: Vec<PanelNode>) -> PanelTree {
        PanelTree { title: title.into(), nodes }
    }

    fn two_tabs() -> Vec<PanelEntry> {
        vec![
            ("a".into(), tree("Alpha", vec![PanelNode::Text("hinweis".into(), None), item("eins", &["open:1", "x"]), item("zwei", &["open:2"])])),
            ("b".into(), tree("", vec![PanelNode::Button { label: "Los".into(), action: "go".into() }])),
        ]
    }

    #[test]
    fn heights_and_keys_follow_the_swift_tree() {
        let mut m = PanelModel::default();
        m.set_panels(vec![(
            "a".into(),
            tree(
                "A",
                vec![
                    PanelNode::Section { title: "S".into(), collapsed: false, children: vec![item("i", &[]), PanelNode::Text("t".into(), None)] },
                    PanelNode::Button { label: "b".into(), action: "x".into() },
                ],
            ),
        )]);
        let keys: Vec<&str> = m.lines().iter().map(|l| l.key.as_str()).collect();
        assert_eq!(keys, ["a/0", "a/0/0", "a/0/1", "a/1"]);
        let heights: Vec<f32> = m.lines().iter().map(|l| l.height).collect();
        assert_eq!(heights, [26., 24., 20., 32.]);
        let depths: Vec<usize> = m.lines().iter().map(|l| l.depth).collect();
        assert_eq!(depths, [0, 1, 1, 0]);
        assert_eq!(m.content_height(), 30. + 4. + 102. + 6.);
    }

    #[test]
    fn collapsed_sections_hide_children_and_toggle_by_hand() {
        let mut m = PanelModel::default();
        let section = |collapsed| PanelNode::Section { title: "S".into(), collapsed, children: vec![item("i", &[])] };
        m.set_panels(vec![("a".into(), tree("A", vec![section(true)]))]);
        assert_eq!(m.lines().len(), 1, "zugeklappt geliefert");
        assert_eq!(m.activate(0), Outcome::Toggled);
        assert_eq!(m.lines().len(), 2, "von Hand aufgeklappt");
        // Ein neues panel.set derselben Form behält die Handklappe (Schlüssel Tab plus Pfad).
        m.set_panels(vec![("a".into(), tree("A2", vec![section(true)]))]);
        assert_eq!(m.lines().len(), 2);
        assert_eq!(m.activate(0), Outcome::Toggled);
        assert_eq!(m.lines().len(), 1);
    }

    #[test]
    fn keyboard_steps_over_selectable_rows_and_clamps() {
        let mut m = PanelModel::default();
        m.set_panels(two_tabs());
        assert_eq!(m.selected_tab(), Some("a"), "ohne Auswahl der erste Tab");
        assert!(!m.lines()[0].selectable(), "Text ist nicht wählbar");
        m.step(1);
        assert_eq!(m.cursor(), Some(1), "der Text wird übersprungen");
        m.step(1);
        m.step(1);
        assert_eq!(m.cursor(), Some(2), "am Ende bleibt es stehen");
        assert_eq!(m.activate(2), Outcome::Fire { tab: "a".into(), id: "open:2".into() });
        m.step(-1);
        m.step(-1);
        assert_eq!(m.cursor(), Some(1), "am Anfang bleibt es stehen");
        assert_eq!(m.activate(1), Outcome::Fire { tab: "a".into(), id: "open:1".into() }, "erste Aktion");
    }

    #[test]
    fn step_without_cursor_starts_at_the_ends() {
        let mut m = PanelModel::default();
        m.set_panels(two_tabs());
        m.step(-1);
        assert_eq!(m.cursor(), Some(2));
    }

    #[test]
    fn tabs_switch_and_clamp_and_reset_the_cursor() {
        let mut m = PanelModel::default();
        m.set_panels(two_tabs());
        m.step(1);
        m.switch_tab(1);
        assert_eq!(m.selected_tab(), Some("b"));
        assert_eq!(m.cursor(), None);
        m.switch_tab(1);
        assert_eq!(m.selected_tab(), Some("b"), "am letzten Tab bleibt es stehen");
        assert_eq!(m.activate(0), Outcome::Fire { tab: "b".into(), id: "go".into() });
        m.switch_tab(-1);
        assert_eq!(m.selected_tab(), Some("a"));
        assert_eq!(tab_label(&m.panels()[0]), "Alpha");
        assert_eq!(tab_label(&m.panels()[1]), "b", "ohne Titel gilt der Name");
    }

    #[test]
    fn vanished_tab_falls_back_and_equal_panels_report_no_change() {
        let mut m = PanelModel::default();
        assert!(m.set_panels(two_tabs()));
        assert!(!m.set_panels(two_tabs()), "gleicher Stand ist keine Änderung");
        m.select_tab("b");
        m.set_panels(vec![two_tabs().remove(0)]);
        assert_eq!(m.selected_tab(), Some("a"));
        m.set_panels(Vec::new());
        assert_eq!(m.selected_tab(), None);
        assert!(m.lines().is_empty());
    }

    #[test]
    fn actions_keep_their_tab_and_spoken_text_joins_detail() {
        let mut m = PanelModel::default();
        m.set_panels(two_tabs());
        let (tab, actions) = m.actions(1).expect("Eintrag mit Aktionen");
        assert_eq!(tab, "a");
        assert_eq!(actions.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(), ["open:1", "x"]);
        assert!(m.actions(0).is_none(), "Text hat keine");
        assert_eq!(m.lines()[1].spoken(), "eins, d");
    }

    #[test]
    fn set_cursor_ignores_text_rows() {
        let mut m = PanelModel::default();
        m.set_panels(two_tabs());
        m.set_cursor(Some(0));
        assert_eq!(m.cursor(), None);
        m.set_cursor(Some(2));
        assert_eq!(m.cursor(), Some(2));
    }
}
