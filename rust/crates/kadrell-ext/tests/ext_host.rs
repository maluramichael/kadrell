//! `ext-host`: Lua-Helper mit Prelude, Protokoll über JSON-Zeilen (Port von `ExtHostTests.swift`).

mod common;

use std::time::Duration;

use common::*;
use serde_json::{Value, json};

fn s(v: &Option<Value>, key: &str) -> Option<String> {
    v.as_ref().and_then(|m| m[key].as_str().map(String::from))
}

#[test]
fn hello_ready_ping_pong() {
    let (p, _r) = HostPipe::start("kadrell.log('geladen')");
    let log = p.next(3.0);
    assert_eq!(s(&log, "t").as_deref(), Some("log"));
    assert_eq!(s(&log, "text").as_deref(), Some("geladen"));
    assert_eq!(s(&p.next(3.0), "t").as_deref(), Some("ready"));
    p.send(json!({"t": "ping", "id": 7}));
    let pong = p.next(3.0).unwrap();
    assert_eq!(pong["t"], "pong");
    assert_eq!(pong["id"], 7);
}

#[test]
fn locale_is_available_at_load() {
    let (p, _r) = HostPipe::start("kadrell.log('loc:' .. kadrell.locale)");
    assert_eq!(
        p.text(3.0).as_deref(),
        Some("loc:de"),
        "hello liefert locale=de"
    );
}

#[test]
fn print_goes_to_log() {
    let (p, _r) = HostPipe::start("print('x', 1)");
    let log = p.next(3.0);
    assert_eq!(s(&log, "t").as_deref(), Some("log"));
    assert_eq!(s(&log, "level").as_deref(), Some("info"));
    assert_eq!(s(&log, "text").as_deref(), Some("x\t1"));
    assert_eq!(s(&p.next(3.0), "t").as_deref(), Some("ready"));
}

#[test]
fn load_error_is_reported_then_exit() {
    let (mut p, _r) = HostPipe::start("error('kaputt')");
    let log = p.next(3.0);
    assert_eq!(s(&log, "t").as_deref(), Some("log"));
    assert_eq!(s(&log, "level").as_deref(), Some("error"));
    let text = s(&log, "text").unwrap_or_default();
    assert!(text.contains("init.lua:1"), "{text}");
    assert!(text.contains("kaputt"), "{text}");
    let status = p.wait_for_exit(3.0).expect("Prozessende");
    assert!(!status.success());
}

#[test]
fn blocked_functions_are_gone() {
    let (p, _r) = HostPipe::start(
        "kadrell.log(tostring(os.execute)..tostring(io.popen)..tostring(package.loadlib)..tostring(os.exit))",
    );
    assert_eq!(p.text(3.0).as_deref(), Some("nilnilnilnil"));
}

#[test]
fn c_module_searchers_are_gone() {
    let (p, _r) = HostPipe::start("kadrell.log(#package.searchers .. ':' .. package.cpath .. ':')");
    assert_eq!(p.text(3.0).as_deref(), Some("2::"));
}

