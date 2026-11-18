//! Protokoll zwischen Kadrell und dem Extension-Helper: eine JSON-Zeile pro Nachricht, Unterscheidung über `t`.

use serde_json::{Map, Value, json};

use crate::json::{as_int, str_of};

/// Nachrichten einer Extension an Kadrell.
#[derive(Debug, Clone, PartialEq)]
pub enum ExtensionMessage {
    Ready,
    Pong(i64),
    Panel {
        tree: Option<Value>,
        dock: Option<String>,
    },
    Status(Option<Value>),
    Palette(Option<Value>),
    Footer(Option<Value>),
    Run {
        id: i64,
        argv: Vec<String>,
    },
    Secret {
        id: i64,
        op: String,
        key: String,
        value: Option<String>,
    },
    Log {
        level: String,
        text: String,
    },
}

impl ExtensionMessage {
    /// Kaputtes JSON, Binärmüll, unbekanntes `t` oder falsche Feldtypen ergeben `None`; der Aufrufer verwirft die Zeile.
    pub fn decode(line: &[u8]) -> Option<ExtensionMessage> {
        let v: Value = serde_json::from_slice(line).ok()?;
        match str_of(&v, "t")? {
            "ready" => Some(Self::Ready),
            "pong" => v.get("id").and_then(as_int).map(Self::Pong),
            "panel" => Some(Self::Panel {
                tree: non_null(&v, "tree"),
                dock: str_of(&v, "dock").map(String::from),
            }),
            "status" => Some(Self::Status(non_null(&v, "item"))),
            "palette" => Some(Self::Palette(non_null(&v, "items"))),
            "footer" => Some(Self::Footer(non_null(&v, "items"))),
            "run" => decode_run(&v),
            "secret" => decode_secret(&v),
            "log" => str_of(&v, "text").map(|text| Self::Log {
                level: str_of(&v, "level").unwrap_or("info").to_string(),
                text: text.to_string(),
            }),
            _ => None,
        }
    }
}

fn non_null(v: &Value, key: &str) -> Option<Value> {
    v.get(key).filter(|x| !x.is_null()).cloned()
}

fn decode_run(v: &Value) -> Option<ExtensionMessage> {
    let id = as_int(v.get("id")?)?;
    let items = v.get("argv")?.as_array()?;
    let argv: Vec<String> = items
        .iter()
        .filter_map(|x| x.as_str().map(String::from))
        .collect();
    (argv.len() == items.len()).then_some(ExtensionMessage::Run { id, argv })
}

fn decode_secret(v: &Value) -> Option<ExtensionMessage> {
    Some(ExtensionMessage::Secret {
        id: as_int(v.get("id")?)?,
        op: str_of(v, "op")?.to_string(),
        key: str_of(v, "key")?.to_string(),
        value: str_of(v, "value").map(String::from),
    })
}

/// Nachrichten von Kadrell an eine Extension; `line()` ist genau eine Zeile für stdin.
#[derive(Debug, Clone, PartialEq)]
pub enum HostMessage {
    Hello {
        api: i64,
        name: String,
        dir: String,
        storage_dir: String,
        config: Map<String, Value>,
        locale: String,
        sessions: Vec<Value>,
    },
    Event {
        name: String,
        data: Value,
    },
    Result {
        id: i64,
        status: i32,
        stdout: String,
        stderr: String,
    },
    Ping(i64),
    Shutdown,
}

impl HostMessage {
    pub fn to_value(&self) -> Value {
        match self {
            Self::Hello {
                api,
                name,
                dir,
                storage_dir,
                config,
                locale,
                sessions,
            } => json!({
                "t": "hello", "api": api, "name": name, "dir": dir, "storageDir": storage_dir,
                "config": config, "locale": locale, "sessions": sessions,
            }),
            Self::Event { name, data } => json!({ "t": "event", "name": name, "data": data }),
            Self::Result {
                id,
                status,
                stdout,
                stderr,
            } => {
                json!({ "t": "result", "id": id, "status": status, "stdout": stdout, "stderr": stderr })
            }
            Self::Ping(id) => json!({ "t": "ping", "id": id }),
            Self::Shutdown => json!({ "t": "shutdown" }),
        }
    }

