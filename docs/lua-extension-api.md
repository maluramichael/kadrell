# Lua-Extensions: API und Regeln (für Agenten)

Diese Datei ist die vollständige Referenz zum Schreiben einer Kadrell-Extension. Wer eine Extension schreiben soll,
liest das hier und muss nichts im Code suchen. API-Stand: `apiVersion` 1. Quelle der Wahrheit bleibt
`app/Kadrell/Extensions/Host/prelude.lua`; ändert sich dort etwas, diese Datei nachziehen.

## Ablage und Manifest

Eine Extension ist ein Ordner unter `~/.config/kadrell/extensions/<name>/`:

```
<name>/
  kadrell.json   Manifest (Pflicht)
  init.lua       Einstieg (Pflicht)
  *.lua          weitere Module, per require ladbar (nur Lua, keine C-Module)
```

Regeln für den Ordner:
- Der `name` im Manifest **muss gleich dem Ordnernamen** sein, sonst lädt die Extension nicht.
- Ordner und alle Dateien müssen dir gehören und dürfen für Gruppe/andere **nicht schreibbar** sein (Trust wie sshd),
  sonst lädt sie nicht. Aus dem Marktplatz installierte werden automatisch gehärtet.

`kadrell.json`:

```json
{
  "name": "mein-tool",
  "version": "1.0.0",
  "description": "Kurzbeschreibung für den F4-Dialog.",
  "apiVersion": 1,
  "permissions": ["exec", "http", "sessions"],
  "settings": [
    { "key": "url",     "type": "string", "label": "Basis-URL", "default": "https://…" },
    { "key": "token",   "type": "secret", "label": "API-Token" },
    { "key": "nurMeine","type": "bool",   "label": "Nur meine", "default": true }
  ]
}
```

- Pflicht: `name`, `apiVersion`. Optional: `version`, `description`, `permissions`, `settings`.
- `permissions`: reine **Anzeige**, wird (Stand apiVersion 1) **nicht erzwungen**. Nicht als Schutz verstehen.
- `settings[].type`: `string` | `secret` | `bool`. `secret` liegt im Schlüsselbund, nie in einer Datei; die Extension
  bekommt den Wert in `kadrell.config`, der Nutzer sieht im Dialog nur „gesetzt/nicht gesetzt".
- Werte landen in `kadrell.config.<key>`; fehlt ein Wert, gilt der `default`.

## Lebenszyklus

- Jede Extension läuft als eigener Helfer-Prozess (`Kadrell ext-host`). `init.lua` wird einmal beim Start ausgeführt:
  dort nur `kadrell.on(...)`-Handler, Timer und `panel.set` registrieren, **keine** Host-Aufrufe (siehe unten).
- Ändert der Nutzer eine Einstellung oder die Dateien, lädt Kadrell die Extension neu (Prozess-Neustart).
- Vor dem **ersten** Einschalten bestätigt der Nutzer einmal, dass fremder Code ohne Sandbox mit seinen Rechten läuft.

## Globale Tabelle `kadrell`

