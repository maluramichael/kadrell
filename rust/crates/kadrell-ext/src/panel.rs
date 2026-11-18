//! Panel-Baum, Statuseintrag und Palette-Einträge: prüft, was eine Extension schickt. Unbekannte Knoten und Felder
//! werden übersprungen und als Warnung gemeldet, zu große Bäume abgelehnt. Fremdes JSON darf nie abstürzen.

use serde_json::Value;

use crate::i18n::{tr, tr_args};
use crate::json::clip;

pub const MAX_NODES: usize = 2000;
pub const MAX_TEXT: usize = 500;
pub const MAX_STATUS: usize = 40;
pub const MAX_WARNINGS: usize = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeColor {
    Accent,
    Muted,
    Ok,
    Warn,
    Err,
}

impl ThemeColor {
    pub fn parse(name: &str) -> Option<ThemeColor> {
        match name {
            "accent" => Some(Self::Accent),
            "muted" => Some(Self::Muted),
            "ok" => Some(Self::Ok),
            "warn" => Some(Self::Warn),
            "err" => Some(Self::Err),
            _ => None,
        }
    }
}

/// Wohin eine Extension ihr Panel hängt. `Right` ist der alte Ort (Default). `Top`/`Bottom` sind reserviert, aber in
/// dieser Version noch nicht platziert (fallen auf `Right` zurück).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Dock {
    Left,
    #[default]
    Right,
}

