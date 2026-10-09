# Git-Repos in Cmd-N: Design (Core-Palette-API + Git-Extension)

Stand 2026-10-09. Baut auf der Extension-Runtime aus `2026-10-04-lua-extensions-design.md` auf. Zwei Teile:
Teil A erweitert den Core (neue Lua-API, die jede Extension nutzen kann), Teil B ist die konkrete Git-Extension,
die darauf aufsetzt. Die Jira-Extension (`2026-10-09-jira-extension-design.md`) nutzt aus Teil A nur `kadrell.secret`.

## Ziel

Beim "Neue Session"-Dialog (⌘N) sollen neben den lokalen Ordnern auch entfernte Repositories von GitHub, Bitbucket
und beliebigen Git-Remotes (z. B. Vault) als Kandidaten erscheinen. Auswahl eines entfernten Repos klont es in einen
Zielordner und öffnet dort eine Session. Die Provider-Logik (Auth, API-Eigenheiten, Zielordner, Klonen) lebt
vollständig in einer Lua-Extension; der Core bekommt nur einen dünnen, Git-unabhängigen Erweiterungspunkt.

## Abgrenzung zur Teil-1-Spec

Die Teil-1-Spec führte als Nicht-Ziel "Keine Palette-Einträge aus Extensions". Dieses Dokument hebt genau das
auf: Teil A ist die additive API, die Extensions Einträge in ⌘N beisteuern lässt. Additiv heißt **kein
apiVersion-Sprung** (Projektregel: nur brechende Änderungen erhöhen `apiVersion`). Extensions erkennen die neue API
per `if kadrell.palette then …` bzw. `if kadrell.secret then …`.

Wichtige Begriffsklärung: ⌘N ist das `NewSessionSheet`/`NewSessionModel`, nicht die ⌘P-Omni-Leiste
(`PaletteWindow`). Lokale Git-Repos erscheinen dort schon heute als Kandidaten (`FolderIndex.repos`, Tag "Git").
Diese Spec ergänzt *entfernte* Repos, die beim Auswählen erst geklont werden.

---

# Teil A: Core-Erweiterung

Zwei neue Lua-APIs, beide additiv, beide über das vorhandene Host-Protokoll.

## A1. `kadrell.palette` (Einträge für ⌘N)

```lua
kadrell.palette.set(items)   -- ersetzt die Liste dieser Extension
kadrell.palette.clear()      -- = set({})
-- items = { { id = "gh:acme/web", title = "acme/web", detail = "GitHub · clone", group = "GitHub" }, … }
```

- `id` ist extension-intern und kommt bei Auswahl im `palette.select`-Event zurück.
- `title` wird als Kandidatentext/Score-Grundlage genutzt, `detail` als Untertitel, `group` als Tag.
- Semantik wie `kadrell.panel.set`: letzter Stand gewinnt, genau einmal pro Dispatch gesendet (Coalescing über
  `latest`/`flush` in `prelude.lua`), damit eine set-Schleife den Core nicht flutet.

### Auswahl: Event `palette.select`

Wählt der Nutzer in ⌘N einen Eintrag, der von einer Extension stammt, ruft der Core **nicht** `startSession`,
sondern feuert an die besitzende Extension:

```lua
kadrell.on("palette.select", function(data) -- data.id = "gh:acme/web"
  -- Extension klont und öffnet selbst
end)
```

Das nutzt den generischen Event-Kanal (`HostMessage.event`), kein neuer Wire-Typ nötig. Der Core merkt sich pro
Palette-Item, welche Extension es geliefert hat, und schickt das Event nur dorthin.

### Anzufassende Stellen (verifiziert)

Host→Core, neuer Nachrichtentyp `palette` (analog `panel`):
- `app/Kadrell/Extensions/Host/prelude.lua`: `kadrell.palette.set/clear` als `latest.palette = json.encode({t="palette", items=…})` (bei `panel.set`, L168), Mitsenden in `flush()` (L162-166).
- `app/Kadrell/Extensions/ExtensionMessage.swift`: neuer `case palette(JSONValue?)` im `enum` (L5) und `case "palette":` in `decode` (L10-18).
- `app/Kadrell/Extensions/ExtensionManager.swift`: `case .palette` in `received()` (L244-261), neue `setPalette(_:name:)` analog `setPanel`/`setStatus` (L263-289), neues Feld `palettes` analog `trees`/`statuses` (L40-41).
- Validierung: `PaletteValidation.items(_:)` analog `PanelValidation.tree`/`.status` (jedes Item braucht `id`+`title`, sonst verworfen und geloggt; Gesamtgröße deckeln wie der Panel-Baum).

