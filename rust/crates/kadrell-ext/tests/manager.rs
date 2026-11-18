//! `ExtensionManager` mit echtem `ext-host`, eigenem Temp-Katalog und gefälschtem Steuerweg (Port von
//! `ExtensionManagerTests.swift`).

mod common;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use common::*;
use kadrell_ext::bridge::{self, ControlResponse, MemoryStore, SecretStore};
use kadrell_ext::i18n::tr_args;
use kadrell_ext::manager::{ExtensionManager, ManagerConfig};
use kadrell_ext::panel::{PanelNode, ThemeColor};
use kadrell_ext::process::ProcessConfig;
use kadrell_ext::settings::{self, KEYCHAIN_SERVICE};
use kadrell_ext::supervisor::ExtensionState;
use serde_json::{Value, json};

struct Fx {
    _root: TempDir,
    catalog: PathBuf,
    manager: ExtensionManager,
    store: Arc<MemoryStore>,
    calls: Arc<Mutex<Vec<Vec<String>>>>,
}

impl Fx {
    /// Ein Katalog `root/catalog` mit der Extension `name`, Manager darauf gestartet.
    fn with(name: &str, init_lua: &str, manifest: Option<Value>) -> Fx {
        let root = TempDir::new("mgr");
        let catalog = root.path().join("catalog");
        std::fs::create_dir_all(&catalog).unwrap();
        make_extension(&catalog, name, init_lua, manifest);
        let fx = Fx::build(root, catalog, |_| ControlResponse::ok("fake"));
        fx.manager.start();
        fx
    }

    fn build(
        root: TempDir,
        catalog: PathBuf,
        answer: impl Fn(&[String]) -> ControlResponse + Send + Sync + 'static,
    ) -> Fx {
        let store = Arc::new(MemoryStore::new());
        let calls: Arc<Mutex<Vec<Vec<String>>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&calls);
        let control = move |argv: &[String]| {
            sink.lock().unwrap().push(argv.to_vec());
            answer(argv)
        };
        let mut process = ProcessConfig::new(HOST_EXE);
        process.extra_environment = helper_env();
        let mut config = ManagerConfig::new(&catalog, process, root.path().join("storage"));
        config.reload_delay = Duration::from_millis(100);
        let manager = ExtensionManager::new(
            config,
            Arc::new(control),
            store.clone(),
            store.clone(),
            Arc::new(Vec::new),
        );
        Fx {
            _root: root,
            catalog,
            manager,
            store,
            calls,
        }
    }

    fn log_contains(&self, name: &str, text: &str) -> bool {
        self.manager.log(name).iter().any(|l| l.contains(text))
    }

    fn helpers(&self) -> usize {
        helpers_in(&self.catalog)
    }

    fn panel_title(&self) -> Option<String> {
        self.manager.panels().first().map(|(_, t)| t.title.clone())
    }

    fn running(&self, name: &str) -> bool {
        self.manager.state(name) == ExtensionState::Running
    }
}

impl Drop for Fx {
    fn drop(&mut self) {
        self.manager.shutdown_all();
        let gone = wait_until(5.0, || self.helpers() == 0);
        if !gone {
            kill_helpers_in(&self.catalog);
        }
        if !std::thread::panicking() {
            assert!(gone, "kein Helper bleibt nach shutdown_all übrig");
        }
    }
}

const PANEL_ON_READY: &str =
    r#"kadrell.on("app.ready", function() kadrell.panel.set({ title = "A", children = {} }) end)"#;

fn app_ready() -> Value {
    json!({})
}

#[test]
fn enable_starts_and_panel_arrives() {
    let fx = Fx::with("a", PANEL_ON_READY, None);
    fx.manager.set_enabled("a", true);
    fx.manager.emit("app.ready", app_ready());
    assert!(
        wait_until(5.0, || fx.panel_title().as_deref() == Some("A")),
        "{:?}",
        fx.manager.log("a")
    );
    assert!(wait_until(3.0, || fx.running("a")));
    assert!(bridge::enabled_extensions(&*fx.store).contains(&"a".to_string()));
}

