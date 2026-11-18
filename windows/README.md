# Kadrell für Windows

Vorabversion, noch nicht zum Download. Ein Programm `kadrell.exe` in Rust, das alle Claude-Code-Sessions in
einem Windows-Terminal-Fenster hält, tmux-artig: links der Baum Gruppe › Session mit Statuspunkt, rechts die
fokussierte Session als Terminal. Dasselbe Binary ist auch die Kommandozeile und spricht dieselben Befehle wie die
Mac-App (`kadrell ls --json`, `new`, `send`, `capture` …). Damit laufen Skripte wie start-my-day auf beiden
Systemen. Gemeinsam mit der Mac-App ist nur der Vertrag (`docs/cli-contract.md`), kein Code.

```powershell
cd windows
cargo build --release              # target\release\kadrell.exe
target\release\kadrell.exe         # Fenster im aktuellen Terminal öffnen (Standardprofil)
kadrell ls --json                  # von überall: an die laufende Instanz, startet sie sonst in einem wt-Fenster
```

Aufbau (`src/`):

- `command.rs`: Befehle parsen, Hilfetext, Tastennamen für `send -k`. Gleiche Flags und Meldungen wie
  `app/Kadrell/Services/Control.swift`.
- `protocol.rs`: Steuerkanal. Eine Zeile JSON hin, eine zurück, Felder wie auf dem Mac. Statt AF_UNIX TCP auf
  127.0.0.1 mit zufälligem Port, Port und Token stehen in `kadrell.endpoint` im Profilordner. `KADRELL_SOCKET`
  zeigt in jeder Kachel auf diese Datei.
- `app.rs`: Zustand und Ausführung der Befehle, `ls` im Mac-Format.
- `pty.rs`: je Session eine ConPTY (portable-pty) und ein VT-Parser (vt100), der DSR- und DA-Anfragen beantwortet.
- `hook.rs`: `kadrell hook claude` und die `--settings`-Hooks für jeden Claude-Prozess, wie `ClaudeHook.swift`.
  Claude Code führt die Hook-Zeile unter Windows in Git Bash aus, mit Claude Code 2.1.327 geprüft.
- `store.rs`: Gruppen und Sessions (`groups.json`, `sessions.json`), Ziele auflösen (`-t`), Pfade.
- `profile.rs`: Profile und `settings.json`.
- `ui.rs`, `input.rs`: Fenster (ratatui) und Tasten.

Bedienung: Präfix Strg+B wie tmux, danach `n`/`p` nächste/vorige, `1`…`9` direkt, `c` neue Session im Ordner der
fokussierten, `x` beenden, `s`/`r` stoppen/neu starten, `,` umbenennen, `w` Tastatur in den Baum, `b` Baum ein/aus,
`q` beenden, `?` Hilfe, Strg+B Strg+B schickt ein Strg+B ans Terminal. Ohne Präfix Alt+N/Alt+P und Alt+1…9 wie die
Standardkürzel der Mac-App. Beim Beenden enden alle Claude-Prozesse. Beim nächsten Start setzt `--resume` die
fokussierte Session fort, die anderen starten beim Fokussieren.

Profile: ohne Angabe `%APPDATA%\Kadrell`, `--profile <name>` unter `profiles\<name>`, `--profile tmp` ein
Wegwerfprofil in `%TEMP%\kadrell-tmp-<pid>`. Es übernimmt die Einstellungen des Standardprofils und verschwindet
beim Beenden. Reste nach einem harten Ende räumt der nächste Start eines Temp-Profils weg.

Einstellungen (`settings.json` im Profilordner, von Hand, bis es einen Dialog gibt):

```json
{
  "claude": { "allowBypass": false, "mode": "", "model": "", "effort": "", "fullscreen": true },
  "controlOtherSessions": true,
  "claudeBinary": ""
}
```

`mode` ist `--permission-mode` (z. B. `bypassPermissions` zusammen mit `allowBypass`), leere Werte lassen das Flag
weg. `claudeBinary` leer sucht `claude` über PATH.

Testen an der laufenden App: wie auf dem Mac nie das Standardprofil, immer ein Temp-Profil im eigenen Fenster:

```bash
wt.exe -w kadrell-test new-tab --title KadrellTest "$(pwd -W)/target/debug/kadrell.exe" --profile tmp
export KADRELL_SOCKET="$(cygpath -w "$TEMP")\\kadrell-tmp-<pid>\\kadrell.endpoint"
target/debug/kadrell.exe ls --json
target/debug/kadrell.exe kill-server      # nur diese Instanz beenden (die pid steht im Ordnernamen)
```

Vor dem Commit: `cargo clippy --all-targets -- -D warnings` und `cargo test` ohne Fehler (wie die Regeln für die
Mac-App: keine Warnungen). Die Versionsnummer in `Cargo.toml` setzt `tools/bump-version.py` mit.

Noch nicht da: Grid und weitere Layouts, Maus, Verlauf zurückscrollen, Benachrichtigungen, Einstellungsdialog,
Lua-Extensions, Pi- und ssh-Kacheln, Terminal ohne Claude. Einfügen per Strg+V aus Windows Terminal ist noch
nicht geprüft. Kommt der Text als einzelne Tasten an statt als Einfügen, schickt mehrzeiliger Text jede Zeile
einzeln ab.
