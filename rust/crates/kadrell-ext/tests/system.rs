//! `kadrell.system`, `kadrell.footer` und `app.locale` im ext-host (Port von `SystemProbeTests.swift`).

mod common;

use common::*;
use serde_json::json;

fn disk_root() -> &'static str {
    if cfg!(windows) { "C:/" } else { "/" }
}

fn texts(p: &HostPipe, count: usize) -> Vec<String> {
    (0..count)
        .filter_map(|_| {
            p.next_of("log", WAIT)
                .and_then(|m| m["text"].as_str().map(String::from))
        })
        .collect()
}

#[test]
fn cpu_and_disk_from_lua() {
    let (p, _r) = HostPipe::start(&format!(
        r#"kadrell.on("app.ready", function()
            local c = kadrell.system.cpu()
            kadrell.log("cores " .. tostring(#c.cores > 0) .. " total " .. tostring(c.total >= 0 and c.total <= 1))
            local d = kadrell.system.disk("{}")
            kadrell.log("disk " .. tostring(d.total > 0 and d.free <= d.total and d.fraction >= 0 and d.fraction <= 1))
            local none, err = kadrell.system.disk("/gibt/es/nicht")
            kadrell.log("missing " .. tostring(none) .. " " .. tostring(err ~= nil))
        end)"#,
        disk_root()
    ));
    assert!(p.next_of("ready", WAIT).is_some());
    p.send(json!({"t": "event", "name": "app.ready", "data": {}}));
    assert_eq!(
        texts(&p, 3),
        ["cores true total true", "disk true", "missing nil true"]
    );
}

#[test]
fn mem_from_lua() {
    let (p, _r) = HostPipe::start(
        r#"kadrell.on("app.ready", function()
            local m = kadrell.system.mem()
            kadrell.log("mem " .. tostring(m.total > 0 and m.used <= m.total and m.fraction >= 0 and m.fraction <= 1))
        end)"#,
    );
    assert!(p.next_of("ready", WAIT).is_some());
    p.send(json!({"t": "event", "name": "app.ready", "data": {}}));
    assert_eq!(texts(&p, 1), ["mem true"]);
}

#[test]
fn system_outside_handler_fails() {
    let (p, _r) = HostPipe::start(
        "local ok, err = pcall(kadrell.system.cpu)\nkadrell.log(tostring(ok) .. ' ' .. tostring(err):match('only allowed inside handlers') )",
    );
    assert_eq!(
        p.next_of("log", WAIT)
            .and_then(|m| m["text"].as_str().map(String::from))
            .as_deref(),
        Some("false only allowed inside handlers")
    );
}

#[test]
fn footer_set_sends_only_the_last_list() {
    let (p, _r) = HostPipe::start(
        r#"kadrell.footer.set({ { id = "a", text = "1" } })
        kadrell.footer.set({ { id = "a", text = "2", bars = { 0.5, 1 } } })"#,
    );
    let m = p.next_of("footer", WAIT).expect("footer");
    assert_eq!(m["items"][0]["text"], "2");
    assert_eq!(m["items"][0]["bars"], json!([0.5, 1]));
    assert!(
        p.next_of("footer", 0.3).is_none(),
        "nur ein Stand pro Dispatch"
    );
}

#[test]
fn footer_clear_sends_null() {
    let (p, _r) = HostPipe::start("kadrell.footer.clear()");
    assert!(p.next_of("footer", WAIT).expect("footer")["items"].is_null());
}

#[test]
fn locale_event_updates_before_handlers() {
    let (p, _r) = HostPipe::start(
        r#"kadrell.on("app.locale", function(d) kadrell.log(kadrell.locale .. "/" .. d.locale) end)"#,
    );
    assert!(p.next_of("ready", WAIT).is_some());
    p.send(json!({"t": "event", "name": "app.locale", "data": {"locale": "en"}}));
    assert_eq!(
        p.next_of("log", WAIT)
            .and_then(|m| m["text"].as_str().map(String::from))
            .as_deref(),
        Some("en/en")
    );
}

/// Der CPU-Wert kommt auch ein zweites Mal sofort (ohne die 250 ms des ersten Aufrufs).
#[test]
fn second_cpu_call_is_quick() {
    let (p, _r) = HostPipe::start(
        r#"kadrell.on("app.ready", function()
            kadrell.system.cpu()
            local c = kadrell.system.cpu()
            kadrell.log(tostring(c ~= nil))
        end)"#,
    );
    assert!(p.next_of("ready", WAIT).is_some());
    p.send(json!({"t": "event", "name": "app.ready", "data": {}}));
    assert_eq!(texts(&p, 1), ["true"]);
}
