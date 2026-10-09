# Jira-Extension: Design

Stand 2026-10-09. Baut auf der Extension-Runtime aus `2026-10-04-lua-extensions-design.md` auf
(Teil 1, Punkt 2 der dortigen Zerlegung: "Jira-Extension"). Setzt die kleine Schlüsselbund-API
`kadrell.secret` aus `2026-10-09-git-repos-in-cmd-n-design.md` voraus (dort Teil A).

## Ziel

Eine Lua-Extension zeigt in der rechten Sidebar die für Michael relevanten Jira-Tickets aus einer oder mehreren
Jira-Cloud-Instanzen. Klick auf ein Ticket öffnet es wahlweise im Browser oder, wenn das Ticket-Projekt mit einem
Ordner verknüpft ist, als Kadrell-Session in genau diesem Ordner. Kein Core-Eingriff über `kadrell.secret` hinaus,
reine apiVersion-1-Extension.

## Nicht-Ziele

- Keine Ticket-Bearbeitung (Status ändern, kommentieren, Zeit buchen). Nur Lesen und Öffnen.
- Kein Jira Server / Data Center. Nur Jira Cloud (`*.atlassian.net`, REST v3, Basic Auth mit E-Mail + API-Token).
- Kein OAuth. Jira Cloud wird mit E-Mail + API-Token angebunden (der im Hybrid-Auth-Entscheid gewählte Token-Weg).
- Keine eigene Eingabemaske jenseits des Einstellungsformulars der Extension (die Runtime bietet keinen freien
  Eingabedialog für Extensions).

## Entscheidungen

| Frage | Entscheidung | Grund |
|---|---|---|
| Auth | E-Mail + API-Token, Basic (`Authorization: Basic base64(email:token)`) | Jira-Cloud-Standard, kein Browser-Flow, passt in die Extension |
| Token-Ablage | `kadrell.secret`, ein Eintrag pro Instanz | Schlüsselbund statt Klartext, beliebig viele Instanzen ohne feste Slots |
| Mehrere Instanzen | Liste in `kadrell.storage` (URL, E-Mail, JQL, Ordner-Map), Token getrennt in `kadrell.secret` | Nur die Token sind geheim, der Rest darf im Profil-JSON liegen |
| Instanz hinzufügen | Über das Einstellungsformular, eine Instanz pro Reload (siehe unten) | Extensions haben keine freie Eingabemaske; das Formular ist die einzige Eingabefläche |
| Welche Tickets | Pro Instanz eine JQL, Default "mir zugewiesen und offen" | Flexibel, deckt "offen / zugewiesen / wie auch immer" ab |
| Ticket-Klick | Zwei Aktionen pro Ticket: im Browser öffnen, im Projektordner öffnen | Deckt beide Workflows ab, Ordner nur wenn gemappt |
| Sprache der Labels | Folgt dem `locale` aus `hello` (de/en) | Nutzersichtbarer Text, die App ist zweisprachig |

## Instanzen und Konfiguration

Eine Instanz ist `{ baseUrl, email, jql, folders }` plus ein Token im Schlüsselbund.

- Nicht-geheime Felder: Liste unter `kadrell.storage.get("instances")` (Array von Objekten).
- Token: `kadrell.secret` unter dem Schlüssel `token:<baseUrl>` (Namespacing pro Profil+Extension macht der Core).
- `folders`: Abbildung Projektschlüssel → absoluter Ordnerpfad, z. B. `{ BODO = "/Users/michael/.../bodo" }`.
  Mehrzeilige Eingabe `PROJ=/pfad` je Zeile im Einstellungsformular, die Extension parst sie.

### Instanz hinzufügen oder ändern (Eingabe-Flow)

Das Manifest hat ein statisches Einstellungsformular mit genau einem "Eingabe-Slot":

```json
"settings": [
  {"key": "baseUrl", "type": "string", "label": "Jira URL (e.g. https://acme.atlassian.net)"},
  {"key": "email",   "type": "string", "label": "Account email"},
  {"key": "token",   "type": "secret", "label": "API token (from id.atlassian.com)"},
  {"key": "jql",     "type": "string", "label": "JQL", "default": "assignee = currentUser() AND resolution = Unresolved ORDER BY updated DESC"},
  {"key": "folders", "type": "string", "label": "Project folders, one per line: PROJ=/path"}
]
```

- Beim Reload (Einstellung geändert) liest die Extension `kadrell.config.baseUrl/email/token/jql/folders`.
- Ist `baseUrl` gesetzt, trägt die Extension die Instanz in die `instances`-Liste ein oder aktualisiert sie
  (Schlüssel = `baseUrl`), schreibt das Token mit `kadrell.secret.set("token:"..baseUrl, token)` und **leert danach
  nichts am Formular** (das Formular bleibt als "zuletzt bearbeitete Instanz" stehen).
- Eine zweite Instanz: im Formular die Felder auf die neue Instanz ändern, speichern, Reload. Die erste bleibt in der
  Liste erhalten.
- Eine Instanz entfernen: Panel-Aktion "Remove instance" an der Instanz-Überschrift (feuert `ui.action`
  `rm:<baseUrl>`, löscht Listeneintrag und `kadrell.secret.delete`).

