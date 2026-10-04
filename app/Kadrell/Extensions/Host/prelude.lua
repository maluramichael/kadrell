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

kadrell = { json = json, config = {}, storage = {}, panel = {}, status = {} }

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

-- Jeder Handler (Event, Timer) läuft in einer eigenen Coroutine. Host-Aufrufe geben dort ab und laufen weiter,
-- wenn das Ergebnis als Nachricht ankommt. Fehler landen mit Traceback im Log, die Extension läuft weiter.
local pending = {}
local nextId = 0

local function resume(co, ...)
    local ok, err = coroutine.resume(co, ...)
    if not ok then log("error", err) end
end

local function spawn(fn, arg, done)
    resume(coroutine.create(function()
        local ok, err = xpcall(fn, debug.traceback, arg)
        if not ok then log("error", err) end
        if done then done() end
    end))
end

-- Sendet eine Anfrage an den Host oder an Kadrell und wartet auf das `result` mit derselben id.
local function request(name, msg)
    if not coroutine.isyieldable() then
        error("kadrell." .. name .. ": nur in Handlern (kadrell.on, every, after)", 3)
    end
    nextId = nextId + 1
    msg.id = nextId
    pending[nextId] = coroutine.running()
    emit(msg)
    return coroutine.yield()
end

function kadrell.exec(argv, opts)
    opts = opts or {}
    return request("exec", { t = "exec", argv = argv, cwd = opts.cwd, timeout = opts.timeout })
end

function kadrell.http(req)
    return request("http", { t = "http", method = req.method or "GET", url = req.url, headers = req.headers,
        body = req.body, timeout = req.timeout })
end

function kadrell.run(...)
    return request("run", { t = "run", argv = { ... } })
end

function kadrell.sessions()
    local r = request("sessions", { t = "run", argv = { "ls", "--json" } })
    return json.decode(r.stdout)
end

-- Timer: der Host meldet den Ablauf mit {"t":"timer","id"}, every plant sich nach jedem Lauf neu.
local timers = {}
local Handle = {}
Handle.__index = Handle

function Handle:cancel() self.cancelled = true end

local function schedule(handle)
    nextId = nextId + 1
    timers[nextId] = handle
    emit({ t = "timer", id = nextId, after = handle.seconds })
end

local function timer(seconds, fn, repeating)
    local handle = setmetatable({ seconds = seconds, fn = fn, repeating = repeating }, Handle)
    schedule(handle)
    return handle
end

function kadrell.after(seconds, fn) return timer(seconds, fn, false) end

function kadrell.every(seconds, fn) return timer(seconds, fn, true) end

-- Persistenz: eine JSON-Datei im storageDir, über kadrell.json.
local function storagePath() return ext.storageDir .. "/storage.json" end

local function readStorage()
    local f = io.open(storagePath(), "r")
    if not f then return {} end
    local text = f:read("a")
    f:close()
    local ok, data = pcall(json.decode, text)
    return ok and type(data) == "table" and data or {}
end

function kadrell.storage.get(key) return readStorage()[key] end

function kadrell.storage.set(key, value)
    local data = readStorage()
    data[key] = value
    local tmp = storagePath() .. ".tmp"
    local f = assert(io.open(tmp, "w"))
    f:write(json.encode(data))
    f:close()
    assert(os.rename(tmp, storagePath()))
end

-- clear sendet ein echtes null, json.encode lässt nil-Felder weg.
function kadrell.panel.set(tree) emit({ t = "panel", tree = tree }) end

function kadrell.panel.clear() send('{"t":"panel","tree":null}') end

function kadrell.status.set(item) emit({ t = "status", item = item }) end

function kadrell.status.clear() send('{"t":"status","item":null}') end

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
    for _, fn in ipairs(handlers[m.name] or {}) do spawn(fn, m.data) end
end

function messages.result(m)
    local co = pending[m.id]
    pending[m.id] = nil
    m.t, m.id = nil, nil
    if co then resume(co, m) end
end

function messages.timer(m)
    local handle = timers[m.id]
    timers[m.id] = nil
    if not handle or handle.cancelled then return end
    spawn(handle.fn, nil, function()
        if handle.repeating and not handle.cancelled then schedule(handle) end
    end)
end

function messages.shutdown()
    os.exit(0)
end

function __kadrell_dispatch(line)
    local m = json.decode(line)
    local handle = messages[m.t]
    if handle then handle(m) end
end