### Logging
- `kadrell.log(text)` – Info-Zeile ins Log (sichtbar im F4-Dialog unter „Log").
- `kadrell.warn(text)` – Warnzeile ins Log.
- `print(...)` ist ersetzt und geht ebenfalls ins Log (stdout gehört dem Protokoll, niemals direkt schreiben).

### Events: `kadrell.on(name, fn)`
Mehrere Handler pro Event möglich. `fn` bekommt `data`. Events:

| Event            | `data`                                  |
|------------------|-----------------------------------------|
| `app.ready`      | `{}` – einmal nach `init.lua`; guter Ort für Erststart-I/O |
| `session.new`    | `{ session }`                           |
| `session.focus`  | `{ session }` – auch beim Start         |
| `session.status` | `{ session, state }` – `state` = `working` \| `waiting` \| `idle` |
| `session.remove` | `{ session }`                           |
| `ui.action`      | `{ id }` – aus Panel-Button/-Item oder Statuseintrag |
| `palette.select` | `{ id }` – ein über `kadrell.palette` beigesteuerter ⌘N-Eintrag wurde gewählt |

`session` = `{ key, title, cwd, branch, sessionId, state, group }`.

### Kadrell steuern (nur in Handlern)
- `kadrell.run(...)` → `{ status, stdout, stderr }` – wie das CLI-Tool, z. B. `kadrell.run("select", "-t", key)`.
- `kadrell.sessions()` → Lua-Tabelle wie `kadrell ls --json`.

### Außenwelt (nur in Handlern)
- `kadrell.exec(argv, { cwd = …, timeout = 30 })` → `{ status, stdout, stderr }`. `argv` ist eine Liste von Strings.
- `kadrell.http{ method = "GET", url = …, headers = { … }, body = …, timeout = 30 }` → `{ status, headers, body }`.
- `kadrell.json.encode(v)` / `kadrell.json.decode(s)`.

### Zeit
- `kadrell.after(sekunden, fn)` – einmalig.
- `kadrell.every(sekunden, fn)` → Handle; `handle:cancel()` stoppt die Wiederholung.

### Daten und Konfiguration
- `kadrell.config.<key>` – Werte aus `settings` (read-only).
- `kadrell.locale` – die eingestellte Sprache (`"de"` oder `"en"`), zum Lokalisieren eigener Labels. Steht ab Start
  bereit (auch auf oberster Ebene von `init.lua`).
- `kadrell.storage.get(key)` / `kadrell.storage.set(key, value)` – JSON-Datei im Profil, überlebt Reload und Neustart.
- `kadrell.secret.get(key)` → Wert oder `nil` / `kadrell.secret.set(key, value)` / `kadrell.secret.delete(key)` – Schlüsselbund
  (nur in Handlern). Pro Profil und Extension genamespaced, eine Extension sieht nur ihre eigenen Schlüssel. Für Secrets,
  die zur Laufzeit entstehen (OAuth-Token, mehrere Instanz-Token); das statische `secret`-Setting bleibt für vom Nutzer
  eingegebene Werte.

### Oberfläche
- `kadrell.panel.set(tree)` / `kadrell.panel.clear()` – füllt den Tab der Extension in der rechten Sidebar.
- `kadrell.status.set(item)` / `kadrell.status.clear()` – ein Eintrag in der Statusleiste.
- `kadrell.palette.set(items)` / `kadrell.palette.clear()` – steuert Einträge zum Neue-Session-Dialog (⌘N) bei.
  `items` ist eine Liste von `{ id, title, detail, group }` (`id`, `title` Pflicht). Auswahl kommt als Event
  `palette.select` mit der `id` zurück; die Extension handelt dann selbst (z. B. klonen und `kadrell.run{"new","-c",…}`).
  Wie `panel.set`: ersetzt immer die ganze Liste, nur der letzte Stand pro Dispatch geht raus.

## Panel-Baum

Wurzel ist ein Objekt mit `title` und der Kinderliste unter dem Schlüssel **`children`** (nicht im Array-Teil neben
`title`, json.lua kodiert gemischte Tabellen nicht). `panel.set` ersetzt immer den ganzen Baum (kein Diffing).

Knotentypen (v1):
- `{ type = "section", title = …, collapsed = false, children = { … } }`
- `{ type = "item", text = …, detail = …, color = …, actions = { { id = …, label = … }, … } }`
- `{ type = "text", text = …, color = … }`
- `{ type = "button", label = …, action = "<id>" }`

Statuseintrag: `{ text = …, color = …, action = "<id>" }` (Klick feuert `ui.action` mit dieser `id`).

Farben nur als Theme-Namen: `accent`, `muted`, `ok`, `warn`, `err`. Unbekannte Knoten/Felder werden ignoriert und
geloggt, Texte auf 500 Zeichen gekürzt. Ein zu großer Baum (zu viele Knoten) wird wie ein Absturz behandelt.

## Harte Regeln

1. **Host-Aufrufe nur in Handlern.** `exec`, `http`, `run`, `sessions`, `storage` laufen in Coroutinen und geben ab,
   bis das Ergebnis da ist. Auf oberster Ebene von `init.lua` werfen sie einen Fehler
   (`only allowed inside handlers (kadrell.on, every, after)`). `panel.set`/`status.set`/`log` gehen überall.
2. **stdout gehört dem Protokoll.** Nie direkt schreiben (`io.write`, `io.stdout`), immer `kadrell.log`/`print`.
3. **UI sichtbar machen.** Button-Aktionen sollen etwas Sichtbares tun (Panel/Status aktualisieren), nicht nur loggen.
   Nach einer Aktion typ. `render()` erneut aufrufen, das den Baum neu setzt.
4. **Nur das letzte Panel/Status pro Dispatch** wird gesendet; eine Schleife mit `panel.set` flutet Kadrell nicht.
5. **Fehler sind ok.** Jeder Handler läuft mit `xpcall`; ein Fehler landet mit Traceback im Log, die Extension läuft
   weiter. `init.lua`-Ladefehler beendet den Helfer mit Log-Eintrag.
6. **Keine C-Module, kein `os.execute`/`io.popen`/`loadlib`.** `io.open` u. a. sind da (voller Dateizugriff),
   Speicher ist auf 64 MB gedeckelt. Für Prozesse `kadrell.exec` benutzen.

## Gerüst einer Extension (`init.lua`)

```lua
local ticks = 0

local function render()
  kadrell.panel.set{
    title = kadrell.config.title or "Beispiel",
    children = {
      { type = "section", title = "Status", children = {
        { type = "item", text = "Ticks", detail = tostring(ticks), color = "muted" },
      }},
      { type = "button", label = "Aktualisieren", action = "refresh" },
    },
  }
  kadrell.status.set{ text = "t=" .. ticks, color = "ok", action = "refresh" }
end

kadrell.on("app.ready", function()
  local runs = (kadrell.storage.get("runs") or 0) + 1
  kadrell.storage.set("runs", runs)
  kadrell.log("Start Nr. " .. runs)
  render()
end)

kadrell.on("session.focus", function(data)
  kadrell.log("Fokus: " .. ((data.session and data.session.title) or "?"))
end)

kadrell.on("ui.action", function(data)
  if data.id == "refresh" then
    ticks = ticks + 1
    render()
  end
end)

kadrell.every(5, function() ticks = ticks + 1; render() end)
```

## Marktplatz (Veröffentlichen)

- Öffentliches GitHub-Repo mit dem **Topic `kadrell-extension`**; `kadrell.json` und `init.lua` liegen im Repo-Wurzel.
- Kadrell findet es im F4-Dialog unter „Entdecken" (Suche über die GitHub-Such-API, nach Sternen sortiert) und
  installiert den Tarball des Standard-Branches in den Katalog.
- Der Ordnername kommt aus dem Manifest-`name`; Kadrell merkt sich die Herkunft (`owner/repo`) und lässt ein fremdes
  Repo keinen schon belegten Namen überschreiben.
- Beispiel mit der vollen API: https://github.com/maluramichael/kadrell-example
```