#[test]
fn disable_clears_panel_immediately() {
    let fx = Fx::with("a", PANEL_ON_READY, None);
    fx.manager.set_enabled("a", true);
    fx.manager.emit("app.ready", app_ready());
    assert!(wait_until(5.0, || !fx.manager.panels().is_empty()));
    fx.manager.set_enabled("a", false);
    assert!(fx.manager.panels().is_empty());
    assert_eq!(fx.manager.state("a"), ExtensionState::Off);
    assert!(!bridge::enabled_extensions(&*fx.store).contains(&"a".to_string()));
}

#[test]
fn run_goes_through_control() {
    let fx = Fx::with(
        "a",
        r#"kadrell.on("app.ready", function()
            local r = kadrell.run("ls")
            kadrell.log("status " .. r.status .. " " .. r.stdout)
        end)"#,
        None,
    );
    fx.manager.set_enabled("a", true);
    fx.manager.emit("app.ready", app_ready());
    assert!(
        wait_until(5.0, || fx.log_contains("a", "status 0 fake")),
        "{:?}",
        fx.manager.log("a")
    );
    assert_eq!(*fx.calls.lock().unwrap(), vec![vec!["ls".to_string()]]);
}

#[test]
fn file_change_reloads() {
    let fx = Fx::with("a", "kadrell.log('v1')", None);
    fx.manager.set_enabled("a", true);
    assert!(
        wait_until(5.0, || fx.running("a") && fx.log_contains("a", "v1")),
        "{:?}",
        fx.manager.log("a")
    );
    // In die bestehende Datei schreiben, wie es manche Editoren tun.
    std::fs::write(fx.catalog.join("a/init.lua"), "kadrell.log('v2')").unwrap();
    assert!(
        wait_until(5.0, || fx.running("a") && fx.log_contains("a", "v2")),
        "{:?}",
        fx.manager.log("a")
    );
}

/// Ordner einer laufenden Extension verschwindet.
#[test]
fn deleted_folder_stops_cleanly() {
    let fx = Fx::with("a", "kadrell.log('hi')", None);
    fx.manager.set_enabled("a", true);
    assert!(wait_until(5.0, || fx.running("a")));
    assert_eq!(fx.helpers(), 1);
    std::fs::remove_dir_all(fx.catalog.join("a")).unwrap();
    let gone = wait_until(5.0, || fx.manager.found().is_empty() && fx.helpers() == 0);
    assert!(
        gone,
        "found: {:?}",
        fx.manager
            .found()
            .iter()
            .map(|f| f.name.clone())
            .collect::<Vec<_>>()
    );
    assert_eq!(fx.manager.state("a"), ExtensionState::Off);
}

/// Umbenannt: alter Name verschwindet samt Prozess, der neue erscheint aus (Name passt nicht mehr zum Manifest).
#[test]
fn renamed_folder_stops_and_appears_off() {
    let fx = Fx::with("a", "kadrell.log('hi')", None);
    fx.manager.set_enabled("a", true);
    assert!(wait_until(5.0, || fx.running("a")));
    std::fs::rename(fx.catalog.join("a"), fx.catalog.join("b")).unwrap();
    let moved = wait_until(5.0, || {
        fx.manager
            .found()
            .iter()
            .map(|f| f.name.clone())
            .collect::<Vec<_>>()
            == ["b"]
            && fx.helpers() == 0
    });
    assert!(
        moved,
        "found: {:?}",
        fx.manager
            .found()
            .iter()
            .map(|f| f.name.clone())
            .collect::<Vec<_>>()
    );
    assert_eq!(fx.manager.state("b"), ExtensionState::Off);
    assert!(fx.manager.found()[0].problem.is_some());
}

