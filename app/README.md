# Kadrell

Native macOS-App (AppKit, Swift 6), die alle laufenden Claude-Code-Sessions als zoombare Karte zeigt.
Reiner Client: Sessions laufen als `claude --bg`, die App pollt `claude agents --json --all` und hängt
sich per SwiftTerm-PTY mit `claude attach <id>` an.

```bash
/opt/homebrew/bin/xcodegen generate
xcodebuild -project Kadrell.xcodeproj -scheme Kadrell -configuration Debug -derivedDataPath build \
  -skipPackagePluginValidation -skipMacroValidation build
open build/Build/Products/Debug/Kadrell.app
```

Tasten: ⌘N neue Session, ⌘P Suche (`>` Kommandos), F fit alles, +/- zoomen, Klick auf Kachel fokussiert,
⌘Esc verlässt Terminal → Gruppe → alles (Esc selbst geht an Claude Code), ⌘W stoppt die fokussierte Session.

Details, verifizierte CLI-Fakten und Abweichungen vom Brief: `../docs/kadrell-verifikation.md`.
