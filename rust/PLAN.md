# Kadrell in Rust: Bauplan (Branch `rust-app`)

Ziel: eine Codebasis für macOS und Windows, gleiches Design und gleiches Verhalten wie die Swift-App
(`app/Kadrell`), GPUI-Oberfläche, eingebettete Terminals. Grundlage: `docs/rust-plattform-plan.md` und der bestandene
Spike `spike/gpui`. Nichts davon geht nach `master`, kein Release, keine Version bumpen, bis Michael es freigibt.

## Regeln für alle Agenten

- `CLAUDE.md` und `.claude/rules/zugzug.md` lesen und befolgen: keine Em-Dashes, deutsche Texte mit echten Umlauten,
  nie Claude als Co-Author, nie nach `nul` umleiten, Funktionen klein (CCN ≤ 15).
- Keine simulierten Tasten oder Klicks, kein SetForegroundWindow/SendKeys/SendInput. Screenshots nur per PrintWindow
  auf das eigene Fenster (Vorlage `shot.ps1` im Scratchpad der Planungs-Session, Pfad steht im Auftrag).
- Nur im eigenen Crate bzw. den im Auftrag genannten Pfaden arbeiten. Fremde Crates nur lesen.
- Verhalten aus Swift 1:1 übernehmen, Swift-Tests als Rust-Unit-Tests mitportieren. Die Swift-Datei ist die Quelle der
  Wahrheit für Regeln, Texte, Reihenfolgen und Grenzwerte.
- CLI-Vertrag `docs/cli-contract.md` und `docs/contract/ls.json` gelten unverändert.
- Eigene Kennung `de.malura.kadrell.rust` (kadrell-core `APP_ID`): Standardprofil macOS
  `~/Library/Application Support/de.malura.kadrell.rust`, Windows `%APPDATA%\KadrellRust`, Temp-Profile
  `kadrell-rust-tmp-<pid>`, eigene Schlüsselbund-Dienste und Caches. Nie die Ordner der Swift-App
  (`de.malura.kadrell`) oder der Windows-TUI (`%APPDATA%\Kadrell`) anfassen; ein Datenumzug kommt bewusst später.
- Testen an der laufenden App nur mit `--profile tmp`, nie mit dem Standardprofil. Jeder CLI-Aufruf
  (`kadrell ls`, `new`, …) braucht vorher `KADRELL_SOCKET` auf die Endpunktdatei des Temp-Profils: ohne startet der
  Client sonst eine App im Standardprofil. Prozesse nie per Name beenden (`Stop-Process -Name`,
  `taskkill /IM`, `pkill`), nur die eigene pid: `kadrell.exe` kann auch Michaels Windows-Kadrell sein.
- Vor dem Commit im Workspace `rust/`: `cargo clippy --workspace --all-targets -- -D warnings` und
  `cargo test --workspace` ohne Fehler. Commit nur eigene Pfade (`git add <pfade>`).
- Abhängigkeiten: keine exakten Pins (`=x.y.z`) unter dem Stand in `rust/Cargo.lock` (gpui-pre braucht z. B.
  serde_json ≥ 1.0.144). gpui-pre schaltet `serde_json/preserve_order` für den ganzen Workspace ein: nie auf eine
  sortierte Reihenfolge von `serde_json::Map` verlassen, sortierte Ausgabe ausdrücklich bauen.
- Speicher: jeder Worktree baut ein eigenes `rust/target` (mehrere GB). Die Planungs-Session löscht es, sobald der
  Agent fertig und gemergt ist; Agenten selbst löschen nichts außerhalb ihres Worktrees.
- Was sich auf Windows nicht bauen lässt (macOS-Zweige), sauber hinter `cfg(target_os = "macos")` halten; die Mac-Session
  `gpui-mac` auf ragasa baut und testet es (Inbox `~/tmp/kadrell-spike/inbox/`).

## Workspace `rust/`

```
rust/
  Cargo.toml              Workspace, gemeinsame Abhängigkeiten in [workspace.dependencies]
  rust-toolchain.toml     1.98.1
  crates/
    kadrell-core/         Modell und Speicher: Group, Session, Status, Settings, Profile, Pfade, JSON-Dateien
    kadrell-control/      Steuerkanal: Befehle parsen, Hilfetext, Ziele auflösen, Protokoll, Server/Client
                          (Unix-Socket mit Peer-pid auf macOS, TCP + Token auf Windows), Aufrufer per Prozessbaum,
                          `kadrell hook claude`
    kadrell-claude/       Claude Code: Startargumente (--session-id/--resume/--settings mit Hooks), Login-Umgebung,
                          `claude agents --json`-Abgleich, Status aus Hooks + ReportFallback, Transcripts,
                          Sessiontitel, Verlaufsindex, Übernahme von --bg-Sessions, Worktree, Git, SSH-Config
    kadrell-accounts/     Konten, Nutzung, Tempo-Warnung, Auto-Wechsel, Schlüsselbund/Credential Manager
    kadrell-ext/          Lua-Extensions (mlua): Host-Prozess, Protokoll, Supervisor, Manifest, GitHub-Katalog
    kadrell-term/         Terminal-Kachel aus dem Spike (alacritty_terminal + Renderer + boxdraw + Schrift)
    kadrell-app/          GPUI-Oberfläche aus dem Spike, verdrahtet mit den Crates oben
    kadrell/              Binary: CLI (`kadrell <befehl>`), App-Start, `ext-host`
```