#[test]
fn json_roundtrip() {
    let (p, _r) = HostPipe::start(
        r#"kadrell.log(kadrell.json.encode(kadrell.json.decode('{"a":[1,2],"b":"ü"}')))"#,
    );
    let text = p.text(3.0).unwrap_or_default();
    assert!(text.contains(r#""a":[1,2]"#), "{text}");
    assert!(text.contains('ü'), "{text}");
}

/// Tiefe C-Rekursion (verschachtelte pcall) ist ein Lua-Fehler, kein Absturz des Helpers: der Fehler wird durch alle
/// Ebenen weitergereicht, init.lua lädt nicht, der Helper endet regulär mit Status 1.
#[test]
fn deep_recursion_is_a_lua_error() {
    let (mut p, _r) =
        HostPipe::start("local function f() local ok, err = pcall(f); error(err, 0) end\nf()");
    let log = p.next(3.0);
    assert_eq!(s(&log, "level").as_deref(), Some("error"));
    let text = s(&log, "text").unwrap_or_default();
    assert!(text.contains("stack overflow"), "{text}");
    let status = p.wait_for_exit(3.0).expect("Prozessende");
    assert_eq!(status.code(), Some(1));
}

/// Rekursives __index in einem Event-Handler: Fehler ins Log, die Extension läuft weiter.
#[test]
fn recursive_index_in_handler_is_logged() {
    let (p, _r) = HostPipe::start(
        "local t = setmetatable({}, { __index = function(t, k) return t[k] end })\nkadrell.on(\"app.ready\", function() return t.x end)",
    );
    assert_eq!(s(&p.next(3.0), "t").as_deref(), Some("ready"));
    p.send(json!({"t": "event", "name": "app.ready", "data": {}}));
    let log = p.next(3.0);
    assert_eq!(s(&log, "level").as_deref(), Some("error"));
    let text = s(&log, "text").unwrap_or_default();
    assert!(text.contains("stack overflow"), "{text}");
    p.assert_alive(3);
}

#[test]
fn stdin_eof_ends_helper() {
    let (mut p, _r) = HostPipe::start("");
    assert_eq!(s(&p.next(3.0), "t").as_deref(), Some("ready"));
    p.close_stdin();
    assert!(p.wait_for_exit(2.0).is_some());
}

/// Kadrell schließt nur stdin (etwa beim Absturz der App). Ein laufendes kadrell.exec stirbt mit dem Helper.
#[test]
fn stdin_eof_kills_exec_children() {
    let (mut p, _r) = HostPipe::start_ready(&format!(
        r#"kadrell.on("app.ready", function() kadrell.exec({{"{}", "sleep", "77"}}) end)"#,
        fake_lua()
    ));
    assert!(wait_until(3.0, || !children_of(p.pid()).is_empty()));
    let child = children_of(p.pid())[0];
    p.close_stdin();
    assert!(p.wait_for_exit(2.0).is_some());
    assert!(
        wait_until(3.0, || !alive(child)),
        "sleep 77 ({child}) lebt nach EOF weiter"
    );
}

/// Der Helper selbst wird hart beendet (Kill von außen): das Job Object nimmt die Kinder mit.
#[test]
fn killing_the_helper_kills_exec_children() {
    let (mut p, _r) = HostPipe::start_ready(&format!(
        r#"kadrell.on("app.ready", function() kadrell.exec({{"{}", "sleep", "77"}}) end)"#,
        fake_lua()
    ));
    assert!(wait_until(3.0, || !children_of(p.pid()).is_empty()));
    let child = children_of(p.pid())[0];
    p.child.kill().unwrap();
    let _ = p.wait_for_exit(2.0);
    // Unter Unix bleiben Kinder nach SIGKILL des Helpers übrig (die App tötet dort die Prozessgruppe), unter Windows
    // sorgt das Job Object dafür.
    if cfg!(windows) {
        assert!(
            wait_until(3.0, || !alive(child)),
            "sleep 77 ({child}) lebt nach dem Kill weiter"
        );
    }
}

#[test]
fn exec_inside_handler() {
    let (p, _r) = HostPipe::start_ready(&format!(
        r#"kadrell.on("app.ready", function()
            local r = kadrell.exec({{"{}", "echo", "hi"}})
            kadrell.log(r.status .. ":" .. r.stdout)
        end)"#,
        fake_lua()
    ));
    assert_eq!(p.text(3.0).as_deref(), Some("0:hi\n"));
}

#[test]
fn exec_missing_binary() {
    let (p, _r) = HostPipe::start_ready(
        r#"kadrell.on("app.ready", function() kadrell.log(kadrell.exec({"/nope/nicht-da"}).status) end)"#,
    );
    assert_eq!(p.text(3.0).as_deref(), Some("127"));
    p.assert_alive(9);
}

#[test]
fn exec_returns_stderr_separately() {
    let (p, _r) = HostPipe::start_ready(&format!(
        r#"kadrell.on("app.ready", function()
            local r = kadrell.exec({{"{}", "stderr-exit", 3, "e"}})
            kadrell.log(r.status .. "|" .. r.stdout .. "|" .. r.stderr)
        end)"#,
        fake_lua()
    ));
    assert_eq!(p.text(3.0).as_deref(), Some("3||e\n"));
}

