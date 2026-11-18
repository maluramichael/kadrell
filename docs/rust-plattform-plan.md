# Kadrell auf Rust: eine Codebasis für macOS und Windows

Stand 18.11.2026. Prüfung, noch keine Entscheidung. Ziel: dasselbe Design wie die Mac-App heute, als echte
native GUI-Anwendung mit eingebetteten Terminals auf beiden Systemen, nicht mehr als TUI im Windows Terminal.
Ein Rust-Kern, möglichst ein Frontend.

## Ausgangslage

| Teil | Umfang |
| --- | --- |
| Mac-App (`app/Kadrell`, Swift 6, AppKit, SwiftUI, SwiftTerm) | 21.302 Zeilen in 113 Dateien |
| davon Kernlogik (portierbar) | ca. 7.860 |
| davon Plattform-Schnittstellen | ca. 1.170 |
| davon Terminal (SwiftTerm) | 378 |
| davon Oberfläche | ca. 11.890 |
| Mac-Tests (`app/KadrellTests`) | 11.441, davon ca. 6.000 als Rust-Kerntests übertragbar |
| Windows (`windows/src`, Rust, ratatui) | 2.728 Zeilen, 26 Tests |

Die Rust-Seite deckt heute nur den Steuerkanal ab: Befehle parsen, `groups.json`/`sessions.json`, Hook, Profil,
PTY, `ls`. Es fehlen Session-Abgleich über `claude agents`, Verlauf und Transcripts, Konten und Nutzung, Layouts,
Auto-Modus, Sync, Schnellstart, Ordnerindex, Worktrees, ssh, Pi, lokale Modelle, Extensions und die Hooks unter
`~/.config/kadrell/hooks/`. Sicherheitslücke nebenbei: Windows glaubt das `caller`-Feld des Clients, der Mac
ermittelt die aufrufende Kachel selbst über die pid am Socket.

Das Design ist fast komplett selbst gezeichnet: Baum, Leiste oben und unten, Kacheln, Paletten-Zeilen, alle Icons
(eigene Striche, keine SF Symbols). Einstellungen und Dialoge sind umgestylte SwiftUI-Controls. Natives AppKit
bleibt nur für Fenster, Menüs, Split-Trenner, Statusitem und die Unschärfe hinter Overlays. Für ein
GPU-gezeichnetes Frontend ist das die beste Ausgangslage: es gibt kaum Systemoptik, die man nachbauen müsste.

## Empfehlung: ein Frontend mit GPUI

GPUI ist das UI-Framework von Zed (Apache-2.0), geprüft im Zed-Repo:

- Mac: Metal, Text über CoreText (`crates/gpui_apple/src/text_system.rs`). Schrift sieht aus wie in nativen Apps.
- Windows: DirectX, Text über DirectWrite (`crates/gpui_windows/src/direct_write.rs`).
- Schlüsselbund, Systembenachrichtigungen, Zwischenablage, Menüs auf beiden Systemen schon vorhanden.
- Zed selbst zeigt große Terminal-Ansichten damit auf Mac und Windows.

Wie GPUI aussieht, bestimmt die App. Kadrell würde mit seinem Theme gezeichnet, nicht im Zed-Stil.

Versionen (crates.io, 18.11.2026):

