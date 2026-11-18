# Kadrell-CLI: gemeinsamer Vertrag von Mac und Windows

Die Mac-App (`app/`, Swift) und Kadrell für Windows (`windows/`, Rust) teilen keinen Code, aber diese
Schnittstelle. Skripte wie start-my-day rufen auf beiden Systemen dieselben Befehle auf und lesen dieselbe
JSON-Ausgabe. Ändert sich hier etwas, ziehen beide Seiten im selben Arbeitsgang nach.

Quellen der Wahrheit: `app/Kadrell/Services/Control.swift` (Befehle, Flags, Meldungen) und
`app/Kadrell/App/AppDelegate+Control.swift` (Ausführung, `ls`). Windows bildet sie in `windows/src/command.rs`
und `windows/src/app.rs` nach.

## Befehle

Gleich auf beiden Systemen, mit denselben Flags und Fehlermeldungen (`kadrell: <meldung>` auf stderr, Exit 1):

| Befehl | Ausgabe |
| --- | --- |
| `ls [--json]` | Gruppen und Sessions, JSON siehe unten |
| `new-group <ordner> [--name N] [--color #rrggbb]` | Gruppen-Id (Favorit, bleibt leer stehen) |
| `new [-t gruppe] [-c ordner] [--name N] [-d] [--resume id] [--model m] [--effort e] [prompt …]` | Session-Key in der letzten Zeile |
| `select`, `rename`, `move`, `set-group`, `stop`, `resume`, `kill`, `kill-group` | nichts |
| `send [-t s] [--no-enter] <text …>`, `send [-t s] -k <taste …>` | nichts, kehrt erst nach dem Enter zurück |
| `capture [-t s] [--all]` | Bildschirmtext, Leerzeichen am Zeilenende und leere Zeilen am Schluss entfernt |
| `status working\|waiting\|idle [-t s] [--session-id] [--title] [--waiting-for] [--message] [--first-prompt]` | nichts |
| `hook claude` | liest Hook-JSON von stdin, meldet `status`, endet immer mit 0 |

Ziele (`-t`): Session-Key, sessionId oder deren Anfang, Titel; Gruppen-Id oder deren Anfang, Gruppenname.
Exakt geht vor Anfang, mehrdeutig ist ein Fehler. Ohne `-t` gilt die aufrufende Session
(`KADRELL_SESSION_KEY`), sonst die fokussierte. `new` ohne `-t` und `-c` aus einer Session heraus landet in deren
Gruppe und Ordner.

Nur Mac: `ssh-hosts`, `layout`, `zoom`, `new --host/--tmux/--pi`, `select -a`. Nur Windows: `kill-server`
(beendet die Instanz, wie bei tmux). Windows meldet die Mac-Befehle als unbekannt.

## `ls --json`

Form siehe `docs/contract/ls.json`. Der Windows-Test `list_json_matches_contract` prüft, dass die eigene Ausgabe
genau diese Schlüssel und Typen hat. `branch` fehlt, wenn der Ordner kein Git-Repo ist. `status` ist bei laufendem
Prozess `running`, `waiting`, `idle` oder `error`, sonst `stopped` (gestoppt oder noch nicht gestartet) oder
`ended` (Prozess hat sich selbst beendet). `layout` ist auf Windows immer `stack`.

## Umgebung jeder Kachel

`KADRELL_SESSION_KEY` (Key der Kachel), `KADRELL_SOCKET` (Mac: Pfad des Unix-Sockets, Windows: Pfad der
Endpunktdatei mit Port und Token), `KADRELL` (Pfad des Binaries, für die Hook-Zeile). Jeder Claude-Prozess bekommt
per `--settings` die Hooks SessionStart, UserPromptSubmit, PermissionRequest, Notification und Stop mit der Zeile
`[ -n "$KADRELL_SOCKET" ] && exec "$KADRELL" hook claude || exit 0`.

## Steuerkanal

Eine Zeile JSON `{"argv": [...], "cwd": "...", "caller": "..."}` hin, eine Zeile `{"status": 0, "stdout": "...",
"stderr": "..."}` zurück. Mac: Unix-Socket, nur der eigene Benutzer (Dateirechte, `getpeereid`). Windows: TCP auf
127.0.0.1, die Anfrage trägt zusätzlich `token` aus der Endpunktdatei im Profilordner.
