# Lua-Extensions für Kadrell: Design

Stand 2026-10-04. Teil 1 von mehreren (siehe Zerlegung).

## Ziel

Kadrell lässt sich mit Lua-Extensions erweitern: Extensions reagieren auf Events, steuern Kadrell über dieselben
Befehle wie die CLI, befüllen eine rechte Sidebar und die Statusleiste, werden beim Speichern live neu geladen und
lassen sich in einem Dialog an- und abschalten. Eine kaputte, hängende oder flutende Extension bringt Kadrell nie zum
Absturz und blockiert nie das Fenster.

Endgame (eigene Specs danach): Jira in der rechten Sidebar, ddev-Steuerung, ein Marketplace über GitHub.

## Zerlegung

1. **Dieses Dokument:** Runtime, Lebenszyklus, Lua-API, UI-Slots (rechte Sidebar, Statusleiste), Extensions-Dialog,
   Doku-Seite. Genau so viel, dass Jira und ddev darauf baubar sind.
2. Jira-Extension (eigene Spec).
3. ddev-Extension (eigene Spec).
4. Marketplace: GitHub-Repos mit Topic `kadrell-extension` suchen, installieren, aktualisieren (eigene Spec).
5. Berechtigungen erzwingen (eigene Spec, sobald fremde Extensions verbreitet werden).

## Nicht-Ziele in Teil 1

- Kein Rechte-Modell, das etwas verbietet. Das Manifest-Feld `permissions` existiert, wird gelesen und angezeigt,
  aber nicht erzwungen.
- Keine Palette-Einträge, keine Kürzel, keine Toasts, keine Badges an Baum oder Kachel aus Extensions.
- Kein Installieren aus dem Netz, kein Marketplace.
- Extensions fassen nie AppKit-Views an. Sie liefern Daten, Kadrell zeichnet.
- Kein Daemon: Extension-Prozesse sind Kindprozesse von Kadrell und enden mit ihm.

## Entscheidungen

| Frage | Entscheidung | Grund |
|---|---|---|
| Wer schreibt Extensions | Erst Michael, später andere | Rechte-Feld jetzt vorsehen, später erzwingen |
| Wo läuft Lua | Ein Prozess pro Extension | Absturz, Hänger und Speicherfresser treffen nur diesen Prozess, hart killbar |
| Lua-Version | 5.5.1 (aktuell, 3. Aug 2026, lua.org), als C-Quellen im Repo | MIT, keine Fremdabhängigkeit, 5.4 bekommt keine Releases mehr |
| UI-Slots v1 | Rechte Sidebar, Statusleiste | Michaels Auswahl |
| Steuer-API | Vorhandene `ControlCommand`-Befehle | Alles, was `kadrell <befehl>` kann, geht ohne Zusatzcode auch aus Lua |
| Doku | kadrell.malura.de/extensions, nur Englisch | Entwickler-Doku für ein internationales Publikum |

## 1. Aufbau und Lebenszyklus

### Ablage

```
~/.config/kadrell/extensions/<name>/
  kadrell.json   Manifest
  init.lua       Einstieg
  *.lua          weitere Module, per require ladbar (nur Lua, keine C-Module)
```

Manifest:

```json
{
  "name": "jira",
  "version": "0.1.0",
  "description": "Jira tickets in the right sidebar",
  "apiVersion": 1,
  "permissions": ["exec", "http:*.atlassian.net", "control"],
  "settings": [
    {"key": "jiraUrl",  "type": "string", "label": "Jira URL"},
    {"key": "token",    "type": "secret", "label": "API token"},
    {"key": "onlyMine", "type": "bool",   "label": "Only my tickets", "default": true}
  ]
}
```

- `name` muss dem Ordnernamen entsprechen, sonst lädt die Extension nicht (Fehler im Dialog).
- `apiVersion` größer als die von Kadrell unterstützte: lädt nicht, Meldung „braucht neueres Kadrell“.
- Ordner und Dateien durchlaufen dieselbe Prüfung wie Shell-Hooks (`Hooks.trusted`): fremder Besitzer oder für
  Gruppe/andere beschreibbar heißt, die Extension lädt nicht.
- Welche Extensions an sind, steht in den Einstellungen des Profils. `--profile tmp` übernimmt sie wie die übrigen
  Einstellungen, damit lässt sich ohne das Hauptprofil testen.
- `settings`-Werte liegen in den Einstellungen des Profils, Typ `secret` im Schlüsselbund (ein Eintrag pro Profil,
  Extension und Schlüssel), nie in einer Datei.

### Prozess

