-- SSH: listet die Hosts aus ~/.ssh/config im linken Dock, ein Klick öffnet eine Remote-Kachel
-- (ssh + tmux, wie ⌘⇧N). Hostliste und zuletzt verbundene kommen von `kadrell ssh-hosts --json`,
-- verbunden wird über `kadrell new --host <h>`.

local de = (kadrell.locale or "en"):sub(1, 2) == "de"
local function t(d, e) return de and d or e end

local hosts, recent = {}, {}

local function fetch()
  local r = kadrell.run("ssh-hosts", "--json")
  if r.status ~= 0 then
    kadrell.warn("ssh-hosts: " .. (r.stderr or ""))
    return
  end
  local data = kadrell.json.decode(r.stdout) or {}
  hosts, recent = data.hosts or {}, data.recent or {}
end

local function hostItem(h)
  return { type = "item", text = h, actions = { { id = "connect:" .. h, label = t("Verbinden", "Connect") } } }
end

local function section(title, items)
  if #items == 0 then return nil end
  return { type = "section", title = title, children = items }
end

local function render()
  local children = {}

  local recentItems = {}
  for _, h in ipairs(recent) do recentItems[#recentItems + 1] = hostItem(h) end
  children[#children + 1] = section(t("Zuletzt", "Recent"), recentItems)

  -- Im vollständigen Abschnitt die schon oben gezeigten weglassen.
  local inRecent = {}
  for _, h in ipairs(recent) do inRecent[h] = true end
  local hostItems = {}
  for _, h in ipairs(hosts) do
    if not inRecent[h] then hostItems[#hostItems + 1] = hostItem(h) end
  end
  if #hostItems == 0 and #recentItems == 0 then
    hostItems[1] = { type = "text", text = t("Keine Hosts in ~/.ssh/config", "No hosts in ~/.ssh/config"), color = "muted" }
  end
  children[#children + 1] = section(t("Hosts", "Hosts"), hostItems)

  children[#children + 1] = { type = "button", label = t("Aktualisieren", "Refresh"), action = "refresh" }

  -- children kann nil-Lücken haben (leere Abschnitte), json.lua kodiert das falsch: verdichten.
  local clean = {}
  for _, c in ipairs(children) do clean[#clean + 1] = c end

  kadrell.panel.set({ title = "SSH", children = clean }, { dock = "left" })
end

kadrell.on("app.ready", function()
  fetch()
  render()
end)

kadrell.on("ui.action", function(data)
  local id = data.id or ""
  if id == "refresh" then
    fetch()
    render()
  elseif id:sub(1, 8) == "connect:" then
    local host = id:sub(9)
    local r = kadrell.run("new", "--host", host)
    if r.status ~= 0 then kadrell.warn(t("Verbinden fehlgeschlagen: ", "Connect failed: ") .. (r.stderr or "")) end
    fetch() -- zuletzt verbundene haben sich geändert
    render()
  end
end)
