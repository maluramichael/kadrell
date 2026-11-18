//! Der Lua-Zustand des Helpers: gleiche Sandbox wie `LuaShim.c`, Prelude und `json.lua` unverändert eingebettet.

use std::sync::Arc;
use std::sync::mpsc::Receiver;

use mlua::{Function, Lua, LuaOptions, MultiValue, StdLib, Value, Variadic};

use super::router::Router;
use crate::sys;

const PRELUDE: &str = include_str!("../../lua/prelude.lua");
const JSON_LUA: &str = include_str!("../../lua/json.lua");
/// Chunkname der Prelude. Sie sucht `json.lua` neben sich (`source:match("^@(.*/)")`); der Name hat deshalb einen
/// Schrägstrich, und `dofile` liefert den Pfad `kadrell/json.lua` aus dem Binary statt von der Platte.
const PRELUDE_NAME: &str = "@kadrell/prelude.lua";
const JSON_PATH: &str = "kadrell/json.lua";

/// Speichergrenze des Lua-Zustands.
pub const MEMORY_LIMIT: usize = 64 << 20;

/// Shell, C-Module und dynamische Bibliotheken gibt es nicht: Außenwelt nur über `kadrell.exec` und `kadrell.http`.
const SANDBOX: &str = r#"
os.execute = nil
io.popen = nil
package.loadlib = nil
package.cpath = ""
package.searchers[4] = nil
package.searchers[3] = nil
io.stdout:setvbuf("no")
"#;

/// Thread-Körper: Zustand aufbauen, Prelude laufen lassen, dann Zeile für Zeile verteilen.
pub fn run(router: Arc<Router>, rx: Receiver<String>) {
    let lua = match setup(&router) {
        Ok(lua) => lua,
        Err(e) => {
            eprintln!("ext-host: {e}");
            sys::kill_exec_children();
            std::process::exit(1);
        }
    };
    let dispatch: Function = match lua.globals().get("__kadrell_dispatch") {
        Ok(f) => f,
        Err(_) => {
            eprintln!("ext-host: prelude.lua definiert __kadrell_dispatch nicht");
            std::process::exit(1);
        }
    };
    for line in rx {
        if let Err(e) = dispatch.call::<()>(line) {
            eprintln!("{e}");
        }
    }
}

fn setup(router: &Arc<Router>) -> mlua::Result<Lua> {
    // SAFETY: `unsafe_new_with` lädt auch `debug`, das die Prelude für Tracebacks braucht; die Sandbox entfernt danach alles,
    // was Prozesse oder C-Module lädt.
    let lua = unsafe { Lua::unsafe_new_with(StdLib::ALL, LuaOptions::default()) };
    lua.set_memory_limit(MEMORY_LIMIT)?;
    lua.load(SANDBOX).set_name("=sandbox").exec()?;
    install_natives(&lua, router)?;
    run_prelude(&lua)?;
    Ok(lua)
}

fn install_natives(lua: &Lua, router: &Arc<Router>) -> mlua::Result<()> {
    let g = lua.globals();
    let router = Arc::clone(router);
    g.set(
        "__kadrell_send",
        lua.create_function(move |_, text: mlua::LuaString| {
            router.route(&String::from_utf8_lossy(&text.as_bytes()));
            Ok(())
        })?,
    )?;
    let os: mlua::Table = g.get("os")?;
    os.set(
        "exit",
        lua.create_function(|_, args: Variadic<Value>| -> mlua::Result<()> { exit(args.first()) })?,
    )?;
    os.set(
        "rename",
        lua.create_function(|lua, (from, to): (String, String)| rename(lua, &from, &to))?,
    )?;
    Ok(())
}

/// `os.exit`: nimmt exec-Kinder mit und beendet den Prozess. Die Prelude behält die Funktion intern, danach ist sie aus
/// `os` entfernt.
fn exit(code: Option<&Value>) -> ! {
    let status = match code {
        Some(Value::Integer(n)) => *n as i32,
        Some(Value::Number(n)) => *n as i32,
        Some(Value::Boolean(false)) => 1,
        _ => 0,
    };
    sys::kill_exec_children();
    std::process::exit(status)
}

/// `os.rename` wie in Lua (`true` oder `nil, Text, Fehlernummer`), aber ersetzt eine vorhandene Zieldatei auch unter
/// Windows, wo das C-`rename` das verweigert. Die Prelude speichert damit `storage.json`.
fn rename(lua: &Lua, from: &str, to: &str) -> mlua::Result<MultiValue> {
    match std::fs::rename(from, to) {
        Ok(()) => Ok(MultiValue::from_iter([Value::Boolean(true)])),
        Err(e) => Ok(MultiValue::from_iter([
            Value::Nil,
            Value::String(lua.create_string(format!("{from}: {e}"))?),
            Value::Integer(i64::from(e.raw_os_error().unwrap_or(0))),
        ])),
    }
}

/// Prelude unverändert ausführen, `dofile` solange für `json.lua` aus dem Binary beantworten.
fn run_prelude(lua: &Lua) -> mlua::Result<()> {
    let g = lua.globals();
    let original: Function = g.get("dofile")?;
    let shim_original = original.clone();
    let shim = lua.create_function(
        move |lua, path: Option<String>| -> mlua::Result<MultiValue> {
            match path.as_deref() {
                Some(JSON_PATH) => lua.load(JSON_LUA).set_name("@kadrell/json.lua").call(()),
                _ => shim_original.call(path),
            }
        },
    )?;
    g.set("dofile", shim)?;
    let result = lua.load(PRELUDE).set_name(PRELUDE_NAME).exec();
    g.set("dofile", original)?;
    result
}