- Kadrell startet pro eingeschalteter Extension `Kadrell ext-host <ordner>` (gleiches Binary, Einstieg in
  `main.swift` neben dem CLI-Weg).
- Umgebung des Helpers: dieselbe Login-Shell-Umgebung, die Claude bekommt (`ClaudeCLI.environment`), damit `ddev`,
  `git` usw. im `PATH` liegen.
- Kommunikation: stdin/stdout, eine JSON-Zeile pro Nachricht (Protokoll siehe unten). stderr geht ins Log der
  Extension.
- Im Helper läuft eine Lua-Instanz auf einem Thread mit Event-Schleife. `exec` und `http` laufen im Helper selbst
  (Foundation `Process` bzw. `URLSession`), Kadrell ist daran nicht beteiligt.
- Lua-Standardbibliothek ohne `os.execute`, `io.popen`, `package.loadlib` und C-Searcher. Shell und HTTP gehen nur
  über `kadrell.exec` und `kadrell.http`, damit sich `permissions` später an genau einer Stelle prüfen lassen.
- Lua meldet Fehler per `longjmp`. Der darf nie durch Swift-Frames laufen: Aufrufe nach Lua gehen über `lua_pcall`
  in einer kleinen C-Schicht, Host-Funktionen geben Fehler als Rückgabewert an die C-Schicht, die erst danach
  `lua_error` auslöst.

### Zustände

`aus → startet → läuft → (Fehler | lädt neu) → …`

| Fall | Verhalten |
|---|---|
| Start | Kadrell schickt `hello`. Kommt binnen 3 s kein `ready`: SIGKILL, Zustand Fehler („startet nicht“). |
| Abschalten | `shutdown`, 1 s Frist, dann SIGTERM, nach weiteren 1 s SIGKILL. Panel und Statuseintrag verschwinden sofort. |
| Absturz | Prozess endet unerwartet: Panel und Statuseintrag weg, letzte stderr-Zeilen im Dialog. Neustart nach 1 s, 5 s, 30 s. Nach 3 Abstürzen in 2 min bleibt sie im Zustand Fehler, bis sie im Dialog wieder angeschaltet wird. |
| Hänger | Kadrell schickt alle 5 s `ping`. Kein `pong` binnen 5 s: SIGKILL, weiter wie Absturz. |
| Flut | Mehr als 50 Nachrichten pro Sekunde, eine Zeile über 1 MB oder ein Panel über 2000 Knoten: SIGKILL, weiter wie Absturz, Grund im Dialog. |
| Datei geändert | Dateiüberwachung auf `~/.config/kadrell/extensions/`, 300 ms entprellt, Neustart nur der betroffenen, eingeschalteten Extension. Zählt nicht als Absturz. |
| Einstellung geändert | Neustart der Extension mit neuer Konfiguration. |
| Kadrell endet | stdin des Helpers schließt, der Helper beendet sich selbst. Zusätzlich SIGTERM an alle Helper beim Beenden. |

Grundregel: Kadrell wartet nie synchron auf eine Extension. Schreiben nach stdin ist nicht blockierend mit
begrenztem Puffer; läuft der Puffer voll, gilt das als Hänger.

### Protokoll (JSON-Zeilen)

Kadrell → Extension:

| Nachricht | Inhalt |
|---|---|
| `hello` | `api`, `name`, `dir`, `storageDir`, `config` (inkl. Secrets), `locale`, `sessions` |
| `event` | `name`, `data` |
| `result` | `id`, `status`, `stdout`, `stderr` (Antwort auf `run`) |
| `ping` | `id` |
| `shutdown` | |

Extension → Kadrell:

| Nachricht | Inhalt |
|---|---|
| `ready` | |
| `pong` | `id` |
| `panel` | `tree` oder `null` |
| `status` | `item` oder `null` |
| `run` | `id`, `argv` (Kadrell-Befehl wie auf der CLI) |
| `log` | `level`, `text` |

`run` läuft über denselben Parser und dieselbe Ausführung wie `ControlCommand` vom Socket. Aufrufer ist „von
außen“, also nicht durch „Sessions dürfen andere Sessions steuern“ eingeschränkt.

## 2. Lua-API

Globale Tabelle `kadrell`. Jeder Handler (Event, Timer, UI-Aktion) läuft als eigene Coroutine. `exec`, `http`, `run`
und `sessions` geben intern ab und laufen weiter, wenn das Ergebnis da ist, sehen für den Autor also synchron aus.
Ein Fehler in einem Handler landet mit Traceback im Log, die Extension läuft weiter.

