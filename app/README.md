# Kadrell

Native macOS-App (AppKit, Swift 6), die alle laufenden Claude-Code-Sessions zeigt: links ein Baum
Gruppe › Session, rechts die ausgewählten Sessions als Terminals im Grid oder als Stack (i3-Akkordeon).
Reiner Client: Sessions laufen als `claude --bg`, die App pollt `claude agents --json --all` und hängt
sich per SwiftTerm-PTY mit `claude attach <id>` an.

```bash
/opt/homebrew/bin/xcodegen generate
xcodebuild -project Kadrell.xcodeproj -scheme Kadrell -configuration Debug -derivedDataPath build \
  -skipPackagePluginValidation -skipMacroValidation build
open build/Build/Products/Debug/Kadrell.app
```

Bedienung: Klick im Baum zeigt nur diese Session, ⌘-Klick nimmt sie dazu oder weg, ⇧-Klick markiert den
Bereich seit dem letzten Klick, Klick auf eine Gruppe zeigt alle ihre Sessions. Ziehen im Baum oder an der Titelzeile
einer Kachel sortiert um, beide Seiten zeigen dieselbe Reihenfolge (Kachel auf eine fremde Gruppe zieht die ganze Gruppe mit). ⌘1 Grid, ⌘2 Stack,
⌘B blendet den Baum aus, ⌘N neue Session, ⌘⏎ neue Session im Ordner der fokussierten, ⌘P Suche
(`>` Kommandos), ⌘W stoppt die fokussierte Session. F1 zeigt die Hilfe, beim ersten Start automatisch.

Belegbare Kürzel (⌘, Einstellungen, Defaults wie in der tmux-Config): ⌥-Pfeile Fokus, ⌥⇧-Pfeile Kachel
tauschen, ⌥1…⌥9 Kachel direkt, ⌥N/⌥P nächste/vorige, ⌥⇥ zuletzt fokussierte, ⌥Z Zoom (Fokus-Kachel allein,
Badge „ZOOM“ in der Leiste und „Z“ in der Titelzeile), ⌥⏎ Grid ↔ Stack, ⌘Esc Kachel schließen
(Esc selbst geht an Claude Code).

Details, verifizierte CLI-Fakten und Abweichungen vom Brief: `../docs/kadrell-verifikation.md`.