| Crate | Version | Bemerkung |
| --- | --- | --- |
| `gpui` | 0.2.2 (Okt. 2025) | offiziell, veraltet. Zed empfiehlt bis zu automatischen Releases einen Git-Pin (Issue #66275) |
| `gpui-pre` | 0.3.13 (16.11.2026) | wöchentlicher Stand von Zed main, veröffentlicht vom Autor von gpui-component |
| `gpui-component` | 0.10.0 (16.11.2026) | Tree, Select, Input, Slider, DataTable, Dialog. Baut auf `gpui-pre =0.3.12` |
| `alacritty_terminal` | 0.26.0 (April 2026) | Terminal-Kern, Apache-2.0 |
| `mlua` | 0.12.2 | Lua für Extensions |
| `objc2` | 0.6.5 | Mac-APIs, wo GPUI nichts bietet |
| `uniffi` | 0.32.2 | nur für die Variante mit zwei Frontends |

Verworfen:

| Weg | Grund |
| --- | --- |
| Slint 1.19 | kein Terminal-Widget, eigene Textdarstellung, Lizenz GPL/Royalty-free/kommerziell |
| iced 0.14, egui 0.37 | eigene Textdarstellung, auf dem Mac sichtbar weicher. `iced_term` seit Feb. 2025 ohne Pflege |
| Tauri + xterm.js | Web-Oberfläche statt nativ, Tauri 3 noch Alpha |
| Rust-Kern als eigener Hintergrunddienst | Nicht-Ziel laut Positionierung (kein eigener Daemon). Der Kern bleibt Bibliothek im selben Prozess |

### Alternative: Rust-Kern, zwei Frontends

Mac behält AppKit und SwiftTerm und ruft den Rust-Kern über UniFFI auf, Windows bekommt ein GPUI-Frontend. Vorteil:
der Mac bleibt pixelgleich, kein Risiko bei Tastatur und Terminal auf dem Mac. Nachteil: jede Ansicht (ca. 12.000
Zeilen) entsteht zweimal und muss von Hand gleich gehalten werden. Swift 6 mit strikter Concurrency wird von
UniFFI laut Doku nur teilweise unterstützt (Stand 0.30, für 0.32 nicht verifiziert). Sinnvoll nur, wenn der Spike
unten auf dem Mac scheitert.

## Was zu tun ist

### 1. Terminal (größtes Risiko)

Zeds `terminal` und `terminal_view` stehen unter GPL-3.0 und dürfen nicht ins MIT-Repo. Grundlage wird
`alacritty_terminal`, die Darstellung in GPUI schreiben wir selbst. `vt100` (heute Windows) reicht nicht, es fehlen
Suche und Auswahl im Verlauf. Abgleich mit dem, was Kadrell an SwiftTerm nutzt:

| Kadrell braucht | alacritty_terminal | selbst bauen |
| --- | --- | --- |
| Scrollback, einstellbar | ja | |
| Suche (⌘F, Suche über alle Terminals) | Regex-Suche ja | Suchleiste |
| Auswahl, Kontextmenü | ja | Maus und Menü |
| Bracketed-Paste-Status (für `send`, `capture`) | ja (Terminal-Modus) | |
| sichtbare Zeilen und ganzer Puffer (`capture`, `--all`) | Zugriff auf das Grid | Text-Export |
| DSR- und DA-Antworten | ja (schreibt an die PTY zurück) | |
| Links öffnen | OSC 8 ja | Erkennung und `LinkTarget` (Code existiert) |
| Schrift, Zeilenabstand, Farben, 16 ANSI-Farben, Cursor | | Renderer |
| Sync-Eingabe an alle Kacheln | | Kadrell-Logik, existiert |
| PTY | eigenes tty-Modul (Unix, ConPTY) oder `portable-pty` wie heute | |

### 2. Tastatur (zweites Risiko)

Auf der deutschen Mac-Tastatur erzeugt ⌥ Zeichen (⌥L = @, ⌥N = ~ als Tottaste), Kadrell nutzt aber ⌥N, ⌥P,
⌥1…9 und ⌥-Pfeile als Kürzel. Dazu kommen Tottasten, Eingabemethoden, Umlaute und Einfügen. SwiftTerm erledigt
das heute. Ob GPUI Taste und erzeugtes Zeichen getrennt liefert, wie wir es brauchen, ist nicht verifiziert und
das Hauptkriterium des Spikes.

### 3. Plattform-Schichten (bleiben je System, auch mit einem Frontend)

| Aufgabe | Mac | Windows |
| --- | --- | --- |
| Steuerkanal | Unix-Socket, `getpeereid`, `LOCAL_PEERPID` | TCP + Token (vorhanden), Aufrufer per pid nachrüsten |
| aufrufende Kachel bestimmen | Prozessbaum per `sysctl` | Prozessbaum per Toolhelp/Job Objects |
| Konten-Schlüssel | `/usr/bin/security` behalten (fragt nicht nach) | Credential Manager |
| Benachrichtigungen | GPUI | GPUI |
| Menüleisten-Icon, Dock-Badge | `objc2` (NSStatusItem, dockTile) | Tray-Icon, Taskleisten-Overlay |
| Login-Shell-Umgebung für `claude` | `zsh -lc env -0` | entfällt |
| Prozessbaum beenden | `proc_listchildpids` | Job Objects |
| Ordner beobachten | FSEvents | ReadDirectoryChangesW (`notify` deckt beides ab) |
| CPU, Speicher, Platte für Extensions | `sysinfo` | `sysinfo` |
| Finder-Ordner bei ⌘N | AppleScript | Explorer-Fenster (optional) |
| Einzelinstanz je Profil | Lock-Datei | Lock-Datei (vorhanden) |

Mac-Bundle: Rust erzeugt nur ein Binary. `.app` mit `Info.plist` muss das Build-Skript bauen, sonst fehlen die
Texte für Mikrofon, Apple Events und Benachrichtigungen. Die Mikrofon-Freigabe der `claude`-Kindprozesse hängt
daran. Signieren mit Hardened Runtime und Notarisieren wie heute, nur auf ragasa.

### 4. Kernlogik nach Rust (ca. 7.900 Zeilen Swift)

Vieles steckt heute in den großen Views und muss beim Portieren herausgelöst werden:

- `AppDelegate` (1.462 Zeilen): Startreihenfolge, Session-Abgleich, Status zählen, Auto-Detach, Übernahme von
  `--bg`-Sessions, Sessions starten, Schließ-Regeln, Tempo-Warnung, Palette-Befehle.
- `WorkspaceView` (1.006): Auswahl- und Fokusmodell, Auto-Modus, Sync-Ziele, Zoom, Vorschau, Fenster-Zustand.
- `SidebarView` (721): Zeilenmodell gruppiert/flach, Tastaturnavigation, Verschieben und Sortieren.
- `StatusBarView` (630): Zähler, Nutzungsfarben, Anzeige-Regeln.

Rein portierbar: Group, Session, Control, SessionRegistry, SessionHistory, Transcript, SessionTitle,
QuickStart, FolderIndex, AccountService, UsageService, StatsSeries, Tiling, Hotkeys, Settings-Schema,
AttentionTracker, Extension-Protokoll und Supervisor, lokale Modelle (HTTP), Git, Worktree, SSHConfig, Pi.
Die Swift-Tests dazu (ca. 6.000 Zeilen) werden Rust-Tests im Kern-Crate.

### 5. Lua-Extensions

`mlua` mit Lua 5.5 (Feature prüfen, sonst 5.4 und Kompatibilität testen). `prelude.lua` und `json.lua`
bleiben unverändert. 64-MB-Grenze per `set_memory_limit`, dieselbe Sandbox (kein `os.execute`, `io.popen`,
`package.loadlib`). Helper-Prozess `kadrell ext-host`, Zeitlimits und Neustart-Backoff 1/5/30 s wie heute.
Lua auf eigenem Thread mit großem Stack (heute verlässt sich der Code auf 8 MB Main-Thread-Stack).

### 6. Zweisprachigkeit

`Localizable.xcstrings` (256 KB) in ein Rust-Format überführen (Fluent oder eigene JSON-Tabelle), Umschalten
ohne Neustart. Ein Test, der fehlende Übersetzungen findet, ersetzt `LocalizationTests`.

### 7. Daten und Einstellungen

`sessions.json` und `groups.json` bleiben, wo sie sind. Die Mac-Einstellungen liegen in UserDefaults und
müssen einmalig nach `settings.json` (wie Windows) wandern. Profile und `--profile tmp` gleich wie heute. Der
CLI-Vertrag (`docs/cli-contract.md`) bleibt, die Rust-App spricht ihn auf beiden Systemen. Damit fällt
der doppelte Vertrag später weg.

### 8. Projektregeln anpassen

CLAUDE.md beschreibt Swift-Werkzeug: `lizard` auf Swift, `xcstrings`, `ChildPanel` und
`OverlayFollowsWindowTests`, `xcodebuild`. Für Rust: `cargo clippy -- -D warnings`, `cargo test`, `lizard`
auf `.rs`, Overlay-Regel als GPUI-Test.

## Aufwand

Zeiten für eine Person mit Agenten, Vollzeit, Review inklusive:

| Block | Dauer |
| --- | --- |
| Spike (siehe unten) | 2 bis 4 Tage |
| Kern-Crate: Windows-Code herauslösen, 7.900 Zeilen Swift-Logik, 6.000 Zeilen Tests | 3 bis 5 Wochen |
| Plattform-Schichten Mac und Windows | 1 bis 2 Wochen |
| Terminal-Renderer in GPUI | 2 bis 3 Wochen |
| übrige Oberfläche: Baum, Leisten, Kacheln, 6 Layouts, Palette, Dialoge, Einstellungen, Statistik-Charts, Extension-Panel | 5 bis 8 Wochen |
| Lua-Host auf mlua | 1 Woche |
| Sprachen, Datenumzug, Bundle, Signatur, Installer | 1 bis 2 Wochen |
| **Summe** | **ca. 3,5 bis 5,5 Monate** |

Während der Umstellung bleibt die Swift-App das tägliche Werkzeug. Die Rust-App läuft parallel mit eigenem
Profil und ersetzt sie erst bei gleichem Funktionsumfang.

Laufende Kosten durch GPUI: keine stabilen Releases, wöchentliche API-Änderungen. Version fest pinnen und bewusst
aktualisieren, nicht jeden Stand mitnehmen.

## Erster Schritt: Spike, 2 bis 4 Tage

Ein Fenster mit GPUI: links ein Baum-Mock im Kadrell-Theme, rechts eine Kachel mit `alacritty_terminal`, darin
ein echtes `claude`. Gebaut unter Windows auf diesem Rechner und auf ragasa.

Bestanden, wenn auf beiden Systemen:

1. ⌥N, ⌥1…9 und ⌥-Pfeile als Kürzel greifen, ⌥L trotzdem @ tippt (deutsche Tastatur).
2. Umlaute, Tottasten und Einfügen mehrzeiligen Texts in Claude ankommen.
3. Scrollback, Auswahl und Suche funktionieren.
4. Schnelle Ausgabe (z. B. `cat` einer großen Datei) flüssig bleibt.
5. Die Schrift neben der heutigen App gleich scharf aussieht (Screenshot-Vergleich).

Scheitert 1, 2 oder 5 auf dem Mac: Variante mit zwei Frontends.