Das ist eine bewusste UX-Krücke, weil Extensions keinen freien Eingabedialog haben.
`shortcut: eine Instanz pro Reload über das Formular; auf echten "Instanz hinzufügen"-Dialog umstellen, sobald die
Runtime Extensions eine Eingabemaske gibt.`

## Datenfluss

1. `app.ready` und `kadrell.every(300, …)` und Panel-Button "Reload" lösen `refresh()` aus.
2. `refresh()` geht pro Instanz:
   - Token aus `kadrell.secret.get("token:"..baseUrl)`. Fehlt es, Instanz als "nicht angemeldet" im Panel zeigen.
   - `kadrell.http{ method="GET", url = baseUrl.."/rest/api/3/search/jql?jql="..enc(jql).."&fields=summary,status,priority",
     headers = { Authorization = "Basic "..b64(email..":"..token), Accept = "application/json" } }`.
   - Antwort per `kadrell.json.decode`, Felder je Ticket: `key`, `fields.summary`, `fields.status.name`,
     `fields.priority.name`.
3. Ergebnis in `kadrell.storage.set("cache:"..baseUrl, tickets)` legen, damit das Panel beim nächsten Start sofort
   etwas zeigt (dann im Hintergrund aktualisieren).
4. `render()` setzt den Panel-Baum.

Fehler (HTTP != 2xx, Timeout, Decode): Instanz-Abschnitt zeigt `color = "err"` mit Kurzgrund, die anderen Instanzen
bleiben sichtbar. Kein Absturz.

## Panel-Baum

```lua
kadrell.panel.set{
  title = "Jira",
  children = {
    { type = "section", title = "acme.atlassian.net", children = {
      { type = "item", text = "BODO-123 Login kaputt", detail = "In Progress · High", color = "warn",
        actions = { { id = "open:https://acme.atlassian.net/browse/BODO-123", label = tr("Open in browser") },
                    { id = "folder:acme.atlassian.net:BODO-123",              label = tr("Open in project folder") } } },
      { type = "text", text = tr("Updated").." 14:02", color = "muted" },
    }},
    { type = "button", label = tr("Reload"), action = "refresh" },
  },
}
```

- Erste Aktion (⏎/Klick) ist "Open in browser". Zweite (⌥⏎/Rechtsklick) "Open in project folder".
- Ist das Projekt eines Tickets nicht in `folders` gemappt, fehlt die Ordner-Aktion und ein `text`-Hinweis nennt das.

## UI-Aktionen (`ui.action`)

| `id` | Wirkung |
|---|---|
| `refresh` | `refresh()` neu laufen lassen |
| `open:<url>` | `kadrell.exec{"open", url}` (Standardbrowser) |
| `folder:<baseUrl>:<KEY>` | Projektpräfix aus `KEY` (`BODO-123` → `BODO`), Ordner aus `folders`, dann `kadrell.run{"new", "-c", ordner}` |
| `rm:<baseUrl>` | Instanz aus Liste und Schlüsselbund entfernen, `render()` |

Projektpräfix: alles vor dem letzten `-` im Ticket-Key. Fehlt der Ordner oder existiert er nicht, Warnung ins Log und
`color = "err"`-Hinweis, kein stiller Fehlschlag.

## Code-Ablage

Eigenes öffentliches Repo `kadrell-jira` mit Topic `kadrell-extension` (konsistent mit dem Marktplatz), Entwicklung
lokal unter `~/.config/kadrell/extensions/jira/`:

```
jira/
  kadrell.json   Manifest (oben)
  init.lua       Events, render, refresh, Aktionen
  jira.lua       reine Funktionen: jql-Encode, Projektpräfix, folders-Parse, Ticket-Mapping
```

`jira.lua` enthält keine Host-Aufrufe, nur reine Logik, damit sie testbar ist.

## Tests

- `jira.lua` mit assert-basiertem Selbsttest (eigene `jira_test.lua`, über `ext-host` gegen eine Fixture laufbar):
  Projektpräfix (`BODO-123` → `BODO`, `A-B-7` → `A-B`), `folders`-Parse (mehrzeilig, Leerzeilen, `=` im Pfad),
  JQL-Encode (Leerzeichen, Sonderzeichen), Ticket-Mapping aus einer Fixture-JSON-Antwort.
- Integration über eine Fixture-Extension mit einem Fake-HTTP-Ergebnis (kein echter Jira-Zugriff im Test), prüft
  Panel-Baum-Form und die Aktions-`id`s.
- Clean Build nicht betroffen (reine Lua-Extension). Core-Tests zur `kadrell.secret`-API liegen in der Git-Spec.

## Offene Punkte für die Umsetzung

- Exakter Jira-Cloud-Such-Endpunkt beim Umsetzen gegen die aktuelle Atlassian-Doku verifizieren
  (`/rest/api/3/search/jql` vs. der ältere `/rest/api/3/search`; Atlassian hat 2024/2025 umgestellt). Nicht raten.
- Paginierung der Suche (`maxResults`, `nextPageToken`): für "meine offenen Tickets" meist unnötig, aber eine harte
  Obergrenze (z. B. 50) setzen, damit der Panel-Baum klein bleibt.
- Labels zweisprachig: `tr()` liest `locale` aus `hello`. Tabelle de/en in `init.lua`, keine App-Bundle-Strings.
- Setzt `kadrell.secret` (Git-Spec Teil A) voraus; diese Extension erst nach dem Core-Teil bauen.
</content>
</invoke>