```lua
-- Events
kadrell.on(name, function(data) ... end)
--   app.ready        einmal nach dem Start, Sessions sind bekannt
--   session.new      data.session
--   session.remove   data.session
--   session.focus    data.session (auch beim Start)
--   session.status   data.session, data.state (working | waiting | idle)
--   ui.action        data.id (aus Panel oder Statusleiste)
-- data.session = {key, title, cwd, branch, sessionId, state, group}

-- Kadrell steuern
kadrell.run("select", "-t", key)        -- {status, stdout, stderr}
kadrell.sessions()                      -- wie `kadrell ls --json`, als Lua-Tabelle

-- Außenwelt
kadrell.exec(argv, {cwd=, timeout=30})  -- {status, stdout, stderr}
kadrell.http{method="GET", url=, headers=, body=, timeout=30}  -- {status, headers, body}
kadrell.json.encode(v) / kadrell.json.decode(s)

-- Zeit
kadrell.every(seconds, fn)              -- gibt Handle zurück, handle:cancel()
kadrell.after(seconds, fn)

-- Daten und Konfiguration
kadrell.storage.get(key) / kadrell.storage.set(key, value)  -- JSON-Datei in storageDir, überlebt Reload
kadrell.config.<key>                    -- Werte aus manifest.settings
kadrell.log(text) / kadrell.warn(text)

-- UI
kadrell.panel.set(tree) / kadrell.panel.clear()
kadrell.status.set(item) / kadrell.status.clear()
```

Erweiterungen der API (Palette, Kürzel, Toasts, Badges) kommen als neue Funktionen und Events dazu, ohne bestehende zu
ändern. Brechende Änderungen erhöhen `apiVersion`.

## 3. UI-Slots

Extensions liefern Bäume, Kadrell prüft und zeichnet. Unbekannte Knotentypen und Felder werden ignoriert und geloggt,
Texte auf 500 Zeichen gekürzt. Farben nur als Theme-Namen: `accent`, `muted`, `ok`, `warn`, `err`. Gezeichnet wird nativ
im Stil des Baums, mit UI-Größe und Theme von Kadrell.

### Rechte Sidebar

```lua
kadrell.panel.set{
  title = "Jira",
  { type = "section", title = "In progress", children = {
      { type = "item", text = "PROJ-123 Login broken", detail = "High", color = "warn",
        actions = { {id = "open:PROJ-123", label = "Open in browser"},
                    {id = "claude:PROJ-123", label = "Start session"} } },
  }},
  { type = "text", text = "Updated 14:02", color = "muted" },
  { type = "button", label = "Reload", action = "refresh" },
}
```

- Knoten v1: `section` (title, children, collapsed), `item` (text, detail, color, actions), `text` (text, color),
  `button` (label, action).
- `panel.set` ersetzt den ganzen Baum. Kein Diffing.
- Ein Tab pro Extension mit Panel, Tab-Leiste oben im Bereich.
- Dritter Bereich im vorhandenen `ThinSplitView` des Hauptfensters. Breite und Sichtbarkeit pro Fenster gespeichert,
  nach dem Muster des Baums (`lastSidebarWidth`, Suffix je Fenster).
- Weitere Fenster (⌘⇧T) zeigen dieselben Panels. Tab-Auswahl und Sichtbarkeit gelten pro Fenster.
- Ohne eingeschaltete Extension mit Panel bleibt der Bereich weg, kein leerer Rand.

Tastatur (über `Hotkeys`, in den Einstellungen belegbar):

| Taste | Wirkung |
|---|---|
| ⌘3 | Fokus auf die rechte Sidebar (analog ⌘1 Baum, ⌘2 Kachel) |
| ⌘⌥B | Rechte Sidebar ein/aus |
| ↑ ↓ | Eintrag wählen |
| ← → | Tab wechseln |
| ⏎ / Klick | erste Aktion des Eintrags |
| ⌥⏎ / Rechtsklick | Menü mit allen Aktionen |
| Esc | Fokus zurück zur Kachel |

Jede Aktion kommt als Event `ui.action` mit ihrer `id` bei der Extension an.

### Statusleiste

```lua
kadrell.status.set{ text = "ddev 3 up", color = "ok", action = "ddev:list" }
```

- Ein Eintrag pro Extension, höchstens 40 Zeichen, rechts in der Leiste vor den Zählern, gezeichnet wie die
  vorhandenen Module in `StatusBarView`.
- Klick schickt `ui.action` mit `action`.

### Verschwinden

Wird eine Extension abgeschaltet oder stirbt sie, verschwinden Tab und Statuseintrag sofort. Läuft sie wieder,
zeichnet sie selbst neu.

## 4. Extensions-Dialog

