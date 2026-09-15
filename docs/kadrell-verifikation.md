# Kadrell: Verifikation (Stand 15.09.2026)

Alles hier wurde real auf Michaels Mac geprüft (Claude CLI 2.1.272, Xcode 26.6, Swift 6.3.3).

## Spike: Attach-Client hängt sich auf, Session läuft weiter

Ablauf (`/tmp/kadrell-spike/spike_attach.py`, Python `pty.fork`):

1. `claude --bg --name kadrell-spike "Antworte nur mit dem Wort PONG und warte dann."` in `/tmp/kadrell-spike`.
2. `claude attach 86f99758` in einer PTY geöffnet, 8 s Output gelesen (3367 Bytes TUI).
3. Attach-Client mit `SIGHUP` beendet und die PTY-Master-Seite geschlossen. Client exit status 1.
4. `claude agents --json --all` danach: Session `86f99758` weiterhin `kind: background`, `state: working`, `status: idle`, gleiche `pid`.

**Ergebnis: Das Beenden des Attach-Clients (SIGHUP, PTY zu) reißt die Session nicht mit.**
Die App hängt Kacheln deshalb per `SIGHUP` an den `attach`-Prozess aus und beendet beim
App-Ende nichts aktiv (der Kernel schließt die PTYs). Kein Fallback nötig.

## `claude --bg`: tatsächliches stdout

```
warning: --bg manages the session id; ignoring --session-id (use --resume <id> to continue an existing session)
Starting background service…
backgrounded · 86f99758 · kadrell-spike
  claude agents             list sessions
  claude attach 86f99758    open in this terminal
  claude logs 86f99758      show recent output
  claude stop 86f99758      stop this session
```

- **`--session-id` wird von `--bg` ignoriert** (Warnung, Exit 0). Der Weg aus dem Brief (eigene UUID
  vorgeben) funktioniert nicht. Die App parst stattdessen die Zeile `backgrounded · <id> · <name>`
  (`id` = 8 Hex-Zeichen) und pollt zur Sicherheit zusätzlich `agents --json` nach dem Eintrag mit
  dieser `id`.
- `claude --bg --resume <sessionId>` weckt eine gestoppte Session **unter derselben `id`** wieder auf:
  `note: woke session 86f99758 with its saved options (--name, --model).` und dann dieselbe
  `backgrounded · 86f99758 · kadrell-spike (idle — send a prompt to start)`-Zeile.
- `claude stop <id>` antwortet `stopped <id>`; die Session erscheint danach mit `state: stopped`
  (ohne `pid`, ohne `status`) in `agents --json --all`.

## `claude agents --json --all`: beobachtete Felder und Werte

Felder: `cwd id kind name pid sessionId startedAt state status waitingFor`.

| Feld | Werte (beobachtet) | Bedeutung |
|---|---|---|
| `kind` | `background`, `interactive` | interaktive Sessions haben kein `id`, aber `pid` |
| `state` (background) | `working`, `blocked`, `done`, `stopped` | Prozesszustand |
| `status` (beide) | `busy`, `idle`, `waiting` | Aktivität, bei laufenden background-Sessions zusätzlich zu `state` |
| `waitingFor` | `"input needed"` | nur bei `status: waiting` |

Mapping in der App (`Session.mapStatus`): `status` hat Vorrang, dann `state`.
`busy`/`working` → running, `waiting`/`blocked` → waiting, `idle`/`done`/`stopped` → idle,
alles mit `error`/`fail` → error, unbekannte Werte → idle plus Log-Zeile.

## `claude logs <id>`

- Für background-Sessions liefert es den rohen Terminal-Output inklusive ANSI-Sequenzen
  (kein zeilenweiser Text, sondern Screen-Redraws).
- Für interaktive Sessions: `No job matching '<id>'. Run 'claude agents' to list running sessions.`
  Interaktive Kacheln zeigen deshalb nur die Kopfzeile plus das Label `LÄUFT IN ANDEREM TERMINAL`.