#[test]
fn exec_resolves_via_path() {
    let (p, _r) = HostPipe::start_ready(
        r#"kadrell.on("app.ready", function() kadrell.log(kadrell.exec({"kadrell-ext-fake", "echo", "x"}).stdout) end)"#,
    );
    assert_eq!(p.text(3.0).as_deref(), Some("x\n"));
}

#[test]
fn exec_timeout_kills_the_child() {
    let (p, _r) = HostPipe::start_ready(&format!(
        r#"kadrell.on("app.ready", function()
            local r = kadrell.exec({{"{}", "sleep", "60"}}, {{ timeout = 0.3 }})
            kadrell.log("status " .. r.status)
        end)"#,
        fake_lua()
    ));
    assert_eq!(p.text(5.0).as_deref(), Some("status 9"));
    p.assert_alive(9);
}

#[test]
fn exec_honors_cwd() {
    let dir = TempDir::new("cwd");
    let lua_dir = dir.path().to_string_lossy().replace('\\', "/");
    let (p, _r) = HostPipe::start_ready(&format!(
        r#"kadrell.on("app.ready", function()
            local r = kadrell.exec({{"{}", "echo", "x"}}, {{ cwd = "{lua_dir}" }})
            local bad = kadrell.exec({{"{}", "echo", "x"}}, {{ cwd = "{lua_dir}/gibt-es-nicht" }})
            kadrell.log(r.status .. " " .. bad.status)
        end)"#,
        fake_lua(),
        fake_lua()
    ));
    assert_eq!(p.text(3.0).as_deref(), Some("0 127"));
}

#[test]
fn handler_error_is_logged_not_fatal() {
    let (p, _r) = HostPipe::start_ready(r#"kadrell.on("app.ready", function() error("boom") end)"#);
    let log = p.next(3.0);
    assert_eq!(s(&log, "level").as_deref(), Some("error"));
    assert!(s(&log, "text").unwrap_or_default().contains("boom"));
    p.assert_alive(9);
}

#[test]
fn call_outside_handler_fails() {
    let (mut p, _r) = HostPipe::start("kadrell.exec({\"x\"})");
    let log = p.next(3.0);
    assert_eq!(s(&log, "level").as_deref(), Some("error"));
    let text = s(&log, "text").unwrap_or_default();
    assert!(
        text.contains("kadrell.exec: only allowed inside handlers (kadrell.on, every, after)"),
        "{text}"
    );
    let status = p.wait_for_exit(3.0).expect("Prozessende");
    assert!(!status.success());
}

#[test]
fn timer_fires() {
    let (p, _r) = HostPipe::start_ready(
        r#"kadrell.on("app.ready", function() kadrell.after(0.1, function() kadrell.log("t") end) end)"#,
    );
    assert_eq!(p.text(1.0).as_deref(), Some("t"));
}

#[test]
fn every_repeats_and_cancels() {
    let (p, _r) = HostPipe::start_ready(
        r#"local n = 0
        kadrell.on("app.ready", function()
            local h
            h = kadrell.every(0.05, function()
                n = n + 1
                kadrell.log("e" .. n)
                if n == 2 then h:cancel() end
            end)
        end)
        kadrell.on("check", function() kadrell.log("n=" .. n) end)"#,
    );
    assert_eq!(p.text(1.0).as_deref(), Some("e1"));
    assert_eq!(p.text(1.0).as_deref(), Some("e2"));
    std::thread::sleep(Duration::from_millis(300));
    p.send(json!({"t": "event", "name": "check", "data": {}}));
    assert_eq!(p.text(1.0).as_deref(), Some("n=2"));
}

#[test]
fn run_roundtrip() {
    let (p, _r) = HostPipe::start_ready(
        r#"kadrell.on("app.ready", function()
            local r = kadrell.run("ls", "--json")
            kadrell.log(r.status .. r.stdout)
        end)"#,
    );
    let run = p.next(3.0).unwrap();
    assert_eq!(run["t"], "run");
    assert_eq!(run["argv"], json!(["ls", "--json"]));
    p.send(json!({"t": "result", "id": run["id"], "status": 0, "stdout": "{}", "stderr": ""}));
    assert_eq!(p.text(3.0).as_deref(), Some("0{}"));
}