- Öffnen über Menü „Extensions …“, Palette (`>Extensions`) und ein Kürzel (Vorschlag F4).
- Liste aller gefundenen Extensions: Name, Version, Beschreibung, Zustand (aus, läuft, Fehler), angeforderte
  `permissions` (nur angezeigt).
- Pro Extension: an/aus, „Neu laden“, „Ordner öffnen“, Log (letzte 200 Zeilen aus stderr und `kadrell.log`),
  Einstellungsformular aus `manifest.settings`.
- Fehlergrund im Klartext, z. B. „hing, nach 5 s beendet“, „3 Abstürze in 2 min, bleibt aus“, „Lua-Fehler in
  init.lua:42: …“, „Ordner für andere beschreibbar“, „braucht neueres Kadrell“.
- Tastatur: ↑↓ wählen, Leertaste an/aus, R neu laden, L Log, Esc schließen.
- SwiftUI im vorhandenen `OverlayPanel`, Sprache folgt der Einstellung über `\.locale`.
- Installieren in Teil 1: Ordner nach `~/.config/kadrell/extensions/` legen oder `git clone`. Der Dialog zeigt einen
  Knopf „Ordner öffnen“ für das Extensions-Verzeichnis.

## 5. Doku-Seite

Repo `~/development/projects/kadrell.malura.de`, neue Datei `extensions.html` (Caddy liefert sie dank
`try_files {path} {path}.html` als `/extensions`). Statisches HTML, `styles.css` der Landingpage, nur Englisch.

Inhalt:

1. Quick start: Hello-World aus `kadrell.json` und `init.lua`, zeigt ein Panel und einen Statuseintrag.
2. Manifest reference inkl. `settings` und `permissions` (Hinweis: noch nicht erzwungen).
3. Lifecycle and limits: Timeouts, Neustarts, alles, was zum Kill führt.
4. API reference: Events, Funktionen, UI-Knoten, Theme-Farben, aktuelle `apiVersion`.
5. Publishing: öffentliches GitHub-Repo mit Topic `kadrell-extension`.

Verlinkt von `index.html`, eingetragen in `sitemap.xml`, verlinkt aus der Hilfe in der App (F1) und aus dem
Extensions-Dialog. Keine Em-Dashes, kein AI-Slop.

## 6. Code-Ablage (Vorschlag)

- `app/Vendor/lua/`: Lua-5.5.1-Quellen als eigenes statisches Target in `project.yml`.
- `app/Kadrell/Extensions/`: Manifest, Protokoll, Zustandsmaschine und Prozessverwaltung (Host-Seite), Panel-Baum mit
  Prüfung, Panel-View, Dialog.
- `app/Kadrell/Extensions/Host/`: Helper-Seite (Lua-Laufzeit, C-Schicht, Event-Schleife, `exec`/`http`).
- `app/KadrellTests/Fixtures/extensions/`: `hello`, `crash`, `loop`, `flood`.

## 7. Tests

- Unit: Manifest lesen (gültig, Name passt nicht, apiVersion zu hoch), Panel-Baum prüfen (unbekannte Knoten, Kürzen,
  Größenlimit), Protokoll kodieren und lesen, Zustandsmaschine mit Fake-Prozess (Backoff 1/5/30 s, 3 Abstürze in
  2 min, Reload zählt nicht als Absturz).
- Integration mit echtem `ext-host` und den Fixtures: `hello` liefert Panel und Status; `crash` endet im Zustand Fehler
  mit Lua-Fehlermeldung; `loop` wird per Ping gekillt; `flood` wird gekillt. Danach antwortet Kadrell weiter
  (z. B. `kadrell ls` über den Socket).
- Lokalisierung: alle neuen Texte deutsch und englisch in `Localizable.xcstrings`, `LocalizationTests` grün.
- Alles mit `TEST_RUNNER_KADRELL_PROFILE=tmp` bzw. `--profile tmp`.
- Clean Build ohne eigene Warnungen, `cd app && uv tool run lizard Kadrell -w` leer. Warnungen der Lua-Quellen
  werden im Lua-Target abgeschaltet (Flag beim Bauen verifizieren), nicht im App-Target.

## Offene Punkte für die Umsetzung

- F4 und ⌘⌥B gegen alle bestehenden Belegungen in Menü und `Hotkeys` prüfen, bevor sie Defaults werden.
- Wie das Lua-Target in `project.yml` (XcodeGen) am einfachsten als statische Bibliothek eingebunden wird, beim
  Bauen verifizieren.
- Changelog-Eintrag und Versionssprung (`minor`) beim Commit der Umsetzung.