#[test]
fn secret_setting_is_passed_in_config() {
    let name = "kadrell-test-secret";
    let manifest = json!({"name": name, "apiVersion": 1, "settings": [
        {"key": "token", "type": "secret", "label": "Token"},
        {"key": "mine", "type": "bool", "label": "Mine", "default": true},
    ]});
    let fx = Fx::with(
        name,
        "kadrell.log('token=' .. tostring(kadrell.config.token) .. ' mine=' .. tostring(kadrell.config.mine))",
        Some(manifest),
    );
    let ext = fx.manager.found().into_iter().next().unwrap();
    fx.manager.settings().set(&json!("abc"), &ext, "token");
    fx.manager.set_enabled(name, true);
    assert!(
        wait_until(8.0, || fx.log_contains(name, "token=abc mine=true")),
        "{:?}",
        fx.manager.log(name)
    );
    fx.manager.settings().set(&Value::Null, &ext, "token");
    let account = settings::account(None, name, "token");
    assert!(
        SecretStore::read(&*fx.store, KEYCHAIN_SERVICE, &account).is_none(),
        "null löscht den Eintrag im Schlüsselbund"
    );
}

/// Der Extensions-Ordner entsteht erst nach dem Start: wird trotzdem gesehen, auch Änderungen darin.
#[test]
fn catalog_created_later() {
    let root = TempDir::new("mgr");
    let prepared = TempDir::new("prep");
    make_extension(prepared.path(), "a", "kadrell.log('v1')", None);
    let catalog = root.path().join("catalog");
    let fx = Fx::build(root, catalog.clone(), |_| ControlResponse::ok("fake"));
    fx.manager.start();
    assert!(fx.manager.found().is_empty());
    std::fs::rename(prepared.path(), &catalog).unwrap();
    assert!(wait_until(5.0, || fx
        .manager
        .found()
        .iter()
        .map(|f| f.name.clone())
        .collect::<Vec<_>>()
        == ["a"]));
    fx.manager.set_enabled("a", true);
    assert!(wait_until(5.0, || fx.running("a")));
    std::fs::write(catalog.join("a/init.lua"), "kadrell.log('v2')").unwrap();
    assert!(
        wait_until(5.0, || fx.log_contains("a", "v2")),
        "{:?}",
        fx.manager.log("a")
    );
}

/// Panel über 2000 Knoten: wie ein Absturz, Grund nennt die Grenze, Panel bleibt leer. Der Prozess stirbt sofort, auch
/// wenn er danach hängt.
#[test]
fn too_large_panel_counts_as_crash() {
    let fx = Fx::with(
        "a",
        r#"kadrell.on("app.ready", function()
            local children = {}
            for i = 1, 2001 do children[i] = { type = "text", text = "x" } end
            kadrell.panel.set({ title = "A", children = children })
            kadrell.after(0, function() while true do end end)
        end)"#,
        None,
    );
    fx.manager.set_enabled("a", true);
    fx.manager.emit("app.ready", app_ready());
    assert!(
        wait_until(5.0, || matches!(
            fx.manager.state("a"),
            ExtensionState::Failed(_)
        )),
        "{:?}",
        fx.manager.state("a")
    );
    assert!(
        wait_until(1.0, || fx.helpers() == 0),
        "Helper lebt nach dem zu großen Panel weiter"
    );
    let ExtensionState::Failed(reason) = fx.manager.state("a") else {
        panic!()
    };
    assert_eq!(
        reason,
        tr_args(
            "Panel zu groß ({0} Knoten, höchstens {1})",
            "Panel too large ({0} nodes, at most {1})",
            &["2001", "2000"]
        )
    );
    assert!(fx.manager.panels().is_empty());
}

