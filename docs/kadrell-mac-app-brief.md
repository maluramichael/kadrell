# Kadrell: native Mac-App, One-Shot-Auftrag

> **Veraltet:** Dies ist der ursprüngliche Auftrag mit dem zuerst geplanten Konzept (zoombare Karte mit
> semantischem Zoom, Architektur auf `claude --bg`/`claude attach`). Gebaut wurde stattdessen Baum links plus
> Grid/Stack rechts, Claude läuft als eigener Kindprozess statt über `claude --bg`, siehe die Begründung in
> `kadrell-verifikation.md`. Aktueller Stand: `../app/README.md` und `../CHANGELOG.md`. Der hier referenzierte
> Prototyp `prototype/index.html` zeigt die verworfene UI, nicht die gebaute App.

> **Für den ausführenden Agenten:** Dies ist ein vollständiger Auftrag. Lies ihn ganz, dann den Prototyp `prototype/index.html` (Design-Referenz, 540 Zeilen) und die Screenshots unter `~/.claude/screenshots/claude-agent-overview/10-*.png` bis `17-*.png`. Baue die App komplett, ohne Rückfragen, in diesem Repo unter `app/`. Am Ende muss `xcodebuild` durchlaufen, die App starten und die Abnahmekriterien in Abschnitt 8 erfüllen. Verifiziere jeden Punkt real, nicht per Annahme.

**Ziel:** Eine schnelle native macOS-App (AppKit, Swift), die alle laufenden Claude-Code-Sessions als zoombare Karte zeigt: Projektgruppen als Grids aus Session-Kacheln, semantischer Zoom (weit weg nur Farbe und Status, näher Titel, dann die letzten Terminalzeilen, im Fokus ein echtes Terminal zum Lesen und Tippen). Sessions überleben Absturz oder Beenden der App, weil sie außerhalb der App als Claude-Code-Hintergrund-Sessions laufen.

**Architektur:** Die App ist ein reiner Client. Claude Code bringt das Hintergrund-System selbst mit (`claude --bg`, `claude attach`, `claude agents --json`). Die App liest die Session-Liste per Polling, hält pro sichtbarer Session ein Terminal (SwiftTerm, PTY mit `claude attach <id>`), und rendert eine Canvas mit layoutbasiertem Zoom: Kachel-Frames werden für den aktuellen Maßstab berechnet, Text bleibt in Bildschirm-Pixeln scharf, die Menge des Inhalts hängt vom Maßstab ab.

**Tech Stack:** Swift 6.3 (Xcode 26.6 ist installiert), AppKit für Canvas und Kacheln, SwiftUI nur für Dialoge, SwiftTerm per Swift Package Manager, XcodeGen (`/opt/homebrew/bin/xcodegen`) erzeugt das Xcode-Projekt aus `app/project.yml`. macOS 15 als Deployment Target (Michael läuft auf 26.6.2).

**Name und Bundle-ID:** `Kadrell`, `de.malura.kadrell`.

---

## 1. Verifizierte Fakten (Stand 15.09.2026, auf Michaels Mac geprüft)

Nichts davon raten, aber alles vor Benutzung nochmal gegen die installierte Version prüfen (`claude --help`).

**Claude CLI**
- Version 2.1.272. Binary: `/Users/dev/.local/bin/claude` (Symlink auf `~/.local/share/claude/versions/2.1.272`).
- **Falle:** In Michaels Shell ist `claude` ein Alias auf `claude --allow-dangerously-skip-permissions --permission-mode plan`. Die App muss den Binary-Pfad selbst auflösen (`~/.local/bin/claude`, sonst `command -v claude` über `/bin/zsh -lc`) und niemals den Alias erben. Nie `--allow-dangerously-skip-permissions` setzen.
- `claude --bg` / `--background`: startet die Session detached und "prints the id that `claude attach`, `logs`, `stop` and `rm` take". Das genaue stdout-Format ist **nicht verifiziert**. Robuster Weg: vorher eine UUID erzeugen, `claude --bg --session-id <uuid> --name "<Name>" "<erster Prompt>"` im Zielordner starten, danach in `claude agents --json` den Eintrag mit passender `sessionId` suchen und dessen kurze `id` (8 Hex-Zeichen) verwenden.
- `claude --bg --resume <session-id>`: führt eine gestoppte Session im Hintergrund fort.
- `claude attach <id>`: "Open a background session in this terminal". Genau das läuft in der PTY jeder Kachel.
- `claude agents --json [--all] [--cwd <path>]`: gibt ein JSON-Array aus. Ohne `--json` braucht `claude agents` ein TTY und bricht sonst ab. Beobachtete Felder:

