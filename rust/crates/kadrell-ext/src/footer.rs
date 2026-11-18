//! Einträge der Statusleiste unten (`kadrell.footer.set`).

use std::collections::HashSet;

use serde_json::Value;

use crate::i18n::{tr, tr_args};
use crate::json::clip;
use crate::panel::{MAX_STATUS, MAX_TEXT, ThemeColor, unknown_color_warning};

pub const MAX_FOOTER_ITEMS: usize = 16;
pub const MAX_BARS: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Align {
    #[default]
    Left,
    Right,
}

/// Ein Eintrag der Statusleiste unten. Gezeichnet in dieser Reihenfolge: `label` gedämpft, `bars` als senkrechte
/// Mini-Balken, `gauge` als waagerechter Balken, `text` in `color`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FooterItem {
    pub id: String,
    pub align: Align,
    pub label: Option<String>,
    pub text: Option<String>,
    pub color: Option<ThemeColor>,
    /// Werte 0...1, je einer ein senkrechter Balken (z. B. Last je Kern).
    pub bars: Vec<f64>,
    /// Ein Wert 0...1 als waagerechter Balken (z. B. Plattenbelegung).
    pub gauge: Option<f64>,
    pub action: Option<String>,
    /// Tooltip; ohne steht dort, was sichtbar ist.
    pub tip: Option<String>,
    /// Name in der Kommandopalette (⌘P `>`) und für Screenreader; ohne gilt `label` und `text`.
    pub title: Option<String>,
}

impl FooterItem {
    /// Was Screenreader und die Palette nennen, wenn `title` fehlt.
    pub fn spoken(&self) -> String {
        if let Some(title) = &self.title {
            return title.clone();
        }
        let parts: Vec<&str> =
            [self.label.as_deref(), self.text.as_deref()].into_iter().flatten().filter(|s| !s.is_empty()).collect();
        if parts.is_empty() { self.id.clone() } else { parts.join(" ") }
    }
}

/// Einträge und Warnungen für die Statusleiste unten.
pub struct FooterResult {
    pub items: Vec<FooterItem>,
    pub warnings: Vec<String>,
}

/// `null` oder Nicht-Array heißt leer. Einträge ohne `id` oder ganz ohne Inhalt werden übersprungen und gemeldet, Werte
/// auf 0...1 begrenzt, Anzahl und Längen gedeckelt.
pub fn footer(v: Option<&Value>) -> FooterResult {
    let mut warnings = Vec::new();
    let raw = v.and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[]);
    if raw.len() > MAX_FOOTER_ITEMS {
        let n = MAX_FOOTER_ITEMS.to_string();
        warnings.push(tr_args(
            "Statusleiste: höchstens {0} Einträge, der Rest fehlt",
            "Status bar: at most {0} entries, the rest is left out",
            &[&n],
        ));
    }
    let mut seen = HashSet::new();
    let mut items = Vec::new();
    for entry in raw.iter().take(MAX_FOOTER_ITEMS) {
        let Some(item) = footer_item(entry, &mut warnings) else { continue };
        if !seen.insert(item.id.clone()) {
            warnings.push(tr_args(
                "Statusleiste: id „{0}“ doppelt, übersprungen",
                "Status bar: duplicate id “{0}”, skipped",
                &[&item.id],
            ));
            continue;
        }
        items.push(item);
    }
    FooterResult { items, warnings }
}

fn opt_text(v: &Value, key: &str, limit: usize) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(|s| clip(s, limit))
}

fn footer_item(v: &Value, warnings: &mut Vec<String>) -> Option<FooterItem> {
    let id = v.get("id").and_then(Value::as_str).filter(|s| !s.is_empty());
    let Some(id) = id else {
        warnings.push(tr("Statusleiste: Eintrag ohne „id“ übersprungen", "Status bar: skipped entry without “id”"));
        return None;
    };
    let item = FooterItem {
        id: clip(id, MAX_TEXT),
        align: if v.get("align").and_then(Value::as_str) == Some("right") { Align::Right } else { Align::Left },
        label: opt_text(v, "label", MAX_STATUS),
        text: opt_text(v, "text", MAX_STATUS),
        color: footer_color(v.get("color"), warnings),
        bars: v
            .get("bars")
            .and_then(Value::as_array)
            .map(|a| a.iter().take(MAX_BARS).filter_map(unit).collect())
            .unwrap_or_default(),
        gauge: v.get("gauge").and_then(unit),
        action: opt_text(v, "action", MAX_TEXT),
        tip: opt_text(v, "tip", MAX_TEXT),
        title: opt_text(v, "title", MAX_STATUS),
    };
    if item.label.is_none() && item.text.is_none() && item.bars.is_empty() && item.gauge.is_none() {
        warnings.push(tr_args(
            "Statusleiste: Eintrag „{0}“ ohne Inhalt übersprungen",
            "Status bar: skipped entry “{0}” without content",
            &[&item.id],
        ));
        return None;
    }
    Some(item)
}

fn footer_color(v: Option<&Value>, warnings: &mut Vec<String>) -> Option<ThemeColor> {
    let v = v?;
    if let Some(color) = v.as_str().and_then(ThemeColor::parse) {
        return Some(color);
    }
    warnings.push(unknown_color_warning());
    None
}

/// Zahl, auf 0...1 begrenzt; `NaN` und Nicht-Zahlen ergeben `None`.
fn unit(v: &Value) -> Option<f64> {
    let f = v.as_f64()?;
    (!f.is_nan()).then(|| f.clamp(0.0, 1.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn footer_drops_invalid_entries_and_clamps() {
        let v = json!([
            {"id": "cpu", "align": "right", "label": "CPU", "bars": [0.2, 3, -1, "x"], "gauge": 1.5},
            {"text": "ohne id"},
            {"id": "leer"},
            {"id": "cpu", "text": "doppelt"},
            {"id": "c", "text": "x", "color": "pink"},
        ]);
        let r = footer(Some(&v));
        assert_eq!(r.items.iter().map(|i| i.id.as_str()).collect::<Vec<_>>(), ["cpu", "c"]);
        assert_eq!(r.items[0].bars, vec![0.2, 1.0, 0.0]);
        assert_eq!(r.items[0].gauge, Some(1.0));
        assert_eq!(r.items[0].align, Align::Right);
        assert_eq!(r.warnings.len(), 4);
    }

    #[test]
    fn footer_caps_entry_count() {
        let items: Vec<Value> = (0..20).map(|i| json!({"id": format!("i{i}"), "text": "x"})).collect();
        let r = footer(Some(&Value::Array(items)));
        assert_eq!(r.items.len(), 16);
        assert_eq!(r.warnings.len(), 1);
    }

    #[test]
    fn spoken_falls_back_to_label_text_then_id() {
        let item = FooterItem { id: "i".into(), label: Some("CPU".into()), text: Some("5%".into()), ..Default::default() };
        assert_eq!(item.spoken(), "CPU 5%");
        assert_eq!(FooterItem { id: "i".into(), ..Default::default() }.spoken(), "i");
        assert_eq!(FooterItem { id: "i".into(), title: Some("T".into()), ..item }.spoken(), "T");
    }

    #[test]
    fn non_array_means_empty() {
        assert!(footer(None).items.is_empty());
        assert!(footer(Some(&json!({}))).items.is_empty());
    }
}