Einspeisung in ⌘N + Auswahl zurückrouten:
- `app/Kadrell/UI/NewSessionSheet.swift`: `Candidate` (L8-14) um eine Herkunft ergänzen (Extension-Name + Item-`id`); `NewSessionModel.update()` (L47-81) die Palette-Items des Core als Kandidaten einmischen (neben `index.repos` L61-62), Tag = `group`.
- `app/Kadrell/App/AppDelegate.swift`: beim `onStart`-Handler (L1167-1170) bzw. `NewSessionModel.pick` (L94-97) prüfen, ob der Kandidat von einer Extension stammt. Wenn ja: `ExtensionManager.emit`/gezielte Zustellung von `palette.select` statt `startSession`. Wenn nein: wie bisher.
- Quelle der Items: `ExtensionManager` hält `palettes` und reicht sie beim Öffnen von ⌘N durch (Verdrahtung analog Panels/Status in `AppDelegate+Extensions.swift:21-51`).

Flut-Einstufung: `palette` wird wie `panel` pro Dispatch gecoalesct, fällt also nicht unter den 50/s-Kill
(`ExtensionProcess.swift` `ProtocolLines.take`). Bei der Umsetzung bestätigen, dass `palette` dort wie `panel`
und nicht wie ein ungedrosselter Typ behandelt wird.

## A2. `kadrell.secret` (Schlüsselbund für Extensions)

```lua
kadrell.secret.get(key)      -- string | nil
kadrell.secret.set(key, value)
kadrell.secret.delete(key)
```

- Für Secrets, die eine Extension zur Laufzeit erhält oder verwaltet (OAuth-Token, mehrere Instanz-Token), die also
  nicht über das statische `secret`-Settingfeld eingegeben werden.
- Ablage im vorhandenen `Keychain`-Service, **genau nach dem Schema der `secret`-Settings**: ein Eintrag pro Profil,
  Extension und Schlüssel. Der Core setzt das Namespacing, die Extension sieht nur ihre eigenen Schlüssel.
- Request/Response wie `exec`/`http` (läuft in Coroutine, nur in Handlern erlaubt), Antwort über das vorhandene
  `result` bzw. einen schlanken eigenen Reply.

### Anzufassende Stellen

- `app/Kadrell/Extensions/Host/prelude.lua`: `kadrell.secret.get/set/delete` über `request("secret", {op=…, key=…, value=…})` (Muster `kadrell.exec`, L82-96; nur in Handlern, L64-66).
- `app/Kadrell/Extensions/ExtensionMessage.swift`: neuer `case secret(id:Int, op:String, key:String, value:String?)` (L5) + `case "secret":` in `decode`.
- `app/Kadrell/Extensions/ExtensionManager.swift`: in `received()` (L244-261) die Operation gegen `Keychain` ausführen und `.result` zurückschicken (bei `get` den Wert in `stdout`).
- `app/Kadrell/Services/Keychain.swift`: vorhandene Get/Set/Delete wiederverwenden; falls nötig eine Variante mit dem Extension-Namespace-Schlüssel (kein neues Speicherkonzept).

## Tests Teil A

- `palette.set` kodieren/lesen, Validierung (Item ohne `id`/`title` verworfen, Größenlimit), Coalescing
  (set-Schleife sendet einmal), `palette.select` kommt nur bei der liefernden Extension an.
- ⌘N mischt Extension-Items als Kandidaten ein; Auswahl eines Extension-Items ruft nicht `startSession`.
- `kadrell.secret` round-trip (set→get→delete), Namespacing (Extension A sieht Schlüssel von B nicht).
- Clean Build ohne eigene Warnungen, `uv tool run lizard Kadrell -w` leer, `LocalizationTests` grün (neue
  ⌘N-Sektionsüberschrift o. ä. zweisprachig über `Bundle.app`).

---

# Teil B: Git-Extension

Reine Lua-Extension, nutzt `kadrell.palette` und `kadrell.secret` aus Teil A.

## Provider und Auth (Hybrid)

| Provider | Repos listen | Auth |
|---|---|---|
| GitHub | REST `GET /user/repos`, `/orgs/<org>/repos` | OAuth **Device Flow**: `exec open` + `http`-Polling, Token in `kadrell.secret` |
| Bitbucket Cloud | REST `GET /2.0/repositories/<workspace>` | App-Passwort / Token (statisches `secret`-Setting oder `kadrell.secret`) |
| Generisches Git (Vault) | keine Listing-API | Manuell hinterlegte Klon-URLs; Auth per vorhandenem SSH-Key/Token der Shell |

- **GitHub Device Flow:** `POST https://github.com/login/device/code` (client_id, scope `repo`), dem Nutzer `user_code`
  + `verification_uri` anzeigen und `exec open verification_uri`, dann `POST .../login/oauth/access_token` im
  `interval` pollen bis `access_token`. Token mit `kadrell.secret.set("github:token", …)`. client_id ist öffentlich,
  Device Flow braucht kein Client-Secret.
- **Bitbucket, Vault:** kein Browser-Flow. Token/URLs über das Einstellungsformular bzw. eine mehrzeilige URL-Liste.

## Konfiguration (Manifest-Settings)