## `claude attach <id>`

Hilfetext: „Open the background session in this terminal. ← returns to agent view, Ctrl+Z drops back
to your shell. The session keeps running either way.“ Der Attach-Client aktiviert Mouse-Tracking
(`?1000h ?1002h ?1003h ?1006h`) und die Kitty-Keyboard-Abfragen; SwiftTerm beantwortet beides.

## Laufzeit

`claude agents --json --all` braucht rund 0,3 s (43 Sessions), Polling alle 2 s ist unkritisch.

## SwiftTerm (Commit 233c6ba, 14.09.2026)

- `LocalProcessTerminalView.startProcess(executable:args:environment:execName:currentDirectory:)`
- Buffer-Zugriff ohne Terminal-Objekt: `terminalStateSnapshot().visibleRows[].text`
  (`public nonisolated`, sichtbare Zeilen der aktuellen Ansicht).
- `LocalProcessTerminalView.process.shellPid` für `kill(pid, SIGHUP)`.
- `processTerminated(_:exitCode:)` ist `open` und wird auf dem Main-Thread geliefert.

## Ergebnisse aus dem App-Test (15.09.2026, 16:00 bis 16:20)

- **`kill -9` der App:** alle 43 Sessions bleiben in `agents --json --all`, die Test-Session `1ac4d16a`
  weiter `state: blocked, status: idle` mit derselben `pid`. Die `claude attach`-Clients sterben mit der
  PTY, nach dem Neustart hängt die Queue neu an.
- **CPU beim Start** mit 43 Sessions (davon 1 anhängbar): 2 bis 7 % eines Kerns (`ps -o %cpu`).
- **`blocked` heißt nicht „Prozess läuft“:** eine Hintergrund-Session mit `state: blocked` **und** `pid`
  ist ein lebender Prozess, der am Prompt wartet (`status: idle`). Einträge mit `blocked` **ohne** `pid`
  sind Leichen: `claude attach` antwortet „Couldn't wake … This session has no saved transcript … `claude
  respawn <id>` starts this one fresh“ und beendet sich mit Exit 1. Die App zeigt solche Kacheln als
  „PROZESS WEG · KLICK STARTET NEU“ und ruft bei Klick `claude respawn <id>`.
- **Tippen kommt an:** Fokus auf eine angehängte Session, Text getippt, Enter, Claude Code antwortet im
  Terminal der Kachel (Screenshots `10-focus-typed.png`, `11-focus-answer.png`).
- **`claude --bg` im fremden Terminal** erscheint beim nächsten 2-s-Poll in der App, Gruppe nach `cwd`.

## Abweichungen vom Brief (mit Michael am 15.09. abgestimmt)

- **Esc geht an Claude Code**, nicht an die App: Esc ist in Claude Code der Interrupt. Das Terminal
  verlässt man mit **⌘Esc** (stufenweise Terminal → Gruppe → alles), mit **⌘ + Scrollrad** oder Pinch
  (beide zoomen auch über dem eingehängten Terminal). Die Kachel-Kopfzeile zeigt im Fokus „⌘Esc zurück“.
  Ohne Fokus reicht Esc wie im Prototyp.
- **Nur Hintergrund-Sessions.** Interaktive Sessions (`kind: interactive`, laufen in iTerm/tmux) werden
  nicht angezeigt; die Registry filtert sie weg. Punkt 3 des Briefs entfällt damit.
- **Leere Gruppen verschwinden** beim nächsten Abgleich von selbst; das X an einer leeren Gruppe fragt
  nicht nach. Rückfragen bestätigt plain ⏎, Esc bricht ab.
- **Keine Gruppen-Chips in der Statusleiste** (bei 30 Gruppen zu voll). Links steht nur der Breadcrumb.
- **Rückfragen und Fehler** sind eigene Overlays im App-Design (`ConfirmView`), kein `NSAlert`.
  Es ist immer nur ein Overlay offen; ⌘P schließt ⌘N und umgekehrt.
- **Layout i3-artig statt CSS-Grid (Michaels Wunsch vom 15.09.):** jede Gruppe ist ein frei
  platzierbares Rechteck in Weltkoordinaten (`Group.frame` in `groups.json`), verschiebbar am Kopf,
  größenveränderbar am Griff unten rechts. Die Kacheln füllen die Gruppe als Raster mit einstellbarer
  Spaltenzahl (⌘, → „Spalten pro Gruppe“, Default 2), kein festes 16:10 mehr. Im Fokus wächst die
  Kachel auf das Fensterformat. Neue Gruppen landen rechts neben den bestehenden, sonst darunter.
  Header-Icons (+, Stift, X) sind immer sichtbar; `+` startet eine Session ohne hinzuzoomen.
- **Zwei Zoom-Modi (⌘,):** „Layout“ wie im Brief (Text bildschirmkonstant, Terminal ab 320 px Kachelbreite).
  „Geometrisch“: die Terminalschrift ist 12 pt × Maßstab, die Spaltenzahl bleibt beim Zoomen konstant,
  während der Bewegung wird das eingehängte Terminal nur per Layer skaliert (kein SIGWINCH), in Ruhe
  Frame und Schrift neu gesetzt. Unter 3 px Schrift wird ausgehängt und der Text-Snapshot gezeigt.
- **Claude-Nutzung in der Leiste:** `GET https://api.anthropic.com/api/oauth/usage` mit dem OAuth-Token
  aus dem Schlüsselbund-Eintrag „Claude Code-credentials“ (derselbe Weg wie die CLI), Header
  `anthropic-beta: oauth-2025-04-20`. Gelesen wird das `limits`-Array (kind `session`, `weekly_all`,
  `weekly_scoped` mit `scope.model.display_name` „Fable“), Fallback `five_hour`/`seven_day`. Jeder
  Parse- oder HTTP-Fehler ergibt „–%“ (bzw. behält die letzten Werte), nie ein Absturz. Abfrage alle drei
  Minuten; der Endpunkt antwortet schnell mit HTTP 429, dann verdoppelt sich die Pause bis 15 Minuten.
