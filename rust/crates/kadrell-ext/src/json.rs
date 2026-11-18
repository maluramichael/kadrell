//! Kleine Helfer für `serde_json::Value`, die das Swift-`JSONValue` nachbilden.

use serde_json::Value;

/// Ganzzahl wie in Swift: json.lua schreibt `3.0` als `3`, ein Bruch oder eine riesige Zahl ist keine id.
pub fn as_int(v: &Value) -> Option<i64> {
    let n = v.as_number()?;
    if let Some(i) = n.as_i64() {
        return (i.unsigned_abs() < 1_000_000_000_000_000).then_some(i);
    }
    let f = n.as_f64()?;
    (f == f.round() && f.abs() < 1e15).then_some(f as i64)
}

/// Text eines Schlüssels, `None` bei fehlendem Schlüssel oder anderem Typ.
pub fn str_of<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key)?.as_str()
}

/// Die ersten `limit` Zeichen (nicht Bytes) eines Textes.
pub fn clip(s: &str, limit: usize) -> String {
    s.chars().take(limit).collect()
}

/// Zahl ohne überflüssiges ".0", so wie sie jemand eintippen würde.
pub fn number_text(v: &Value) -> String {
    match v {
        Value::Number(n) => match n.as_f64() {
            Some(f) if f == f.round() && f.abs() < 1e15 => format!("{}", f as i64),
            Some(f) => format!("{f}"),
            None => n.to_string(),
        },
        Value::String(s) => s.clone(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn int_accepts_integral_floats_only() {
        assert_eq!(as_int(&json!(3)), Some(3));
        assert_eq!(
            as_int(&serde_json::from_str::<Value>("3.0").unwrap()),
            Some(3)
        );
        assert_eq!(as_int(&json!(3.5)), None);
        assert_eq!(as_int(&json!(1e16)), None);
        assert_eq!(as_int(&json!("3")), None);
    }

    #[test]
    fn number_text_drops_trailing_zero() {
        assert_eq!(number_text(&json!(5)), "5");
        assert_eq!(number_text(&json!(5.0)), "5");
        assert_eq!(number_text(&json!(2.5)), "2.5");
        assert_eq!(number_text(&json!("x")), "x");
    }
}