/// Zahlen im argv gehen als Strings raus, sonst verwürfe Kadrell die Zeile und die Coroutine wartete ewig.
#[test]
fn run_converts_arguments_to_strings() {
    let (p, _r) = HostPipe::start_ready(
        r#"kadrell.on("app.ready", function() kadrell.run("select", "-t", 1) end)"#,
    );
    assert_eq!(p.next(3.0).unwrap()["argv"], json!(["select", "-t", "1"]));
}

#[test]
fn exec_converts_arguments_to_strings() {
    let (p, _r) = HostPipe::start_ready(&format!(
        r#"kadrell.on("app.ready", function() kadrell.log(kadrell.exec({{"{}", "echo", 1, true}}).stdout) end)"#,
        fake_lua()
    ));
    assert_eq!(p.text(3.0).as_deref(), Some("1 true\n"));
}

/// sessions() liefert das Objekt von `kadrell ls --json` unverändert.
#[test]
fn sessions_parses_ls_json() {
    let (p, _r) = HostPipe::start_ready(
        r#"kadrell.on("app.ready", function()
            local s = kadrell.sessions()
            kadrell.log(s.layout .. " " .. s.focused .. " " .. s.groups[1].name .. " " .. s.groups[1].sessions[1].key)
        end)"#,
    );
    let run = p.next(3.0).unwrap();
    assert_eq!(run["argv"], json!(["ls", "--json"]));
    let ls = r#"{"focused":"a1","groups":[{"color":"blue","cwd":"/tmp","favorite":false,"id":"g1","name":"G","sessions":[{"branch":null,"cwd":"/tmp","focused":true,"key":"a1","running":true,"sessionId":"s1","shown":true,"status":"idle","title":"T"}]}],"layout":"grid"}"#;
    p.send(json!({"t": "result", "id": run["id"], "status": 0, "stdout": ls, "stderr": ""}));
    assert_eq!(p.text(3.0).as_deref(), Some("grid a1 G a1"));
}

#[test]
fn http_roundtrip() {
    let server = OneShotHttpServer::new(
        "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nX-A: b\r\nConnection: close\r\n\r\nhi",
    );
    let (p, _r) = HostPipe::start_ready(&format!(
        r#"kadrell.on("app.ready", function()
            local r = kadrell.http{{url = "http://127.0.0.1:{}/x", method = "POST", body = "q", headers = {{["X-T"] = "1"}}}}
            kadrell.log(r.status .. ":" .. r.body .. ":" .. r.headers["X-A"])
        end)"#,
        server.port
    ));
    assert_eq!(p.text(8.0).as_deref(), Some("200:hi:b"));
    let request = server.request();
    assert!(request.starts_with("POST /x"), "{request}");
    assert!(request.to_lowercase().contains("x-t: 1"), "{request}");
}

/// Eine Zahl als Header-Wert darf die übrigen Header nicht mitnehmen.
#[test]
fn http_numeric_header_keeps_others() {
    let server = OneShotHttpServer::new(
        "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nhi",
    );
    let (p, _r) = HostPipe::start_ready(&format!(
        r#"kadrell.on("app.ready", function()
            local r = kadrell.http{{url = "http://127.0.0.1:{}/x", method = "POST", body = "hello world!",
                headers = {{["Content-Length"] = 12, Authorization = "Bearer t"}}}}
            kadrell.log(r.status)
        end)"#,
        server.port
    ));
    assert_eq!(p.text(8.0).as_deref(), Some("200"));
    let request = server.request().to_lowercase();
    assert!(request.contains("authorization: bearer t"), "{request}");
    assert!(request.contains("content-length: 12"), "{request}");
}

