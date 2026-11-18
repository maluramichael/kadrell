-- Beispiel-Extension: Panel mit Abschnitten, Einträgen, Text und Knöpfen, dazu ein Statuseintrag oben.
-- Struktur laut docs/lua-extension-api.md: Wurzel mit title und children, Knoten section, item, text, button.

local ticks = 0
local sessions = {}

local function count(state)
  local n = 0
  for _, s in ipairs(sessions) do
    if s.state == state then n = n + 1 end
  end
  return n
end

local function session_items()
  local out = {}
  for _, s in ipairs(sessions) do
    local color = "muted"
    if s.state == "working" then color = "ok" elseif s.state == "waiting" then color = "warn" end
    out[#out + 1] = {
      type = "item", text = s.title or s.key, detail = s.state, color = color,
      actions = { { id = "select:" .. s.key, label = "Anzeigen" } },
    }
  end
  if #out == 0 then
    out[1] = { type = "text", text = "Keine Sessions." }
  end
  return out
end

local function render()
  kadrell.panel.set{
    title = kadrell.config.titel or "Demo",
    children = {
      { type = "section", title = "Übersicht", children = {
        { type = "item", text = "Läuft", detail = tostring(count("working")), color = "ok" },
        { type = "item", text = "Wartet", detail = tostring(count("waiting")), color = "warn" },
        { type = "item", text = "Ruhig", detail = tostring(count("idle")), color = "muted" },
        { type = "item", text = "Zähler", detail = tostring(ticks), color = "accent",
          actions = { { id = "tick", label = "Erhöhen" }, { id = "reset", label = "Zurücksetzen" } } },
      }},
      { type = "section", title = "Sessions", children = session_items() },
      { type = "section", title = "Hinweise", collapsed = true, children = {
        { type = "text", text = "Dieser Abschnitt kommt zugeklappt an." },
        { type = "text", text = "Fehlerfarbe", color = "err" },
      }},
      { type = "button", label = "Zähler erhöhen", action = "tick" },
      { type = "button", label = "Neu laden", action = "refresh" },
    },
  }
  kadrell.status.set{ text = "demo " .. ticks, color = "ok", action = "tick" }
end

local function refresh()
  local list = kadrell.sessions()
  if type(list) == "table" then sessions = list end
  render()
end

kadrell.on("app.ready", function()
  kadrell.log("panel-demo gestartet")
  refresh()
end)

kadrell.on("session.status", function() refresh() end)
kadrell.on("session.new", function() refresh() end)
kadrell.on("session.remove", function() refresh() end)

kadrell.on("ui.action", function(data)
  local id = data.id or ""
  if id == "tick" then
    ticks = ticks + 1
  elseif id == "reset" then
    ticks = 0
  elseif id == "refresh" then
    refresh()
    return
  elseif id:sub(1, 7) == "select:" then
    kadrell.run("select", "-t", id:sub(8))
  end
  render()
end)

kadrell.every(tonumber(kadrell.config.alle) or 5, function()
  ticks = ticks + 1
  render()
end)

render()
