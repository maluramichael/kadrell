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
Bereich seit dem letzten Klick, Klick auf eine Gruppe zeigt alle ihre Sessions. ⌘1 Grid, ⌘2 Stack, ⌘⌥-Pfeile bewegen den Fokus, ⌘⇧⏎ zeigt die Fokus-Kachel allein,
⌘Esc schließt sie (Esc selbst geht an Claude Code), ⌘B blendet den Baum aus, ⌘N neue Session, ⌘⏎ neue Session in der Gruppe der Fokus-Kachel, ⌘P Suche
(`>` Kommandos), ⌘W stoppt die fokussierte Session. F1 zeigt die Hilfe, beim ersten Start automatisch.

Details, verifizierte CLI-Fakten und Abweichungen vom Brief: `../docs/kadrell-verifikation.md`.