```json
{ "id": "0ace1aab", "cwd": "/Users/dev/development/projects/homelab", "kind": "background",
  "startedAt": 1786719895859, "sessionId": "0ace1aab-c1a7-45e2-9856-dea5d34eda81",
  "name": "Weltport für Raspberry Pi Display Dashboard prüfen", "state": "blocked" }
{ "pid": 5395, "cwd": "/Users/dev/development/acme/intern/demoapp", "kind": "interactive",
  "startedAt": 1789040585856, "sessionId": "b746c5bb-…", "name": "demoapp-78", "status": "idle" }
```
  Background-Sessions haben `state` (gesehen: `blocked`, `done`), interaktive haben `status` (gesehen: `idle`) und `pid`. Die vollständigen Enums sind **nicht verifiziert**: unbekannte Werte als `idle` behandeln und ins Log schreiben. `blocked` als "wartet auf Eingabe" interpretieren.
- `claude stop <id>` stoppt, Konversation bleibt. `claude respawn <id>` startet neu. `claude rm <id>` löscht. `claude logs <id>` zeigt den letzten Terminal-Output (nützlich für Kachel-Vorschau ohne Attach).
- `-n, --name <name>` setzt den Anzeigenamen, der in `agents --json` als `name` erscheint.

**SwiftTerm** ([github.com/migueldeicaza/SwiftTerm](https://github.com/migueldeicaza/SwiftTerm)): AppKit-`TerminalView`, dazu `LocalProcessTerminalView`, das eine PTY öffnet und darin einen Prozess startet. Wird in Secure Shellfish, La Terminal und CodeEdit eingesetzt. Exakte Signaturen (`startProcess(executable:args:environment:execName:)`, Buffer-Zugriff über `getTerminal()`) im Package-Quelltext nachschlagen, nicht aus dem Gedächtnis schreiben.

**Toolchain:** Xcode 26.6 (Build 17F113), Swift 6.3.3, xcodegen unter `/opt/homebrew/bin/xcodegen`, kein tuist.

## 2. Referenz: der Prototyp

`prototype/index.html` ist die Design-Vorgabe. Alles dort Sichtbare gilt, außer es steht hier anders. Kernregeln daraus:

- **Palette** Catppuccin Mocha: Hintergrund `#11111b`, Panel `#181825`, Surface `#1e1e2e`, Linie `#313244`, Text `#cdd6f4`, Subtext `#a6adc8`, Muted `#6c7086`. Status: running `#89b4fa` (pulsierender Punkt), waiting `#f9e2af`, idle `#a6e3a1`, error `#f38ba8`, detached `#45475a`. Gruppenfarben aus `#fab387 #cba6f7 #f5c2e7 #89b4fa #a6e3a1 #94e2d5 #f9e2af #eba0ac #b4befe #74c7ec`.
- **Schrift:** überall Monospace (JetBrains Mono, sonst SF Mono/Menlo). Keine Rundungen über 2 px, keine Schatten, kein Glow. Gruppen haben einen 3 px farbigen Balken links und eine 1 px Linie.
- **Layout:** Welt = Grid aus Gruppen (Spalten `auto-fit`, mindestens 720 Welt-Punkte breit), Gruppe = Grid aus Kacheln (mindestens 320 Welt-Punkte breit, Seitenverhältnis 16:10) plus einer gestrichelten `+`-Kachel. Alles ordentlich, Spaltenzahl folgt der Fensterbreite.
- **LOD** nach Kachelbreite auf dem Bildschirm: unter 60 px nur Farbfläche und Status-Punkt; unter 140 px Titel und Gruppenname, Titel auf drei Zeilen geklemmt; unter 320 px Kopfzeile plus so viele der letzten Terminalzeilen, wie hineinpassen; darüber echtes Terminal, von oben gefüllt; im Fokus zusätzlich die Eingabe. Kopfzeile, Rahmen, Icons und Terminalschrift haben immer dieselbe Bildschirmgröße, nur die Menge des Inhalts wächst mit dem Zoom.
- **Statusleiste oben** (30 px): links Gruppen als Chips mit Farbquadrat, Name und Anzahl (Klick zoomt auf die Gruppe, aktive Gruppe unterstrichen), Mitte Breadcrumb `Gruppe › Session` oder `kadrell · n sessions`, rechts Zähler je Status, Buttons `neu ⌘N`, `suche ⌘P`, `fit F`, dann Zoom-Prozent, `attach a/b`, Uhr.
- **Navigation:** Scrollrad und Pinch zoomen immer um den Mauszeiger, Shift+Rad pannt, Drag pannt. Klick auf eine Kachel snappt sie randlos in den Bereich unter der Leiste, die Kachel nimmt dabei das Seitenverhältnis des Fensters an. Klick auf den Gruppen-Header snappt die Gruppe. Esc stufenweise: Terminal → Gruppe → alles. `F` fit alles. `+`/`-` zoomen. Alle Übergänge animiert (~350 ms, ease-out cubic).
- **Gruppen-Header:** `▪ Name`, Pfad gekürzt (`~/dev/…`), Anzahl, beim Hover ein Stift (Name, Farbe, Ordner bearbeiten) und ein X (Gruppe schließen, mit Rückfrage). Kachel-Kopfzeile: Status-Punkt, Titel mit Ellipsis, Laufzeit (`42m`, `2h`, `2d`), beim Hover ein X (Session stoppen und entfernen, mit Rückfrage). Icons als dünne Strichgrafiken, nicht als Systembuttons.
- **⌘P Omni-Leiste:** Fuzzy-Suche über Session-Titel, Gruppen, Pfade, letzte Terminalzeilen; Treffer zeigen Status-Punkt und Gruppe; während des Tippens werden Treffer im Grid hervorgehoben, der Rest gedimmt; Enter zoomt hin. Mit `>` Kommandomodus: Fit alles, Neue Session, Alle anhängen, Session stoppen, Session schließen, Gruppe bearbeiten, Reload, Zoom auf Gruppe X. ⌘P bei offener Leiste wechselt in den `>`-Modus.
- **⌘N Neue Session:** Schritt 1 Gruppe wählen (Liste, Pfeiltasten, tippen filtert, letzter Eintrag "Neue Gruppe …"); bei `+` in einer Gruppe entfällt Schritt 1; ohne Gruppen direkt Schritt 2. Schritt 2 Ordner (vorbelegt mit dem Gruppenordner) plus erster Prompt, ⌘⏎ startet, neue Kachel bekommt sofort den Fokus.
- **Attach-Anzeige:** nicht angehängte Kacheln sind gedimmt mit diagonaler Schraffur und dem Label `NICHT ANGEHÄNGT`.

## 3. Anforderungen

### 3.1 Session-Registry
1. Poll `claude agents --json --all` alle 2 s im Hintergrund, Ergebnis diffen, UI nur bei Änderung aktualisieren.
2. Jede Session bekommt aus `state`/`status` einen Status: `blocked` → waiting, `done` → idle, alles mit "run"/"work"/"active" im Namen → running, "error"/"fail" → error, sonst idle. Unbekannte Werte loggen.
3. Interaktive Sessions (`kind: interactive`, laufen in einem fremden Terminal) werden angezeigt, aber nicht angehängt: Kachel mit Vorschau aus `claude logs <id>` (falls das für interaktive geht, sonst nur Kopfzeile) und dem Label `LÄUFT IN ANDEREM TERMINAL`.
4. Beendete Sessions (`done`) bleiben sichtbar, grün, und lassen sich per Klick weiterführen: die App startet dann `claude --bg --resume <sessionId>` und hängt sich anschließend an.

### 3.2 Gruppen
5. Gruppen sind App-Daten: `id`, `name`, `color`, `cwd`, `sessionIds`. Persistenz als JSON unter `~/Library/Application Support/de.malura.kadrell/groups.json`, atomar geschrieben.
6. Sessions ohne Gruppe werden automatisch der Gruppe mit gleichem `cwd` zugeordnet, sonst wird eine neue Gruppe mit Ordnername und nächster freier Palettenfarbe angelegt.
7. Stift am Header ändert Name, Farbe (nativer `NSColorWell` oder Palette), Ordner für neue Sessions. X schließt die Gruppe: Rückfrage, dann `claude stop` für alle enthaltenen Sessions und Entfernen aus der Datei.

### 3.3 Attach und Terminal
8. Pro angehängter Session genau ein `LocalProcessTerminalView`, das `<claude-binary> attach <id>` in einer PTY startet, Arbeitsverzeichnis = `cwd` der Session, Umgebung = Login-Shell-Umgebung (`/bin/zsh -lc env` einmal beim Start einlesen, damit PATH, HOME, TERM stimmen). `TERM=xterm-256color`.
9. Beim App-Start sind alle Kacheln "nicht angehängt". Eine Attach-Queue hängt gestaffelt an: eine Session alle 500 ms, Reihenfolge nach Abstand zur Viewport-Mitte. Fokus per Klick zieht die Session an die Spitze und hängt sofort an. Die Statusleiste zeigt `attach a/b`.
10. Das Terminal-View ist nur eingehängt, wenn die Kachel auf dem Bildschirm mindestens 320 px breit ist (Gruppen- oder Fokuszoom). Darunter zeigt die Kachel einen Text-Snapshot der letzten Zeilen aus dem Terminal-Buffer (über die SwiftTerm-Buffer-API, alle 500 ms aktualisiert, solange die Session angehängt ist). Während einer Zoom-Animation wird ebenfalls der Snapshot gezeigt und erst am Ende das Terminal eingehängt, damit die PTY nicht bei jedem Frame ein `SIGWINCH` mit neuer Größe bekommt.
11. Tastatureingaben gehen nur an das fokussierte Terminal. Esc verlässt den Fokus, wird also **nicht** an Claude weitergereicht; dafür ist die Eingabezeile von Claude Code selbst zuständig (die App zeichnet keine eigene Prompt-Zeile).
12. Beenden der App oder Absturz darf keine Hintergrund-Session beenden. **Das muss der Agent als erstes real prüfen** (Abschnitt 7, Spike).

### 3.4 Neue Sessions
13. ⌘N, `+`-Kachel oder Kommando: Ordner und erster Prompt wie im Prototyp. Start: UUID erzeugen, `<claude-binary> --bg --session-id <uuid> --name "<Kurzname>" "<Prompt>"` mit `cwd` = Ordner ausführen (kein Alias, keine Permission-Flags). Kurzname = Prompt auf 48 Zeichen gekürzt, ohne Prompt Ordnername plus Zähler. Danach Registry sofort neu pollen, bis die `sessionId` auftaucht (maximal 10 s), Kachel anlegen, Gruppe zuordnen, fokussieren.

### 3.5 Canvas, Zoom, LOD
14. Layoutbasierter Zoom, keine Layer-Skalierung von Text: die Welt hat Koordinaten in Punkten, die Ansicht hat `scale` und `offset`. Für jeden Frame werden Kachel- und Gruppen-Frames in Bildschirmkoordinaten berechnet und gesetzt. Schriftgrößen sind konstant in Bildschirmpunkten. LOD-Schwellen wie in Abschnitt 2.
15. Zoom-Grenzen 3 % bis 800 %. Fit-Funktionen (Kachel, Gruppe, alles) berechnen das Ziel so, dass das Objekt exakt in den Bereich unter der Leiste passt (Kachel 0 px Rand, Gruppe 6 px, alles 12 px).
16. Rendering bleibt bei 60 fps mit 50 Kacheln, davon 6 mit eingehängtem Terminal. Messen mit Instruments oder einem einfachen Frame-Zähler im Debug-Build.
17. Fenstergröße ändert die Spaltenzahl der Grids; danach Fit alles.

### 3.6 Omni-Leiste, Dialoge, Tastatur
18. ⌘P und ⌘N wie im Prototyp, als eigene Fenster-Overlays (NSPanel oder SwiftUI-Sheet), Pfeiltasten und Enter, Esc schließt.
19. Menüleiste mit Standard-App-Menü, `Datei › Neue Session ⌘N`, `Ansicht › Fit alles F`, `Ansicht › Suche ⌘P`, `Session › Stoppen ⌘W` (mit Rückfrage).

## 4. Nicht-Ziele
- Kein eigener Daemon, kein tmux, kein Nachbau der Session-Verwaltung. Alles läuft über die Claude-CLI.
- Keine Minimap, kein Drag-Reorder von Kacheln, keine Themes außer Catppuccin Mocha.
- Kein App-Store-Sandboxing in dieser Version: die App startet fremde Prozesse in PTYs, das verträgt sich nicht mit der App-Sandbox. Ziel ist eine Developer-ID-signierte, notarisierte App außerhalb des Stores. Die Bundle-ID bleibt trotzdem `de.malura.kadrell`.
- Kein SwiftUI für Canvas und Kacheln (zu langsam für viele animierte Frames), nur für Dialoge.

## 5. Projektstruktur

```
app/
  project.yml                 XcodeGen: Target Kadrell (macOS app), Deployment 15.0, SwiftTerm als Package
  Kadrell/
    App/AppDelegate.swift      Fenster, Menü, Lifecycle
    Model/Session.swift        Session-Struct aus agents --json, Status-Mapping
    Model/Group.swift          Gruppe, Persistenz (GroupStore)
    Services/ClaudeCLI.swift   Binary-Auflösung, Login-Env, agents/--bg/stop/resume/logs als async-Funktionen
    Services/SessionRegistry.swift  Polling, Diff, Observable
    Services/AttachManager.swift    Queue, Terminal-Views pro Session, Snapshots
    Canvas/CanvasView.swift    Welt, Zoom, Pan, Hit-Test, Animation (CVDisplayLink oder NSTimer 60 Hz)
    Canvas/Layout.swift        Grid-Layout in Weltkoordinaten (Gruppen, Kacheln, +-Kachel)
    Canvas/GroupView.swift     Header, Balken, Icons
    Canvas/CellView.swift      LOD-Renderer, Snapshot-Text, Terminal-Container
    Bar/StatusBarView.swift    Chips, Breadcrumb, Zähler, Buttons, Uhr
    UI/PaletteWindow.swift     ⌘P
    UI/NewSessionSheet.swift   ⌘N (SwiftUI)
    UI/EditGroupSheet.swift    Stift (SwiftUI)
    Theme.swift                Farben, Schriften, Maße
  KadrellTests/
    SessionParsingTests.swift  JSON aus Abschnitt 1 parsen, Status-Mapping
    LayoutTests.swift          Grid-Layout, Fit-Berechnung, LOD-Schwellen
    GroupStoreTests.swift      Persistenz, Auto-Zuordnung nach cwd
```

## 6. Technische Entscheidungen und Fallstricke
- **Layout-Zoom statt Transform:** Skalierte Layer machen Text unscharf und würden das Terminal mitskalieren. Deshalb Frames pro Frame neu setzen. Für die Animation `NSAnimationContext` mit `allowsImplicitAnimation` oder ein eigener Ticker über `CVDisplayLink`; Ziel ist ein weicher Übergang, nicht die Technik.
- **Chrome-Größen sind bildschirmkonstant, das verschiebt das Layout beim Zoomen.** Im Prototyp war das der Grund für einen 98-px-Versatz beim Fokus. Fit-Ziele daher im Zielmaßstab berechnen (Layout mit dem Ziel-`scale` rechnen, dann Offset bestimmen).
- **Fokus-Kachel** nimmt das Seitenverhältnis des Bereichs unter der Leiste an, damit das Terminal die ganze Fläche nutzt. Die Grid-Zeile darf dabei wachsen; nach Esc gilt wieder 16:10.
- **PTY-Größe:** Größe des Terminal-Views nur am Ende einer Animation setzen (Punkt 10).
- **Prozessumgebung:** Die App wird aus dem Finder gestartet und hat keinen Shell-PATH. Login-Env einmal über `/bin/zsh -lc 'env'` holen.
- **Kein Alias, keine Permission-Flags** (Abschnitt 1).
- **Attach-Client beenden:** Beim Aushängen einer Kachel (oder beim App-Ende) den `attach`-Prozess mit `SIGHUP` beenden, dann prüfen, dass die Session in `agents --json` weiterläuft. Falls das Beenden des Attach-Clients die Session mitreißt, gilt der Fallback: angehängte PTYs bleiben bis zum App-Ende offen und werden nur ausgeblendet, und beim App-Ende werden sie nicht aktiv beendet (der Kernel schließt sie). Welcher Fall gilt, steht am Ende in `docs/kadrell-verifikation.md`.

## 7. Vorgehen für den Agenten

1. **Spike zuerst (30 Minuten):** Kleines Swift-Kommandozeilenprogramm oder Xcode-Test, das (a) `agents --json` parst, (b) eine Test-Session mit `--bg --session-id` in `/tmp/kadrell-spike` startet, (c) `attach` in einer PTY (`forkpty` oder SwiftTerm headless) öffnet, (d) die PTY per `SIGHUP` schließt und (e) in `agents --json` prüft, ob die Session noch läuft. Ergebnis nach `docs/kadrell-verifikation.md` schreiben. Danach die Test-Session mit `claude stop` und `claude rm` aufräumen.
2. `app/project.yml` anlegen, `xcodegen generate`, leeres Fenster bauen und starten (`xcodebuild -project app/Kadrell.xcodeproj -scheme Kadrell -configuration Debug build`).
3. Model, ClaudeCLI, Registry mit Tests (Abschnitt 5). Tests laufen mit `xcodebuild test`.
4. Layout und Canvas mit Fake-Sessions aus dem Prototyp-Seed (`seed()` in `prototype/index.html`), erst Zoom, LOD, Fit, Esc-Stufen, dann Statusleiste.
5. AttachManager und echtes Terminal; mit echten Sessions von Michaels Mac testen (`claude agents --json` liefert derzeit ein Dutzend).
6. ⌘N, ⌘P, Stift, X, Menü.
7. Commit nach jedem abgeschlossenen Schritt, aussagekräftige Messages ohne Co-Author-Zeile, Push auf `master` (privates Projekt, kein PR-Workflow). Repo ist noch nicht initialisiert: `git init`, Default-Branch `master`.
8. Screenshots jedes Zustands nach `~/.claude/screenshots/claude-agent-overview/app/` (nie löschen): Übersicht, Gruppe, Fokus, weit weg, ⌘P, ⌘N.

## 8. Abnahmekriterien

- [ ] `xcodebuild build` und `xcodebuild test` laufen ohne Fehler und ohne Warnungen zu Swift-Concurrency.
- [ ] App startet aus dem Finder (ohne Terminal-Umgebung) und zeigt alle Sessions aus `claude agents --json --all` in Gruppen nach `cwd`.
- [ ] Nach `claude --bg` in einem separaten Terminal erscheint die neue Session innerhalb von 3 s in der App.
- [ ] Rausgezoomt auf 12 % sind nur Farbflächen und Status-Punkte zu sehen, die Gruppennamen bleiben lesbar und brechen nicht um.
- [ ] Klick auf eine Kachel: randloser Fokus unter der Leiste, Terminal zeigt live den Claude-Code-TUI, Tippen kommt in Claude Code an (mit einer Test-Session verifiziert). Esc → Gruppe, Esc → alles.
- [ ] Attach ist gestaffelt: `attach a/b` zählt sichtbar hoch, keine CPU-Spitze über 100 % eines Kerns beim Start mit 12 Sessions.
- [ ] App per `kill -9` beenden, neu starten: alle Sessions sind noch da und lassen sich wieder anhängen.
- [ ] ⌘N legt eine Session in einem gewählten Ordner an, sie erscheint in der richtigen Gruppe und ist fokussiert.
- [ ] Stift ändert Name, Farbe, Ordner; Änderung überlebt Neustart. X mit Rückfrage stoppt und entfernt.
- [ ] ⌘P findet Sessions nach Titel, Pfad und letzten Zeilen; `>fit` führt das Kommando aus.
- [ ] Keine Rundungen über 2 px, keine Schatten, Monospace überall, Catppuccin-Mocha-Farben wie in Abschnitt 2.
- [ ] `docs/kadrell-verifikation.md` dokumentiert Spike-Ergebnis, tatsächliches `--bg`-stdout-Format und die beobachteten `state`/`status`-Werte.

## 9. Offene Punkte, die der Agent selbst klärt
- Exaktes stdout von `claude --bg` (nur relevant, wenn der `--session-id`-Weg nicht funktioniert).
- Verhalten beim Beenden des Attach-Clients (Abschnitt 6, letzter Punkt).
- Ob `claude logs <id>` für interaktive Sessions Output liefert.
- Exakte SwiftTerm-API für Buffer-Zeilen und Prozessstart in der installierten Package-Version.