- **Spaltenzahl der Karte (alt, ersetzt):** mindestens so viele wie in die Fensterbreite passen, bei vielen Gruppen
  mehr, damit die Karte ungefähr das Seitenverhältnis des Fensters hat (`Layout.worldWidth`). Mit
  30 Gruppen in einer Spalte wäre „Fit alles“ ein 5-%-Turm gewesen.

## Build

```bash
cd app && /opt/homebrew/bin/xcodegen generate
xcodebuild -project Kadrell.xcodeproj -scheme Kadrell -configuration Debug -derivedDataPath build \
  -skipPackagePluginValidation -skipMacroValidation build
xcodebuild -project Kadrell.xcodeproj -scheme Kadrell -configuration Debug -derivedDataPath build \
  -skipPackagePluginValidation -skipMacroValidation test
open build/Build/Products/Debug/Kadrell.app
```

`-skipPackagePluginValidation` ist nötig: SwiftTerm bringt den Build-Plugin `SwiftTermBuildInfoPlugin`
mit, den Xcode ohne die Freigabe („Validate plug-in“) nicht ausführt. SwiftTerm ist auf Commit
`233c6ba` gepinnt, weil die Snapshot-API (`terminalStateSnapshot`) dort gelesen wurde.

Screenshots aller Zustände: `~/.claude/screenshots/claude-agent-overview/app/` (Übersicht `13-all-after-esc.png`,
Gruppe `09-focus.png`, Fokus `10-focus-typed.png`, weit weg `06-far.png`, ⌘P `04-palette.png` und
`05-commands.png`, ⌘N `03-new-session.png`).