Abhängigkeiten nur nach unten: `kadrell-app` → alle; `control`, `claude`, `accounts`, `ext` → `core`; `term` → nichts
von Kadrell. Kein eigener Daemon: alles läuft im App-Prozess (plus Extension-Helper wie heute).

## Quellen je Crate (Swift → Rust, plus vorhandener Windows-Code in `windows/src`)

| Crate | Swift | Windows-Rust | Tests (Swift) |
| --- | --- | --- | --- |
| core | Model/Group, Model/Session, Model/AgentKind, App/Settings, App/Profile, Services/JSONFile, Services/Stats, App/Hotkeys (Modell) | store.rs, profile.rs | GroupStore, GroupMove, SessionParsing (Modellteil), Profile, SettingsModel, Hotkey, Stats |
| control | Services/Control, Services/ControlSocket, App/AppDelegate+Control, Services/ClaudeHook, Services/Hooks, Services/ProcessRunner (Prozessbaum) | command.rs, protocol.rs, app.rs, hook.rs | Control, ClaudeHook, BackgroundShells |
| claude | Services/ClaudeCLI, SessionRegistry, ReportFallback, Transcript, SessionTitle, SessionHistory, QuickStart, FolderIndex, Worktree, Git, SSHConfig, LinkTarget, AttentionTracker, BackgroundShells, Pi/PiCLI | pty.rs (Start) | SessionParsing, RegistryReport, ReportFallback, TranscriptCount, SessionTitle, SessionHistory, QuickStart, FolderIndex, Worktree, Git, SSHConfig, LinkTarget, AttentionTracker, PiSession, ClaudeCLIVersion |
| accounts | AccountService, AccountToken, UsageService, UsageHistory, StatsSeries, Keychain | | AccountService, AccountToken, UsageBackoff, StatsSeries |
| ext | Extensions/* (ohne UI), Host/prelude.lua, Host/json.lua, LuaShim | | ExtHost, ExtensionManager, ExtensionProcess, ExtensionSupervisor, GitHubExtensions, SystemProbe |
| term | spike/gpui/src/terminal.rs, boxdraw.rs, fonts.rs, assets/fonts | | Scrollback, SyncInput |
| app | spike/gpui/src/ui/*, menus.rs, main.rs; Swift UI/*, Sidebar/*, Bar/*, Workspace/* als Vorlage | ui.rs, input.rs | Tiling, Tile, SidebarRenderer, SidebarFlat, StatusDisplay, ListFilter, NewSessionModel |

## Wellen

1. **Welle 0 (Gerüst):** Workspace anlegen, `kadrell-core` portieren; parallel den Spike nach `kadrell-term` und
   `kadrell-app` umziehen (App läuft danach unverändert, nur neue Struktur).
2. **Welle 1 (Kern parallel):** `control`, `claude`, `accounts`, `ext` gleichzeitig, je ein Agent.
3. **Welle 2 (Verdrahtung):** App auf echte Sessions umstellen (Mock raus), Steuerkanal und CLI im Binary, Hooks,
   Status, Profile; Einstellungen-Dialog, Suche ⌘P, Verlaufssuche, Schnellstart, Statistik, Extensions-Panel.
4. **Welle 3 (Plattform und Feinschliff):** macOS-Teile auf ragasa (Mac-Session), .app-Bundle, Menüleisten-Icon,
   Benachrichtigungen, Sprachwechsel zur Laufzeit (Fluent, Texte aus `Localizable.xcstrings`), Pixelvergleich mit der
   Swift-App.

Fortschritt und Abweichungen trägt die Planungs-Session hier unten ein.

## Fortschritt

- Welle 0 fertig: Workspace, `kadrell-core` (113 Tests), Spike nach `kadrell-term`/`kadrell-app`/`kadrell`.
- Welle 1 fertig: `kadrell-accounts`, `kadrell-claude`, `kadrell-control`, `kadrell-ext` (Lua 5.5 über mlua) gemergt.
- Welle 2: Suche (⌘P) und Einstellungen-Dialog gemergt (704 Tests, a6c69e7). Laufen: Verdrahtung mit echten Sessions
  (`rw-wire`), `kadrell-i18n` (Fluent aus Localizable.xcstrings), `kadrell-tray` (Tray, Benachrichtigungen, Badge).
  Mac-Session: `.app`-Bündel, danach Unix-Teile von control/accounts/claude.
- Welle 2 fertig (f20afab, 880 Tests): echte Sessions + Steuerkanal + CLI, Einstellungen, Suche, Verlauf,
  Schnellstart, Sprachen live, Tray/Fenster, Nutzung/Konten/Statistik, Extensions-Oberfläche, Mac: Bündel,
  Unix-Steuerkanal, Signale, Schlüsselbund. Eigene Kennung `de.malura.kadrell.rust`.
- Welle 3 läuft: A Arbeitsfläche (Zoom, Sync, Vorschau, Frei/Scrollen/Reihe, Kürzel aus Settings), B Gruppen
  (Dialoge, Kontextmenüs, Drag), C lokale Modelle (`kadrell-models`), Pi, Remote, D Fenster/Darstellung,
  E Hilfe/About/Übernahme/Worktrees/Links, F wackelnde Tests. Mac-Session: ganze App als Bündel prüfen.
- Feinschliff-Liste: Einstellungstexte zeigen „⌘N“ auch unter Windows; unterste Listenzeile im Dialog „Neue Session“
  angeschnitten; Fensterfokus beim Start auf dem Mac; Layout-Menüeinträge auf dem Mac ausgegraut; Buchstabenabstand
  der Gruppennamen; Blur hinter Dialogen; Konten/lokale Modelle im Einstellungen-Dialog noch Mock.