    /// Genau eine Zeile mit abschließendem Zeilenumbruch; Umbrüche in Werten sind maskiert.
    pub fn line(&self) -> Vec<u8> {
        let mut data = serde_json::to_vec(&self.to_value()).unwrap_or_else(|_| b"{}".to_vec());
        data.push(b'\n');
        data
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dec(s: &str) -> Option<ExtensionMessage> {
        ExtensionMessage::decode(s.as_bytes())
    }

    #[test]
    fn decode_ignores_garbage() {
        assert_eq!(dec("nicht json"), None);
        assert_eq!(ExtensionMessage::decode(&[0xff, 0xfe]), None);
        assert_eq!(dec(r#"{"t":"wat"}"#), None);
        assert_eq!(dec("[1,2]"), None);
        assert_eq!(dec(r#"{"t":"run","id":"x","argv":["ls"]}"#), None);
        assert_eq!(dec(r#"{"t":"run","id":1,"argv":[1]}"#), None);
    }

    #[test]
    fn decode_run_and_log() {
        assert_eq!(
            dec(r#"{"t":"run","id":3,"argv":["ls"]}"#),
            Some(ExtensionMessage::Run {
                id: 3,
                argv: vec!["ls".into()]
            })
        );
        assert_eq!(
            dec(r#"{"t":"run","id":3.0,"argv":[]}"#),
            Some(ExtensionMessage::Run {
                id: 3,
                argv: vec![]
            })
        );
        assert_eq!(
            dec(r#"{"t":"log","level":"warn","text":"hi"}"#),
            Some(ExtensionMessage::Log {
                level: "warn".into(),
                text: "hi".into()
            })
        );
        assert_eq!(dec(r#"{"t":"ready"}"#), Some(ExtensionMessage::Ready));
        assert_eq!(
            dec(r#"{"t":"pong","id":7}"#),
            Some(ExtensionMessage::Pong(7))
        );
    }

    #[test]
    fn decode_panel_and_status_null() {
        assert_eq!(
            dec(r#"{"t":"panel","tree":null}"#),
            Some(ExtensionMessage::Panel {
                tree: None,
                dock: None
            })
        );
        assert_eq!(
            dec(r#"{"t":"status","item":null}"#),
            Some(ExtensionMessage::Status(None))
        );
        assert_eq!(
            dec(r#"{"t":"status","item":{"text":"a"}}"#),
            Some(ExtensionMessage::Status(Some(json!({"text": "a"}))))
        );
        assert_eq!(
            dec(r#"{"t":"panel","tree":{"title":"A"},"dock":"left"}"#),
            Some(ExtensionMessage::Panel {
                tree: Some(json!({"title": "A"})),
                dock: Some("left".into())
            })
        );
        assert_eq!(
            dec(r#"{"t":"footer","items":null}"#),
            Some(ExtensionMessage::Footer(None))
        );
    }

    #[test]
    fn decode_palette_and_secret() {
        assert_eq!(
            dec(r#"{"t":"palette","items":null}"#),
            Some(ExtensionMessage::Palette(None))
        );
        assert_eq!(
            dec(r#"{"t":"palette","items":[{"id":"a","title":"A"}]}"#),
            Some(ExtensionMessage::Palette(Some(
                json!([{"id": "a", "title": "A"}])
            )))
        );
        assert_eq!(
            dec(r#"{"t":"secret","id":5,"op":"get","key":"tok"}"#),
            Some(ExtensionMessage::Secret {
                id: 5,
                op: "get".into(),
                key: "tok".into(),
                value: None
            })
        );
        assert_eq!(
            dec(r#"{"t":"secret","id":6,"op":"set","key":"tok","value":"v"}"#),
            Some(ExtensionMessage::Secret {
                id: 6,
                op: "set".into(),
                key: "tok".into(),
                value: Some("v".into())
            })
        );
        assert_eq!(
            dec(r#"{"t":"secret","op":"get","key":"tok"}"#),
            None,
            "ohne id verworfen"
        );
        assert_eq!(
            dec(r#"{"t":"secret","id":1,"op":"get"}"#),
            None,
            "ohne key verworfen"
        );
    }

    #[test]
    fn host_message_line_ends_with_newline() {
        let mut config = Map::new();
        config.insert("k".into(), json!("v\nw"));
        let hello = HostMessage::Hello {
            api: 1,
            name: "x".into(),
            dir: "/a/b".into(),
            storage_dir: "/s".into(),
            config,
            locale: "de".into(),
            sessions: vec![],
        };
        let line = hello.line();
        assert_eq!(line.last(), Some(&b'\n'));
        assert_eq!(
            line.iter().filter(|b| **b == b'\n').count(),
            1,
            "Zeilenumbruch im Wert wird maskiert"
        );
        let v: Value = serde_json::from_slice(&line).unwrap();
        assert_eq!(v["t"], "hello");
        assert_eq!(v["config"]["k"], "v\nw");
        assert_eq!(v["storageDir"], "/s");
        assert_eq!(v["sessions"], json!([]));
        let result: Value = serde_json::from_slice(
            &HostMessage::Result {
                id: 4,
                status: 1,
                stdout: "o".into(),
                stderr: "e".into(),
            }
            .line(),
        )
        .unwrap();
        assert_eq!(as_int(&result["id"]), Some(4));
        assert_eq!(result["stderr"], "e");
        assert_eq!(HostMessage::Shutdown.line(), b"{\"t\":\"shutdown\"}\n");
    }
}
