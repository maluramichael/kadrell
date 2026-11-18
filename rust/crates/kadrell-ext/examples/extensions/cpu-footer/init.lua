-- Beispiel-Extension: Statusleiste unten mit CPU-Balken (je Kern), Gesamtlast und Speicherpegel.
-- kadrell.system.* gibt es erst in neueren Kadrell-Versionen und nur in Handlern.

if not (kadrell.footer and kadrell.system) then
  kadrell.warn("Diese Kadrell-Version kennt kadrell.footer oder kadrell.system nicht")
  return
end

local function level(fraction)
  if fraction >= 0.85 then return "err" end
  if fraction >= 0.6 then return "warn" end
  return "ok"
end

local function measure()
  local cpu = kadrell.system.cpu()
  local mem = kadrell.system.mem()
  local items = {}
  if cpu then
    items[#items + 1] = {
      id = "cpu", align = "right", label = "CPU", bars = cpu.cores,
      text = string.format("%d%%", math.floor(cpu.total * 100 + 0.5)),
      color = level(cpu.total),
      tip = string.format("CPU-Last gesamt %.0f %%", cpu.total * 100),
      title = "CPU-Last",
    }
  end
  if mem then
    items[#items + 1] = {
      id = "mem", align = "right", label = "RAM", gauge = mem.fraction,
      text = string.format("%d%%", math.floor(mem.fraction * 100 + 0.5)),
      color = level(mem.fraction),
      action = "log-mem",
      tip = string.format("Speicher %.1f von %.1f GB belegt", mem.used / 2^30, mem.total / 2^30),
      title = "Speicherbelegung",
    }
  end
  items[#items + 1] = { id = "hint", align = "left", text = "cpu-footer läuft", color = "muted" }
  kadrell.footer.set(items)
end

kadrell.on("ui.action", function(data)
  if data.id == "log-mem" then
    local mem = kadrell.system.mem()
    if mem then kadrell.log(string.format("Speicher belegt: %.0f %%", mem.fraction * 100)) end
  end
end)

kadrell.on("app.ready", measure)
kadrell.every(tonumber(kadrell.config.alle) or 2, measure)