```json
"settings": [
  {"key": "githubClientId", "type": "string", "label": "GitHub OAuth app client ID"},
  {"key": "githubOrgs",     "type": "string", "label": "GitHub orgs (one per line, blank = your repos)"},
  {"key": "bitbucketWorkspace", "type": "string", "label": "Bitbucket workspace"},
  {"key": "bitbucketToken", "type": "secret", "label": "Bitbucket app password"},
  {"key": "vaultRepos",     "type": "string", "label": "Extra git URLs, one per line"},
  {"key": "cloneRoot",      "type": "string", "label": "Clone into (base folder)", "default": "~/development/projects"}
]
```

- Zielordner eines Repos: `cloneRoot/<repo-name>`. Später optional pro Provider/Repo verfeinerbar (YAGNI jetzt).

## Datenfluss

1. `app.ready` + `kadrell.every(600, …)` + Panel-Button "Refresh repos":
   - GitHub/Bitbucket per `kadrell.http` mit Token listen, Vault-URLs aus `vaultRepos` nehmen.
   - Ergebnis in `kadrell.storage.set("repos", …)` cachen (Offline-/Sofortanzeige beim Start).
   - `kadrell.palette.set(items)` mit je `{ id = "<provider>:<slug>", title = "<slug>", detail = "<provider> · clone",
     group = "<provider>" }`.
2. `palette.select` mit `data.id`:
   - Zielordner `expand(cloneRoot).."/"..name`. Existiert er schon: direkt `kadrell.run{"new","-c",ziel}` (kein
     erneutes Klonen).
   - Sonst: Klon-URL aus dem gecachten Repo zu `id`, `kadrell.exec{"git","clone",url,ziel}` (Fortschritt/Fehler über
     `kadrell.status.set`), bei Erfolg `kadrell.run{"new","-c",ziel}`.
   - Fehler (Klon schlägt fehl, Ordner nicht anlegbar): `color="err"` im Panel + Log, keine Session.
3. Panel zeigt zusätzlich Login-Status je Provider und eine "Connect GitHub"-Aktion (startet Device Flow), wenn kein
   Token vorliegt.

`kadrell.run{"new","-c",ziel}` verlangt einen **existierenden** Ordner (`existingDir` in `AppDelegate+Control.swift`).
Deshalb erst klonen (legt den Ordner an), dann `new`. Das ist der Grund, warum der Klon in der Extension läuft und
nicht über ein einziges ⌘N-Kommando.

## Panel-Baum

```lua
kadrell.panel.set{
  title = "Repos",
  children = {
    { type = "section", title = "GitHub", children = {
      { type = "text", text = tr("Not connected"), color = "warn" },      -- oder Repo-Zähler
      { type = "button", label = tr("Connect GitHub"), action = "gh:connect" },
    }},
    { type = "text", text = tr("Open Cmd-N to clone a repo"), color = "muted" },
    { type = "button", label = tr("Refresh repos"), action = "refresh" },
  },
}
```

Die eigentliche Repo-Liste erscheint in ⌘N (über `kadrell.palette`), nicht im Panel. Das Panel ist für
Login/Status/Refresh da.

## Code-Ablage

Eigenes Repo `kadrell-git` mit Topic `kadrell-extension`, Entwicklung unter `~/.config/kadrell/extensions/git/`:

```
git/
  kadrell.json
  init.lua       Events, Panel, palette.set, palette.select, Device-Flow-Steuerung
  providers.lua  reine Funktionen: Repo-JSON → items, slug→Klon-URL, Zielordner, URL-Liste parsen
```

## Tests Teil B

- `providers.lua` assert-Selbsttest: GitHub-/Bitbucket-JSON → items (slug, Klon-URL), `vaultRepos`-Parse,
  Zielordner-Ableitung (`cloneRoot` mit `~`), id-Roundtrip (`provider:slug` ↔ Klon-URL).
- Device-Flow-Zustand (Polling-Schleife, `authorization_pending`/`slow_down`/`access_token`) gegen Fake-HTTP.
- Integration: Fixture-Extension pusht Palette-Items, ⌘N zeigt sie, `palette.select` löst den Klon-Pfad aus
  (mit Fake-`exec`).

## Offene Punkte für die Umsetzung

- GitHub OAuth-App registrieren (client_id), Device Flow in der App-Doku beschreiben. Scope minimal (`repo` bzw.
  `public_repo`), beim Umsetzen gegen die aktuelle GitHub-Doku verifizieren.
- Bitbucket Cloud vs. Server: diese Spec deckt Cloud. Falls Server gebraucht wird, eigene kleine Ergänzung.
- Reihenfolge: **zuerst Teil A** (Core), dann Jira-Extension und Git-Extension. Beide Extensions hängen an
  `kadrell.secret`, die Git-Extension zusätzlich an `kadrell.palette`.
- Changelog-Eintrag (nutzersichtbar: "Neue-Session-Dialog kann Repos von Extensions anbieten") und Versionssprung
  `minor` beim Commit des Core-Teils. Doku-Seite `extensions.html` um `palette` und `secret` ergänzen,
  `docs/lua-extension-api.md` im selben Arbeitsgang nachziehen.
</content>