#[test]
fn http_rejects_other_schemes() {
    let (p, _r) = HostPipe::start_ready(
        r#"kadrell.on("app.ready", function()
            local r = kadrell.http{url = "file:///etc/passwd"}
            kadrell.log(r.status .. ":" .. tostring(r.error))
        end)"#,
    );
    assert_eq!(
        p.text(3.0).as_deref(),
        Some("0:URL muss http oder https sein")
    );
}

#[test]
fn storage_survives_restart() {
    let storage = TempDir::new("storage");
    let (mut first, _r1) = HostPipe::start_in(
        r#"kadrell.on("app.ready", function() kadrell.storage.set("k", {a = 1}); kadrell.log("ok") end)"#,
        storage.path(),
    );
    first.expect_ready();
    assert_eq!(first.text(3.0).as_deref(), Some("ok"));
    first.close_stdin();
    assert!(first.wait_for_exit(2.0).is_some());
    let (second, _r2) = HostPipe::start_in(
        r#"kadrell.on("app.ready", function() kadrell.log(kadrell.storage.get("k").a) end)"#,
        storage.path(),
    );
    second.expect_ready();
    assert_eq!(second.text(3.0).as_deref(), Some("1"));
}

/// Windows ersetzt mit dem C-`rename` keine vorhandene Datei: ein zweites `storage.set` darf trotzdem klappen.
#[test]
fn storage_can_be_overwritten() {
    let storage = TempDir::new("storage");
    let (p, _r) = HostPipe::start_in(
        r#"kadrell.on("app.ready", function()
            kadrell.storage.set("k", 1)
            kadrell.storage.set("k", 2)
            kadrell.storage.set("j", 3)
            kadrell.log(kadrell.storage.get("k") .. ":" .. kadrell.storage.get("j"))
        end)"#,
        storage.path(),
    );
    p.expect_ready();
    assert_eq!(p.text(3.0).as_deref(), Some("2:3"));
}

/// Panel und Status gehen je Handler-Lauf einmal raus, mit dem letzten Stand, nach allem anderen.
#[test]
fn panel_and_status_are_forwarded() {
    let (p, _r) = HostPipe::start_ready(
        r#"kadrell.on("app.ready", function()
            for i = 1, 1000 do kadrell.panel.set{title = "T" .. i} end
            kadrell.status.set{text = "s"}
            kadrell.status.clear()
            kadrell.log("x")
        end)
        kadrell.on("clear", function() kadrell.panel.clear() end)"#,
    );
    assert_eq!(p.text(3.0).as_deref(), Some("x"));
    let panel = p.next(3.0).unwrap();
    assert_eq!(panel["t"], "panel");
    assert_eq!(panel["tree"]["title"], "T1000");
    let status = p.next(3.0).unwrap();
    assert_eq!(status["t"], "status");
    assert!(status["item"].is_null(), "{status}");
    p.send(json!({"t": "event", "name": "clear", "data": {}}));
    assert!(p.next(3.0).unwrap()["tree"].is_null());
    p.assert_alive(9);
}

/// Palette geht wie Panel einmal pro Handler-Lauf mit dem letzten Stand raus; die Auswahl kommt als Event zurück.
#[test]
fn palette_is_forwarded_and_select_delivered() {
    let (p, _r) = HostPipe::start_ready(
        r#"kadrell.on("app.ready", function()
            for i = 1, 100 do kadrell.palette.set{{id = "r" .. i, title = "R" .. i}} end
            kadrell.log("x")
        end)
        kadrell.on("palette.select", function(d) kadrell.log("sel:" .. d.id) end)"#,
    );
    assert_eq!(p.text(3.0).as_deref(), Some("x"));
    let palette = p.next(3.0).unwrap();
    assert_eq!(palette["t"], "palette");
    assert_eq!(palette["items"][0]["id"], "r100");
    p.send(json!({"t": "event", "name": "palette.select", "data": {"id": "r100"}}));
    assert_eq!(p.text(3.0).as_deref(), Some("sel:r100"));
    p.assert_alive(9);
}

