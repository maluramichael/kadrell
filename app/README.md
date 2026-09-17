# Kadrell

Native macOS-App (AppKit, Swift 6) für Claude-Code-Sessions: links ein Baum Gruppe › Session, rechts die
ausgewählten Sessions als Terminals im Grid oder als Stack (i3-Akkordeon).
Kein `claude --bg`: jede Kachel startet Claude selbst als Kindprozess in einer SwiftTerm-PTY, neue Sessions mit
`claude --session-id <uuid>`, bekannte mit `claude --resume <sessionId>`. Die Liste liegt in
`~/Library/Application Support/de.malura.kadrell/sessions.json`. Beim Beenden von Kadrell enden die Prozesse,
beim nächsten Start setzt `--resume` die angezeigten fort; ausgeblendete starten erst beim Anklicken. Status, Titel
und aktuelle sessionId pollt die App über `claude agents --json --all`, zugeordnet über die pid des eigenen Prozesses.
Laufende `claude --bg`-Sessions bietet sie beim Start zur Übernahme an (`claude stop`, dann `--resume`).

```bash
/opt/homebrew/bin/xcodegen generate
xcodebuild -project Kadrell.xcodeproj -scheme Kadrell -configuration Debug -derivedDataPath build \
  -skipPackagePluginValidation -skipMacroValidation build
open build/Build/Products/Debug/Kadrell.app
```

Profile: `open -n Kadrell.app --args --profile <name>` startet eine weitere Instanz mit eigenen Sessions, Gruppen,
Einstellungen und eigenem Socket (`~/Library/Application Support/de.malura.kadrell/profiles/<name>/`). Ohne Namen
gilt das Standardprofil am bisherigen Ort. `--profile tmp` ist ein Wegwerfprofil im Temp-Ordner, das beim Beenden
verschwindet (auch im Menü: „Neue Instanz mit temporärem Profil“). Jedes Profil läuft höchstens einmal.

Layouts (Symbol links in der Leiste, ⌘L reihum): Grid, Haupt + Spalte, Spirale (bspwm), Stack. Jedes Layout ist eine
Vorlage aus Feldern, die Terminals füllen sie in Baumreihenfolge. Trennlinien lassen sich ziehen (Doppelklick verteilt
gleich) oder per ⌃⌥-Pfeil um 5 % verschieben, die Verhältnisse gehören der Vorlage, nicht den Sessions. Im Grid legt
‹ SP › in der Leiste die Spaltenzahl fest.

Bedienung: Klick im Baum zeigt nur diese Session, ⌘-Klick nimmt sie dazu oder weg, ⇧-Klick markiert den
Bereich seit dem letzten Klick, Klick auf eine Gruppe zeigt alle ihre Sessions. Ziehen im Baum oder an der Titelzeile
einer Kachel sortiert um, beide Seiten zeigen dieselbe Reihenfolge (Kachel auf eine fremde Gruppe zieht die ganze Gruppe mit).
⌘B blendet den Baum aus, ⌘N neue Session (Suchfeld über Gruppen, benutzte Ordner und Git-Repos unter dem Projektordner; `~/d/p/kad` kürzt Pfade ab, ⌘O Finder, Ordner aus dem Finder in Dialog oder Baum ziehen), ⌘⏎ neue Session im Ordner der fokussierten, ⌘T Terminal ohne Claude im selben Ordner, ⌘P Suche
(`>` Kommandos, `/` Text in allen laufenden Terminals, direkt per ⌘⇧F), ⌘F Suchleiste im fokussierten Terminal (⌘G / ⌘⇧G weiter), ⌘W beendet die fokussierte Session. Der rote Knopf versteckt nur das Fenster, Kadrell und alle Sessions laufen weiter; das Menüleisten-Icon (Kurzstatus wartend/arbeitend) oder ein Klick aufs Dock-Icon holen es zurück. F1 zeigt die Hilfe, beim ersten Start automatisch.