/// Abschalten während des Starts, sofort wieder an. Das späte ready des alten Prozesses darf den neuen Start nicht
/// verhindern: am Ende läuft genau ein neuer Prozess, und das Panel kommt von ihm.
#[test]
fn disable_during_start_then_enable() {
    let fx = Fx::with(
        "a",
        r#"local n = (kadrell.storage.get("n") or 0) + 1
        kadrell.storage.set("n", n)
        local t = os.clock()
        while os.clock() - t < 1.5 do end
        kadrell.on("app.ready", function() kadrell.panel.set({ title = "run " .. n, children = {} }) end)"#,
        None,
    );
    fx.manager.emit("app.ready", app_ready());
    fx.manager.set_enabled("a", true);
    assert!(wait_until(5.0, || fx.helpers() == 1));
    assert_eq!(fx.manager.state("a"), ExtensionState::Starting);
    fx.manager.set_enabled("a", false);
    std::thread::sleep(Duration::from_millis(2000));
    fx.manager.set_enabled("a", true);
    let ok = wait_until(8.0, || {
        fx.panel_title().as_deref() == Some("run 2") && fx.helpers() == 1
    });
    assert!(
        ok,
        "state {:?}, panels {:?}, helpers {}",
        fx.manager.state("a"),
        fx.manager.panels(),
        fx.helpers()
    );
    assert_eq!(fx.manager.state("a"), ExtensionState::Running);
}

/// Ein kadrell.run, das nach dem Abschalten noch ankommt, wird nicht mehr ausgeführt.
#[test]
fn run_after_disable_is_not_executed() {
    let fx = Fx::with(
        "a",
        r#"kadrell.on("app.ready", function()
            local t = os.clock()
            while os.clock() - t < 0.4 do end
            kadrell.run("late")
        end)"#,
        None,
    );
    fx.manager.set_enabled("a", true);
    fx.manager.emit("app.ready", app_ready());
    assert!(wait_until(5.0, || fx.running("a")));
    fx.manager.set_enabled("a", false);
    assert!(wait_until(5.0, || fx.helpers() == 0));
    std::thread::sleep(Duration::from_millis(200));
    assert!(
        !fx.calls.lock().unwrap().contains(&vec!["late".to_string()]),
        "{:?}",
        fx.calls.lock().unwrap()
    );
}

/// session.focus auch beim Start: eine später eingeschaltete Extension erfährt die fokussierte Session.
#[test]
fn late_starter_gets_current_focus() {
    let fx = Fx::with(
        "a",
        r#"kadrell.on("session.focus", function(d) kadrell.log("focus " .. d.session.key) end)"#,
        None,
    );
    fx.manager.emit("app.ready", app_ready());
    fx.manager
        .emit("session.focus", json!({"session": {"key": "s1"}}));
    fx.manager.set_enabled("a", true);
    assert!(
        wait_until(5.0, || fx.log_contains("a", "focus s1")),
        "{:?}",
        fx.manager.log("a")
    );
}

/// Der Katalog verschwindet und entsteht zweimal hintereinander neu, der Beobachter sieht jedes Mal den neuen.
#[test]
fn catalog_deleted_and_recreated_twice() {
    let fx = Fx::with("a", "kadrell.log('v1')", None);
    for round in 1..=2 {
        std::fs::remove_dir_all(&fx.catalog).unwrap();
        assert!(
            wait_until(5.0, || fx.manager.found().is_empty()),
            "Runde {round}: Löschen nicht gesehen"
        );
        std::fs::create_dir_all(&fx.catalog).unwrap();
        make_extension(
            &fx.catalog,
            "a",
            &format!("kadrell.log('v{}')", round + 1),
            None,
        );
        assert!(
            wait_until(5.0, || fx
                .manager
                .found()
                .iter()
                .map(|f| f.name.clone())
                .collect::<Vec<_>>()
                == ["a"]),
            "Runde {round}: Neuanlage nicht gesehen"
        );
    }
    fx.manager.set_enabled("a", true);
    assert!(wait_until(5.0, || fx.running("a")));
    std::fs::write(fx.catalog.join("a/init.lua"), "kadrell.log('changed')").unwrap();
    assert!(
        wait_until(5.0, || fx.log_contains("a", "changed")),
        "{:?}",
        fx.manager.log("a")
    );
}

