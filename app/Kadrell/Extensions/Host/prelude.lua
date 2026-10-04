-- Prelude des ext-host: läuft vor init.lua, legt die globale Tabelle `kadrell` an und verteilt die Nachrichten
-- von Kadrell. Die C-Schicht ruft für jede JSON-Zeile auf stdin `__kadrell_dispatch(zeile)`.

local here = debug.getinfo(1, "S").source:match("^@(.*/)")
local json = dofile(here .. "json.lua")

-- Nur die Prelude schreibt Protokollzeilen, Extensions gehen über die kadrell-Funktionen.
local send = __kadrell_send
__kadrell_send = nil

local function emit(msg)
    send(json.encode(msg))
end

local function log(level, text)
    emit({ t = "log", level = level, text = tostring(text) })
end

local ext = {}
local handlers = {}

kadrell = { json = json, config = {} }

function kadrell.log(text) log("info", text) end

function kadrell.warn(text) log("warn", text) end

function kadrell.on(name, fn)
    handlers[name] = handlers[name] or {}
    table.insert(handlers[name], fn)
end

-- stdout gehört dem Protokoll, print landet im Log.
function print(...)
    local parts = table.pack(...)
    for i = 1, parts.n do parts[i] = tostring(parts[i]) end
    log("info", table.concat(parts, "\t", 1, parts.n))
end

local messages = {}

function messages.hello(m)
    ext.name, ext.dir, ext.storageDir = m.name, m.dir, m.storageDir
    kadrell.config = m.config or {}
    package.path = m.dir .. "/?.lua"
    local ok, err = xpcall(dofile, debug.traceback, m.dir .. "/init.lua")
    if not ok then
        log("error", err)
        os.exit(1)
    end
    emit({ t = "ready" })
end

function messages.ping(m)
    emit({ t = "pong", id = m.id })
end

function messages.event(m)
    for _, fn in ipairs(handlers[m.name] or {}) do
        local ok, err = xpcall(fn, debug.traceback, m.data)
        if not ok then log("error", err) end
    end
end

function messages.shutdown()
    os.exit(0)
end

function __kadrell_dispatch(line)
    local m = json.decode(line)
    local handle = messages[m.t]
    if handle then handle(m) end
end
