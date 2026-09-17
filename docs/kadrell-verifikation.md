# Kadrell: Verifikation (Stand 16.09.2026)

Alles hier wurde real auf Michaels Mac geprüft (Claude CLI 2.1.272/2.1.273, Xcode 26.6, Swift 6.3.3).

## Ab 1.0.0: kein `--bg` mehr, Kindprozess plus `--resume` (16.09.2026, 2.1.273)

Grund: `claude --bg` gibt Claude im Systemprompt einen Abschnitt „Background Session“ mit, der vor jeder
Dateiänderung `EnterWorktree` erzwingt. Geprüft per Python-`pty.fork` in einer sauberen Umgebung:

- `claude --session-id <uuid>` (interaktiv) erscheint sofort in `claude agents --json --all`: `kind: interactive`,
  `pid`, `sessionId` wie vorgegeben, `status` (`idle`/`busy`/`waiting`), Name `<ordner>-<2 hex>`.
- Frage „Enthält dein Systemprompt einen Abschnitt Background Session?“ → **NEIN**. Kein Worktree-Zwang.
- `SIGHUP` beendet den Prozess (Exit 0), der Eintrag verschwindet aus `agents`.
- `claude --resume <uuid>` setzt mit Verlauf fort, **gleiche sessionId**. Der Auto-Name ändert sich dabei
  (`-ad` → `-c6`), deshalb speichert Kadrell nur Namen, die nicht diesem Muster folgen.
- Ohne erste Nachricht gibt es kein Transcript: `--resume` scheitert mit „No conversation found with session ID“,
  Exit 1. Kadrell startet dann mit `--session-id <dieselbe uuid>`.
- **Aus einer Claude-Session heraus gestartet** erbt der Prozess `CLAUDE_CODE_CHILD_SESSION` usw.: „Transcript saving
  is off … inherited CLAUDE_CODE_CHILD_SESSION marker“, die Session fehlt in `agents`. Kadrell filtert diese
  Variablen aus der Umgebung.
- Übernahme einer laufenden `--bg`-Session: `claude stop <id>` beendet den Prozess (pid weg, Eintrag `state: done`
  ohne pid), danach `claude --resume <sessionId>` interaktiv mit Verlauf, Name und Modell. Der gestoppte
  Hintergrund-Eintrag bleibt mit derselben sessionId in `agents --all` stehen, deshalb ordnet Kadrell Live-Werte nur
  über die pid des eigenen Prozesses zu. `claude rm <id>` lässt das Transcript stehen, löscht laut Hilfe aber ggf.
  den Worktree der Session; Kadrell ruft es nicht auf.
- Interaktiv fragt Claude in einem noch nicht vertrauten Ordner erst „Is this a project you trust?“ (mit `--bg`
  nicht). Die Frage erscheint im Terminal der Kachel.

## Hooks per `--settings` (17.09.2026, 2.1.274)

- `claude --settings '{"hooks":{…}}' -p …` lädt die Hooks aus dem JSON-String, obwohl die Hooks-Doku nur
  `settings.json` nennt: `UserPromptSubmit` und `Stop` feuerten, stdin-JSON mit `session_id`, `transcript_path`,
  `cwd`, `hook_event_name`, `prompt_id`, `permission_mode`; `Stop` zusätzlich `last_assistant_message`.
  Darauf baut `ClaudeHook.launchArgs`: kein Eintrag in `~/.claude/settings.json` nötig.
- Der Hook-Prozess erbt die Umgebung des claude-Prozesses samt eigenen Variablen (`KADRELL_*`) und `CLAUDE_PID`;
  `~/.claude/sessions/$CLAUDE_PID.json` hat die Felder `name`, `nameSource` (`derived` = automatischer Name), `status`.

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
  `backgrounded · 86f99758 · kadrell-spike (idle, send a prompt to start)`-Zeile.
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
- **`done` heißt nicht „beendet“** (15.09.2026, 2.1.273): nach der ersten Antwort springt `state` von
  `working` auf `done`, `status: idle`, gleiche `pid`, und `claude attach` funktioniert weiter. Beendet ist
  nur `stopped` oder `done` ohne `pid`. Vorher hat Kadrell solche Sessions abgehängt und „Fortsetzen“
  angeboten, das per `--bg --resume` eine Kopie mit gleichem Namen startete.