/// Das Quick-start-Beispiel von https://kadrell.malura.de/extensions, Byte für Byte wie auf der Seite.
#[test]
fn docs_quick_start_shows_panel_and_status() {
    let manifest = json!({
        "name": "hello", "version": "0.1.0",
        "description": "Counts sessions in the right sidebar and the status bar", "apiVersion": 1
    });
    let init_lua = r#"local function count()
  local n = 0
  for _, group in ipairs(kadrell.sessions().groups) do n = n + #group.sessions end
  return n
end

local function render()
  local text = count() .. " sessions"
  kadrell.panel.set{
    title = "Hello",
    children = {
      { type = "text", text = text, color = "muted" },
      { type = "button", label = "Refresh", action = "refresh" },
    },
  }
  kadrell.status.set{ text = text, color = "ok", action = "refresh" }
end

kadrell.on("app.ready", render)
kadrell.on("session.new", render)
kadrell.on("session.remove", render)
kadrell.on("ui.action", render)
"#;
    let root = TempDir::new("mgr");
    let catalog = root.path().join("catalog");
    std::fs::create_dir_all(&catalog).unwrap();
    make_extension(&catalog, "hello", init_lua, Some(manifest));
    let sessions = Arc::new(Mutex::new(2usize));
    let counter = Arc::clone(&sessions);
    let fx = Fx::build(root, catalog, move |_| {
        // Form von `kadrell ls --json`: Gruppen mit Sessions.
        let n = *counter.lock().unwrap();
        ControlResponse::ok(&format!(
            r#"{{"layout":"grid","groups":[{{"sessions":[{}]}}]}}"#,
            vec!["{}"; n].join(",")
        ))
    });
    fx.manager.start();
    assert!(fx.manager.found()[0].problem.is_none());
    fx.manager.set_enabled("hello", true);
    fx.manager.emit("app.ready", app_ready());
    assert!(
        wait_until(8.0, || fx
            .manager
            .status_items()
            .first()
            .map(|(_, i)| i.text.clone())
            .as_deref()
            == Some("2 sessions")),
        "{:?}",
        fx.manager.log("hello")
    );
    let panels = fx.manager.panels();
    assert_eq!(panels[0].1.title, "Hello");
    assert_eq!(
        panels[0].1.nodes,
        vec![
            PanelNode::Text("2 sessions".into(), Some(ThemeColor::Muted)),
            PanelNode::Button {
                label: "Refresh".into(),
                action: "refresh".into()
            }
        ]
    );
    assert_eq!(fx.manager.status_items()[0].1.color, Some(ThemeColor::Ok));
    *sessions.lock().unwrap() = 3;
    fx.manager.action("hello", "refresh");
    assert!(
        wait_until(8.0, || fx
            .manager
            .status_items()
            .first()
            .map(|(_, i)| i.text.clone())
            .as_deref()
            == Some("3 sessions")),
        "{:?}",
        fx.manager.log("hello")
    );
}

#[test]
fn footer_items_arrive_and_clear_on_disable() {
    let fx = Fx::with(
        "a",
        r#"kadrell.on("app.ready", function()
            kadrell.footer.set({ { id = "cpu", align = "right", label = "CPU", bars = { 0.2, 0.8 }, action = "open" },
                                 { text = "ohne id" } })
        end)"#,
        None,
    );
    fx.manager.set_enabled("a", true);
    fx.manager.emit("app.ready", app_ready());
    assert!(
        wait_until(5.0, || !fx.manager.footer_items().is_empty()),
        "{:?}",
        fx.manager.log("a")
    );
    let items = fx.manager.footer_items();
    assert_eq!(
        items.iter().map(|(_, i)| i.id.as_str()).collect::<Vec<_>>(),
        ["cpu"]
    );
    assert_eq!(items[0].1.bars, vec![0.2, 0.8]);
    let warning = kadrell_ext::i18n::tr(
        "Statusleiste: Eintrag ohne „id“ übersprungen",
        "Status bar: skipped entry without “id”",
    );
    assert!(
        fx.log_contains("a", &warning),
        "verworfene Einträge stehen im Log: {:?}",
        fx.manager.log("a")
    );
    fx.manager.set_enabled("a", false);
    assert!(fx.manager.footer_items().is_empty());
}

