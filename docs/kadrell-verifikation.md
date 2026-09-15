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