Belegbare Kürzel (⌘, Einstellungen, Defaults wie in der tmux-Config): ⌥-Pfeile Fokus, ⌥⇧-Pfeile Kachel
tauschen, ⌥1…⌥9 Kachel direkt, ⌥N/⌥P nächste/vorige, ⌥J/⌥K Vorschau: blättert durch den Baum und zeigt die Session allein, ohne die Auswahl anzufassen (⏎ übernimmt, Esc zurück), ⌥⇥ zuletzt fokussierte, ⌥I Sync (Tippen und ⌘V gehen an alle offenen Kacheln, rotes Badge „SYNC“ in der Leiste, auch per Klick), ⌥Z Zoom (Fokus-Kachel allein,
Badge „ZOOM“ in der Leiste und „Z“ in der Titelzeile), ⌥E Ordner der Fokus-Session im externen Editor (Kommando in den Einstellungen, z. B. `code`),
F2 oder Stift an Kachel/Baum-Zeile benennt die Session um (eigener Name geht immer vor dem Titel von Claude Code, leer = zurück),
⌘L Grid ↔ Stack, ⌘Esc Kachel schließen
(Esc selbst geht an Claude Code). ⌘1 gibt dem Baum die Tastatur, ↑↓ zeigt dann die nächste Session rechts, ohne
dass Claude die Tasten bekommt; ⌘2 gibt die Tastatur an die fokussierte Kachel zurück (ohne offene Kachel: erste Session).

Darstellung (⌘, Einstellungen): Design des Baums (Klassisch, Getönte Gruppen, Kompakt; je ein `SidebarRenderer`
unter `Kadrell/Sidebar/`), Laufzeit im Baum an/aus, UI-Größe 90 bis 130 % für Baum, Leiste, Kacheln und Dialoge, Terminal-Schrift
(alle installierten Monospace-Fonts), Schriftgröße, Zeilenabstand und Innenabstand der Terminals.
⌘+ / ⌘- / ⌘0 ändern die Terminal-Schriftgröße direkt, für alle Terminals.

Hooks (wie tmux `set-hook`): ausführbare Skripte unter `~/.config/kadrell/hooks/`, Name = Ereignis.
`session-new` (Session angelegt oder übernommen), `session-focus` (andere Session fokussiert, auch beim Start),
`session-remove` (Session entfernt). Aufruf im Session-Ordner ohne Warten mit `$1` Ordner, `$2` sessionId von
Claude Code, `$3` Titel; Umgebung wie Claude (Login-Shell) plus `KADRELL_EVENT`, `KADRELL_CWD`,
`KADRELL_SESSION_ID`, `KADRELL_SESSION_KEY`, `KADRELL_TITLE`, `KADRELL_BRANCH`. Fehlt das Skript, passiert nichts.

Fernsteuerung (wie das `tmux`-Kommando): Menü Kadrell › „Kommandozeilen-Tool installieren …“ legt
`~/.local/bin/kadrell` als Symlink auf das App-Binary an. `kadrell <befehl>` spricht über den Unix-Socket
`~/Library/Application Support/de.malura.kadrell/kadrell.sock` (0600) mit der laufenden App und startet sie, falls
sie nicht läuft. Befehle: `ls [--json]`, `new-group`, `new [-t gruppe] [-c ordner] [-d] [prompt]`, `select`, `layout`,
`zoom`, `rename`, `move`, `set-group`, `stop`, `resume`, `kill`, `kill-group`, `send [-k]`, `capture [--all]`, alles in
`kadrell help`. Ziele per `-t` (Key-Anfang, Titel, Gruppenname); ohne `-t` gilt die Session, in der das Kommando
läuft: jede Kachel hat `KADRELL_SESSION_KEY`, `KADRELL_SOCKET` und `KADRELL` (Pfad zum Binary) in der Umgebung.
Per CLI angelegte Gruppen sind Favoriten, sonst räumt der Abgleich sie leer wieder weg. Rückfragen entfallen.

tmux-Sessions importieren: `tools/tmux-dump.py dump -o dump.json` sammelt alle laufenden Claude-Sessions aus
tmux-Panes (Baum Session/Window/Pane) als JSON, `tools/tmux-dump.py import dump.json` startet sie per
`claude --bg --resume` als Hintergrund-Sessions (läuft die Original-Session noch, macht das eine Kopie statt sie
anzufassen) und legt je tmux-Session eine Gruppe in `groups.json` an. Kadrell muss dabei beendet sein, sonst
überschreibt der Import dessen `groups.json` unter der laufenden App weg. `--dry-run` zeigt nur, was passieren würde.
Der Leerzustand zählt separat, wie viele Claude-Sessions gerade interaktiv in anderen Terminals laufen, und
verweist auf dieses Skript.

Details, verifizierte CLI-Fakten und Abweichungen vom Brief: `../docs/kadrell-verifikation.md`.