#[test]
fn locale_change_reaches_running_extension() {
    let fx = Fx::with(
        "a",
        r#"kadrell.on("app.locale", function(d) kadrell.log("sprache " .. kadrell.locale) end)"#,
        None,
    );
    fx.manager.set_enabled("a", true);
    assert!(wait_until(5.0, || fx.running("a")));
    let other = if fx.manager.locale() == "de" {
        "en"
    } else {
        "de"
    };
    fx.manager.set_locale(other);
    fx.manager.set_locale(other);
    assert!(
        wait_until(5.0, || fx.log_contains("a", &format!("sprache {other}"))),
        "{:?}",
        fx.manager.log("a")
    );
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(
        fx.manager
            .log("a")
            .iter()
            .filter(|l| l.contains("sprache"))
            .count(),
        1,
        "gleiche Sprache zweimal: ein Event"
    );
}

#[test]
fn palette_entries_arrive_and_select_goes_back() {
    let fx = Fx::with(
        "a",
        r#"kadrell.on("app.ready", function() kadrell.palette.set{ { id = "gh:a/web", title = "a/web", group = "GitHub" } } end)
        kadrell.on("palette.select", function(d) kadrell.log("sel " .. d.id) end)"#,
        None,
    );
    fx.manager.set_enabled("a", true);
    fx.manager.emit("app.ready", app_ready());
    assert!(wait_until(5.0, || !fx.manager.palette_entries().is_empty()));
    assert_eq!(fx.manager.palette_entries()[0].1.id, "gh:a/web");
    fx.manager.palette_select("a", "gh:a/web");
    assert!(wait_until(5.0, || fx.log_contains("a", "sel gh:a/web")));
}

#[test]
fn secret_calls_use_the_store_with_namespaced_accounts() {
    let fx = Fx::with(
        "a",
        r#"kadrell.on("app.ready", function()
            kadrell.secret.set("tok", "v1")
            kadrell.log("got " .. tostring(kadrell.secret.get("tok")))
            kadrell.secret.delete("tok")
            kadrell.log("after " .. tostring(kadrell.secret.get("tok")))
        end)"#,
        None,
    );
    fx.manager.set_enabled("a", true);
    fx.manager.emit("app.ready", app_ready());
    assert!(
        wait_until(5.0, || fx.log_contains("a", "after nil")),
        "{:?}",
        fx.manager.log("a")
    );
    assert!(fx.log_contains("a", "got v1"));
}

#[test]
fn on_change_fires_without_holding_the_lock() {
    let fx = Fx::with("a", PANEL_ON_READY, None);
    let seen = Arc::new(Mutex::new(0usize));
    let counter = Arc::clone(&seen);
    let manager = fx.manager.clone();
    fx.manager.set_on_change(Arc::new(move || {
        // Der Rückruf darf den Manager befragen (kein Deadlock).
        let _ = manager.panels();
        *counter.lock().unwrap() += 1;
    }));
    fx.manager.set_enabled("a", true);
    fx.manager.emit("app.ready", app_ready());
    assert!(wait_until(5.0, || fx.panel_title().as_deref() == Some("A")));
    assert!(*seen.lock().unwrap() > 0);
}

/// Nicht nur Lua: der Pfad der Fixtures hat keine Leerzeichen, aber der Katalog darf welche haben.
#[test]
fn catalog_path_with_spaces_works() {
    let root = TempDir::new("mgr leer");
    let catalog: PathBuf = root.path().join("my catalog");
    std::fs::create_dir_all(&catalog).unwrap();
    make_extension(&catalog, "a", "kadrell.log('hi')", None);
    let fx = Fx::build(root, catalog, |_| ControlResponse::ok(""));
    fx.manager.start();
    fx.manager.set_enabled("a", true);
    assert!(
        wait_until(5.0, || fx.log_contains("a", "hi")),
        "{:?}",
        fx.manager.log("a")
    );
}