/// kadrell.secret.get fragt den Host und gibt bei Status 0 den Wert zurück.
#[test]
fn secret_get_returns_value() {
    let (p, _r) = HostPipe::start_ready(
        r#"kadrell.on("app.ready", function() kadrell.log("got:" .. tostring(kadrell.secret.get("tok"))) end)"#,
    );
    let req = p.next(3.0).unwrap();
    assert_eq!(req["t"], "secret");
    assert_eq!(req["op"], "get");
    assert_eq!(req["key"], "tok");
    p.send(json!({"t": "result", "id": req["id"], "status": 0, "stdout": "s3cr3t", "stderr": ""}));
    assert_eq!(p.text(3.0).as_deref(), Some("got:s3cr3t"));
    p.assert_alive(9);
}

/// Fehlt der Eintrag (Status 1), gibt get nil zurück.
#[test]
fn secret_get_missing_returns_nil() {
    let (p, _r) = HostPipe::start_ready(
        r#"kadrell.on("app.ready", function() kadrell.log("got:" .. tostring(kadrell.secret.get("none"))) end)"#,
    );
    let req = p.next(3.0).unwrap();
    p.send(json!({"t": "result", "id": req["id"], "status": 1, "stdout": "", "stderr": ""}));
    assert_eq!(p.text(3.0).as_deref(), Some("got:nil"));
    p.assert_alive(9);
}

/// Stray-Ausgabe auf stdout (io.write) landet im Log (stderr), nicht im Protokoll.
#[test]
fn io_write_goes_to_stderr_not_protocol() {
    let (p, _r) = HostPipe::start_ready(
        r#"kadrell.on("app.ready", function() io.write("junk-zeile\n"); io.stdout:write("noch-eine\n"); kadrell.log("danach") end)"#,
    );
    assert_eq!(p.text(3.0).as_deref(), Some("danach"));
    assert!(
        wait_until(3.0, || p.stderr_text().contains("junk-zeile")
            && p.stderr_text().contains("noch-eine")),
        "{}",
        p.stderr_text()
    );
    p.assert_alive(9);
}

/// Speicher ist auf 64 MB gedeckelt: ein Fehler im Handler, der Helper läuft weiter.
#[test]
fn memory_limit_is_a_lua_error() {
    let (p, _r) = HostPipe::start_ready(
        r#"kadrell.on("app.ready", function()
            local t = {}
            for i = 1, 200 do t[i] = string.rep("x", 1024 * 1024) .. i end
        end)"#,
    );
    let log = p.next(10.0);
    assert_eq!(s(&log, "level").as_deref(), Some("error"));
    assert!(
        s(&log, "text").unwrap_or_default().contains("memory"),
        "{log:?}"
    );
    p.assert_alive(9);
}

#[test]
fn require_loads_lua_modules_from_the_extension_dir_only() {
    let root = TempDir::new("host");
    let dir = make_extension(
        root.path(),
        "hello",
        r#"local m = require("helper"); kadrell.log(m.greet())
        local ok, err = pcall(require, "gibt.es.nicht"); kadrell.log(tostring(ok))"#,
        None,
    );
    std::fs::write(
        dir.join("helper.lua"),
        "return { greet = function() return 'hallo' end }",
    )
    .unwrap();
    let p = HostPipe::new(&dir);
    p.send(json!({"t": "hello", "api": 1, "name": "hello", "dir": dir.to_string_lossy(), "storageDir": root.path().to_string_lossy(),
        "config": {}, "locale": "de", "sessions": []}));
    assert_eq!(p.text(3.0).as_deref(), Some("hallo"));
    assert_eq!(p.text(3.0).as_deref(), Some("false"));
}

#[cfg(unix)]
#[test]
fn exec_child_cannot_write_into_protocol() {
    let (p, _r) = HostPipe::start_ready(
        r#"kadrell.on("app.ready", function()
            kadrell.exec({"/bin/sh", "-c", "for fd in 3 4 5 6 7 8 9 10 11 12; do echo junk >&$fd; done 2>/dev/null; true"})
            kadrell.log("danach")
        end)"#,
    );
    assert_eq!(p.text(3.0).as_deref(), Some("danach"));
    p.assert_alive(9);
}
