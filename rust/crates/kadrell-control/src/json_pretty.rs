//! JSON im Format von Swifts `JSONEncoder` mit `.prettyPrinted`: zwei Leerzeichen je Ebene, `"key" : value` (mit
//! Leerzeichen vor dem Doppelpunkt) und leere Arrays als `[`, Leerzeile, Einrückung, `]`. Die Schlüsselreihenfolge
//! bleibt die des Werts (die Aufrufer liefern sie sortiert, wie `.sortedKeys`).

use serde_json::Value;

/// `value` so formatiert, wie die Mac-App `ls --json` schreibt.
pub fn pretty(value: &Value) -> String {
    let mut out = String::new();
    write_value(value, 0, &mut out);
    out
}

fn indent(level: usize, out: &mut String) {
    out.extend(std::iter::repeat_n("  ", level));
}

fn write_value(value: &Value, level: usize, out: &mut String) {
    match value {
        Value::Array(items) => write_array(items, level, out),
        Value::Object(map) => write_object(map, level, out),
        other => out.push_str(&other.to_string()),
    }
}

fn write_array(items: &[Value], level: usize, out: &mut String) {
    out.push_str("[\n");
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            out.push_str(",\n");
        }
        indent(level + 1, out);
        write_value(item, level + 1, out);
    }
    out.push('\n');
    indent(level, out);
    out.push(']');
}

fn write_object(map: &serde_json::Map<String, Value>, level: usize, out: &mut String) {
    out.push_str("{\n");
    for (i, (key, item)) in map.iter().enumerate() {
        if i > 0 {
            out.push_str(",\n");
        }
        indent(level + 1, out);
        out.push_str(&Value::String(key.clone()).to_string());
        out.push_str(" : ");
        write_value(item, level + 1, out);
    }
    out.push('\n');
    indent(level, out);
    out.push('}');
}

#[cfg(test)]
mod tests {
    use super::*;

    /// So schreibt die Mac-App (`JSONEncoder`, prettyPrinted + sortedKeys) eine Liste mit einer Gruppe ohne Sessions.
    #[test]
    fn matches_the_swift_encoder() {
        let value: Value = serde_json::from_str(
            r##"{"groups":[{"color":"#fab387","favorite":true,"id":"g1","name":"A/B","sessions":[]}],"layout":"grid"}"##,
        )
        .unwrap();
        let swift = "{\n  \"groups\" : [\n    {\n      \"color\" : \"#fab387\",\n      \"favorite\" : true,\n      \"id\" : \"g1\",\n      \"name\" : \"A/B\",\n      \"sessions\" : [\n\n      ]\n    }\n  ],\n  \"layout\" : \"grid\"\n}";
        assert_eq!(pretty(&value), swift);
    }

    #[test]
    fn empty_top_level_array_is_bracket_blank_line_bracket() {
        assert_eq!(pretty(&serde_json::json!([])), "[\n\n]");
        assert_eq!(pretty(&serde_json::json!({"a": [1, null]})), "{\n  \"a\" : [\n    1,\n    null\n  ]\n}");
    }
}
