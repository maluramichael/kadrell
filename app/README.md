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

Bedienung: Klick im Baum zeigt nur diese Session, ⌘-Klick nimmt sie dazu oder weg, ⇧-Klick markiert den
Bereich seit dem letzten Klick, Klick auf eine Gruppe zeigt alle ihre Sessions. Ziehen im Baum oder an der Titelzeile
einer Kachel sortiert um, beide Seiten zeigen dieselbe Reihenfolge (Kachel auf eine fremde Gruppe zieht die ganze Gruppe mit).
⌘B blendet den Baum aus, ⌘N neue Session, ⌘⏎ neue Session im Ordner der fokussierten, ⌘P Suche
(`>` Kommandos), ⌘W schließt das Fenster wie der rote Knopf, Sessions laufen weiter. F1 zeigt die Hilfe, beim ersten Start automatisch.

Belegbare Kürzel (⌘, Einstellungen, Defaults wie in der tmux-Config): ⌥-Pfeile Fokus, ⌥⇧-Pfeile Kachel
tauschen, ⌥1…⌥9 Kachel direkt, ⌥N/⌥P nächste/vorige, ⌥J/⌥K Vorschau: blättert durch den Baum und zeigt die Session allein, ohne die Auswahl anzufassen (⏎ übernimmt, Esc zurück), ⌥⇥ zuletzt fokussierte, ⌥Z Zoom (Fokus-Kachel allein,
Badge „ZOOM“ in der Leiste und „Z“ in der Titelzeile), ⌥E Ordner der Fokus-Session im externen Editor (Kommando in den Einstellungen, z. B. `code`),
F2 oder Stift an Kachel/Baum-Zeile benennt die Session um (eigener Name geht immer vor dem Titel von Claude Code, leer = zurück),
⌘L Grid ↔ Stack, ⌘Esc Kachel schließen
(Esc selbst geht an Claude Code). ⌘1 gibt dem Baum die Tastatur, ↑↓ zeigt dann die nächste Session rechts, ohne
dass Claude die Tasten bekommt; ⌘2 gibt die Tastatur an die fokussierte Kachel zurück (ohne offene Kachel: erste Session).

Darstellung (⌘, Einstellungen): UI-Größe 90 bis 130 % für Baum, Leiste, Kacheln und Dialoge, Terminal-Schrift
(alle installierten Monospace-Fonts), Schriftgröße, Zeilenabstand und Innenabstand der Terminals.
⌘+ / ⌘- / ⌘0 ändern die Terminal-Schriftgröße direkt, für alle Terminals.

Hooks (wie tmux `set-hook`): ausführbare Skripte unter `~/.config/kadrell/hooks/`, Name = Ereignis.
`session-new` (Session angelegt oder übernommen), `session-focus` (andere Session fokussiert, auch beim Start),
`session-remove` (Session entfernt). Aufruf im Session-Ordner ohne Warten mit `$1` Ordner, `$2` sessionId von
Claude Code, `$3` Titel; Umgebung wie Claude (Login-Shell) plus `KADRELL_EVENT`, `KADRELL_CWD`,
`KADRELL_SESSION_ID`, `KADRELL_SESSION_KEY`, `KADRELL_TITLE`, `KADRELL_BRANCH`. Fehlt das Skript, passiert nichts.

Details, verifizierte CLI-Fakten und Abweichungen vom Brief: `../docs/kadrell-verifikation.md`.