impl Dock {
    /// Unbekanntes oder fehlendes `dock` ergibt rechts, wie bisher. Noch nicht platzierte Docks ebenfalls.
    pub fn placed(raw: Option<&str>) -> Dock {
        if raw == Some("left") {
            Dock::Left
        } else {
            Dock::Right
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PanelAction {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PanelNode {
    Section {
        title: String,
        collapsed: bool,
        children: Vec<PanelNode>,
    },
    Item {
        text: String,
        detail: Option<String>,
        color: Option<ThemeColor>,
        actions: Vec<PanelAction>,
    },
    Text(String, Option<ThemeColor>),
    Button {
        label: String,
        action: String,
    },
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct PanelTree {
    pub title: String,
    pub nodes: Vec<PanelNode>,
}

/// Ein von einer Extension beigesteuerter Eintrag für den Neue-Session-Dialog (⌘N).
#[derive(Debug, Clone, PartialEq)]
pub struct PaletteItem {
    pub id: String,
    pub title: String,
    pub detail: Option<String>,
    pub group: Option<String>,
}

/// Der Statuseintrag oben.
#[derive(Debug, Clone, PartialEq)]
pub struct StatusItem {
    pub text: String,
    pub color: Option<ThemeColor>,
    pub action: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PanelError {
    TooLarge(usize),
}

/// Ergebnis von `tree`: der Baum und die Warnungen (höchstens `MAX_WARNINGS` plus eine Sammelzeile).
pub type TreeResult = Result<(PanelTree, Vec<String>), PanelError>;

pub fn tree(v: &Value) -> TreeResult {
    let count = node_count(v.get("children"));
    if count > MAX_NODES {
        return Err(PanelError::TooLarge(count));
    }
    let mut builder = Builder::default();
    let nodes = builder.nodes(v.get("children"));
    let title = clip(
        v.get("title").and_then(Value::as_str).unwrap_or(""),
        MAX_TEXT,
    );
    Ok((PanelTree { title, nodes }, builder.summary()))
}

pub fn status(v: &Value) -> Option<StatusItem> {
    let text = v.get("text")?.as_str()?;
    Some(StatusItem {
        text: clip(text, MAX_STATUS),
        color: v
            .get("color")
            .and_then(Value::as_str)
            .and_then(ThemeColor::parse),
        action: v
            .get("action")
            .and_then(Value::as_str)
            .map(|s| clip(s, MAX_TEXT)),
    })
}

/// Palette-Einträge für ⌘N. `null` oder Nicht-Array heißt leer; Einträge ohne `id`/`title` werden übersprungen, die
/// Anzahl gedeckelt wie der Panel-Baum.
pub fn palette(v: Option<&Value>) -> Vec<PaletteItem> {
    let items = v
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    items
        .iter()
        .take(MAX_NODES)
        .filter_map(palette_item)
        .collect()
}

fn palette_item(item: &Value) -> Option<PaletteItem> {
    let id = item.get("id")?.as_str()?;
    let title = item.get("title")?.as_str()?;
    Some(PaletteItem {
        id: clip(id, MAX_TEXT),
        title: clip(title, MAX_TEXT),
        detail: item
            .get("detail")
            .and_then(Value::as_str)
            .map(|s| clip(s, MAX_TEXT)),
        group: item
            .get("group")
            .and_then(Value::as_str)
            .map(|s| clip(s, MAX_STATUS)),
    })
}

/// Zählt Rohknoten vor dem Prüfen, damit ein Flut-Panel gar nicht erst gebaut wird. Jedes Array-Element zählt, auch
/// Nicht-Knoten und Aktionen.
fn node_count(children: Option<&Value>) -> usize {
    let Some(items) = children.and_then(Value::as_array) else {
        return 0;
    };
    items
        .iter()
        .map(|n| {
            1 + node_count(n.get("children"))
                + n.get("actions")
                    .and_then(Value::as_array)
                    .map_or(0, Vec::len)
        })
        .sum()
}

/// Warnung über eine unbekannte Farbe (Panel und Statusleiste teilen sie).
pub fn unknown_color_warning() -> String {
    tr(
        "Unbekannte Farbe ignoriert (erlaubt: accent, muted, ok, warn, err)",
        "Ignored unknown color (allowed: accent, muted, ok, warn, err)",
    )
}

/// Warnung über ein fehlendes Pflichtfeld eines Knotens (auch für den Statuseintrag).
pub fn missing_field_warning(kind: &str, field: &str) -> String {
    tr_args(
        "Knoten „{0}“ ohne Feld „{1}“ übersprungen",
        "Skipped “{0}” node without field “{1}”",
        &[kind, field],
    )
}

/// Sammelt Warnungen beim Bauen; ein Knoten pro Aufruf, damit jede Funktion klein bleibt.
#[derive(Default)]
struct Builder {
    warnings: Vec<String>,
    dropped: usize,
}

impl Builder {
    /// Höchstens `MAX_WARNINGS`, der Rest als eine Sammelzeile.
    fn summary(&self) -> Vec<String> {
        let mut out = self.warnings.clone();
        if self.dropped > 0 {
            let n = self.dropped.to_string();
            out.push(tr_args(
                "… und {0} weitere Warnungen",
                "… and {0} more warnings",
                &[&n],
            ));
        }
        out
    }

    fn warn(&mut self, text: String) {
        if self.warnings.len() < MAX_WARNINGS {
            self.warnings.push(text);
        } else {
            self.dropped += 1;
        }
    }

    /// Eine leere Lua-Tabelle kommt als `[]`, ein Nicht-Array (`{}`, Zahl, …) heißt: keine Kinder.
    fn nodes(&mut self, v: Option<&Value>) -> Vec<PanelNode> {
        let items = v
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        items.iter().filter_map(|n| self.node(n)).collect()
    }

    fn node(&mut self, v: &Value) -> Option<PanelNode> {
        let kind = v
            .as_object()
            .and_then(|_| v.get("type"))
            .and_then(Value::as_str);
        let Some(kind) = kind else {
            self.warn(tr(
                "Knoten ohne Typ übersprungen",
                "Skipped a node without a type",
            ));
            return None;
        };
        match kind {
            "section" => Some(self.section(v)),
            "item" => self.item(v),
            "text" => self.text(v),
            "button" => self.button(v),
            other => {
                self.warn(tr_args(
                    "Unbekannter Knotentyp „{0}“ übersprungen",
                    "Skipped unknown node type “{0}”",
                    &[other],
                ));
                None
            }
        }
    }

    fn section(&mut self, v: &Value) -> PanelNode {
        PanelNode::Section {
            title: clip(
                v.get("title").and_then(Value::as_str).unwrap_or(""),
                MAX_TEXT,
            ),
            collapsed: v.get("collapsed").and_then(Value::as_bool).unwrap_or(false),
            children: self.nodes(v.get("children")),
        }
    }

    fn item(&mut self, v: &Value) -> Option<PanelNode> {
        let text = self.required(v, "item", "text")?;
        Some(PanelNode::Item {
            text,
            detail: v
                .get("detail")
                .and_then(Value::as_str)
                .map(|s| clip(s, MAX_TEXT)),
            color: self.color(v.get("color")),
            actions: self.actions(v.get("actions")),
        })
    }

    fn text(&mut self, v: &Value) -> Option<PanelNode> {
        let text = self.required(v, "text", "text")?;
        Some(PanelNode::Text(text, self.color(v.get("color"))))
    }

    fn button(&mut self, v: &Value) -> Option<PanelNode> {
        let label = self.required(v, "button", "label")?;
        let action = self.required(v, "button", "action")?;
        Some(PanelNode::Button { label, action })
    }

    fn actions(&mut self, v: Option<&Value>) -> Vec<PanelAction> {
        let items = v
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let mut out = Vec::new();
        for a in items {
            match (
                a.get("id").and_then(Value::as_str),
                a.get("label").and_then(Value::as_str),
            ) {
                (Some(id), Some(label)) => {
                    out.push(PanelAction {
                        id: clip(id, MAX_TEXT),
                        label: clip(label, MAX_TEXT),
                    });
                }
                _ => self.warn(tr(
                    "Aktion ohne id oder label übersprungen",
                    "Skipped an action without id or label",
                )),
            }
        }
        out
    }

    fn color(&mut self, v: Option<&Value>) -> Option<ThemeColor> {
        let v = v?;
        if let Some(color) = v.as_str().and_then(ThemeColor::parse) {
            return Some(color);
        }
        self.warn(unknown_color_warning());
        None
    }

    fn required(&mut self, v: &Value, kind: &str, field: &str) -> Option<String> {
        if let Some(s) = v.get(field).and_then(Value::as_str) {
            return Some(clip(s, MAX_TEXT));
        }
        self.warn(missing_field_warning(kind, field));
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn tree_of(json: &str) -> (PanelTree, Vec<String>) {
        tree(&serde_json::from_str(json).unwrap()).expect("abgelehnt")
    }

    #[test]
    fn palette_validation() {
        let long = "a".repeat(600);
        let v = json!([
            {"id": "gh:a/web", "title": "a/web", "detail": "GitHub", "group": "GitHub"},
            {"title": "ohne id"},
            {"id": "x"},
            {"id": long, "title": long},
        ]);
        let items = palette(Some(&v));
        assert_eq!(items.len(), 2, "Einträge ohne id oder title übersprungen");
        assert_eq!(
            items[0],
            PaletteItem {
                id: "gh:a/web".into(),
                title: "a/web".into(),
                detail: Some("GitHub".into()),
                group: Some("GitHub".into())
            }
        );
        assert_eq!(items[1].id.chars().count(), 500, "id auf MAX_TEXT gekürzt");
        assert!(palette(None).is_empty());
        assert!(
            palette(Some(&json!({}))).is_empty(),
            "Nicht-Array heißt leer"
        );
    }

    #[test]
    fn panel_validation() {
        let long = "a".repeat(600);
        let (t, warnings) = tree_of(&format!(
            r#"{{"title":"T","children":[
              {{"type":"video"}},
              {{"type":"item","text":"{long}","color":"pink","actions":[{{"id":"a","label":"A"}}]}},
              {{"type":"section","title":"S","collapsed":true,"children":[{{"type":"button","label":"B","action":"go"}}]}},
              {{"type":"text","text":"t","color":"muted"}}]}}"#
        ));
        assert_eq!(t.title, "T");
        assert_eq!(
            t.nodes,
            vec![
                PanelNode::Item {
                    text: "a".repeat(500),
                    detail: None,
                    color: None,
                    actions: vec![PanelAction {
                        id: "a".into(),
                        label: "A".into()
                    }]
                },
                PanelNode::Section {
                    title: "S".into(),
                    collapsed: true,
                    children: vec![PanelNode::Button {
                        label: "B".into(),
                        action: "go".into()
                    }]
                },
                PanelNode::Text("t".into(), Some(ThemeColor::Muted)),
            ]
        );
        assert_eq!(warnings.len(), 2, "Typ video und Farbe pink");
    }

    #[test]
    fn panel_tolerates_wrong_shapes() {
        let (empty, w1) = tree_of(r#"{"title":"T","children":[]}"#);
        assert!(empty.nodes.is_empty());
        assert!(w1.is_empty());
        // json.lua kodiert eine leere Tabelle als [], ein Knoten kann also auch [] sein.
        let (t, w2) = tree_of(
            r#"{"children":[[],5,null,"x",{"type":7},{"type":"item"},{"type":"section","children":{}},{"type":"item","text":"a","actions":[{"id":1}]}]}"#,
        );
        assert_eq!(t.title, "");
        assert_eq!(
            t.nodes,
            vec![
                PanelNode::Section {
                    title: String::new(),
                    collapsed: false,
                    children: vec![]
                },
                PanelNode::Item {
                    text: "a".into(),
                    detail: None,
                    color: None,
                    actions: vec![]
                },
            ]
        );
        assert_eq!(
            w2.len(),
            7,
            "fünf Nicht-Knoten, item ohne text, Aktion ohne label"
        );
        let (none, _) = tree_of("[]");
        assert!(none.nodes.is_empty());
    }

    #[test]
    fn panel_too_large() {
        let items = vec![json!({"type": "item", "text": "x"}); 2001];
        let v = json!({"title": "T", "children": items});
        assert_eq!(
            tree(&v).unwrap_err(),
            PanelError::TooLarge(2001),
            "2001 Knoten müssen abgelehnt werden"
        );
    }

    /// Jedes Array-Element zählt zur Grenze, auch Nicht-Knoten und Aktionen.
    #[test]
    fn panel_counts_every_array_element() {
        let v = json!({"children": vec![json!(1); 500_000]});
        assert_eq!(tree(&v).unwrap_err(), PanelError::TooLarge(500_000));
        let actions = vec![json!({"id": "a", "label": "A"}); 2000];
        let w = json!({"children": [{"type": "item", "text": "x", "actions": actions}]});
        assert_eq!(
            tree(&w).unwrap_err(),
            PanelError::TooLarge(2001),
            "2000 Aktionen zählen mit"
        );
    }

    /// Höchstens 20 Warnungen je Baum, dazu eine Sammelzeile.
    #[test]
    fn panel_warnings_are_capped() {
        let v = json!({"children": vec![json!({"type": "video"}); 100]});
        let (_, warnings) = tree(&v).unwrap();
        assert_eq!(warnings.len(), 21);
        assert_eq!(
            warnings.last().unwrap(),
            &tr_args(
                "… und {0} weitere Warnungen",
                "… and {0} more warnings",
                &["80"]
            )
        );
    }

    #[test]
    fn status_truncated_to_40() {
        let v = json!({"text": "x".repeat(60), "color": "ok", "action": "a"});
        let s = status(&v).unwrap();
        assert_eq!(s.text.chars().count(), 40);
        assert_eq!(s.color, Some(ThemeColor::Ok));
        assert_eq!(s.action.as_deref(), Some("a"));
        assert!(status(&json!([])).is_none());
        assert!(status(&json!({"color": "nope"})).is_none());
        assert_eq!(
            status(&json!({"text": "a", "color": "nope"}))
                .unwrap()
                .color,
            None
        );
    }

    #[test]
    fn dock_defaults_to_right() {
        assert_eq!(Dock::placed(Some("left")), Dock::Left);
        assert_eq!(Dock::placed(Some("top")), Dock::Right);
        assert_eq!(Dock::placed(Some("xyz")), Dock::Right);
        assert_eq!(Dock::placed(None), Dock::Right);
    }
}