- **Tippen kommt an:** Fokus auf eine angehängte Session, Text getippt, Enter, Claude Code antwortet im
  Terminal der Kachel (Screenshots `10-focus-typed.png`, `11-focus-answer.png`).
- **`claude --bg` im fremden Terminal** erscheint beim nächsten 2-s-Poll in der App, Gruppe nach `cwd`.

## Abweichungen vom Brief (mit Michael am 15.09. abgestimmt)

- **Esc geht an Claude Code**, nicht an die App: Esc ist in Claude Code der Interrupt. Die Fokus-Kachel
  schließt man mit **⌘Esc** (im Zen-Modus erst zurück ins Layout).
- **Nur Hintergrund-Sessions.** Interaktive Sessions (`kind: interactive`, laufen in iTerm/tmux) werden
  nicht angezeigt; die Registry filtert sie weg. Punkt 3 des Briefs entfällt damit.
- **Leere Gruppen verschwinden** beim nächsten Abgleich von selbst; das X an einer leeren Gruppe fragt
  nicht nach. Rückfragen bestätigt plain ⏎, Esc bricht ab.
- **Keine Gruppen-Chips in der Statusleiste** (bei 30 Gruppen zu voll). Links steht nur der Breadcrumb.
- **Rückfragen und Fehler** sind eigene Overlays im App-Design (`ConfirmView`), kein `NSAlert`.
  Es ist immer nur ein Overlay offen; ⌘P schließt ⌘N und umgekehrt.
- **Baum plus Tiling statt Karte (Michaels Wunsch vom 15.09., abends):** die zoombare Karte mit frei
  platzierbaren Gruppen, Raster, Snap und LOD ist raus (`Canvas/`, `Group.frame`, Zoom-Modi, Spalten).
  Links steht ein Baum Gruppe › Session (`Sidebar/SidebarView.swift`, handgezeichnet wie die Leiste),
  rechts die ausgewählten Sessions (`Workspace/WorkspaceView.swift`) als Grid (ceil(√n) Spalten) oder
  Stack im i3-Akkordeon: jede Kachel eine Titelzeile, die aktive klappt an ihrer Stelle auf, die Zeilen
  danach bleiben darunter. Die Mathe dazu ist `Workspace/Tiling.swift` (`TilingTests`). Auswahl und
  Layout liegen in UserDefaults (`workspace.selected`, `workspace.mode`). Terminals werden nur in sichtbare
  Kacheln eingehängt, angehängt werden weiterhin alle (sichtbare zuerst), damit ⌘P in den Zeilen suchen kann.
  Icons im Baum (+, Stift, X) erscheinen nur bei Hover, Laufzeiten stehen rechtsbündig in einer Flucht.
  Der Layout-Umschalter in der Leiste ist ein einzelnes Icon ohne Text, F1 beim ersten Start zeigt alles.
  Fokusbewegung liegt auf ⌘⌥-Pfeilen, weil ⌥-Pfeile in Claude Code Wortsprünge sind.
- **Claude-Nutzung in der Leiste:** `GET https://api.anthropic.com/api/oauth/usage` mit dem OAuth-Token
  aus dem Schlüsselbund-Eintrag „Claude Code-credentials“ (derselbe Weg wie die CLI), Header
  `anthropic-beta: oauth-2025-04-20`. Gelesen wird das `limits`-Array (kind `session`, `weekly_all`,
  `weekly_scoped` mit `scope.model.display_name` „Fable“), Fallback `five_hour`/`seven_day`. Jeder
  Parse- oder HTTP-Fehler ergibt „–%“ (bzw. behält die letzten Werte), nie ein Absturz. Abfrage alle drei
  Minuten; der Endpunkt antwortet schnell mit HTTP 429, dann verdoppelt sich die Pause bis 15 Minuten.
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
