# Plan: weitere Agent-CLIs als Session-Typ

Stand 2026-09-17. Recherche und Entscheidungen aus der Planungssession; Phase 1 (Claude-Hooks statt Polling,
`kadrell status`, `kadrell hook claude`) ist umgesetzt. Alles Weitere hier ist noch nicht gebaut.

## Entscheidungen

- Kadrell startet wahlweise **andere Agent-CLIs** in der Kachel: OpenCode, Codex CLI, Kimi Code CLI, Hermes Agent.
  Claude Code bleibt ein Typ von mehreren. Kein eigener Agent-Loop, kein reiner Terminal-Manager.
- Status **nur aus exakten Quellen** (Hooks/Plugins der CLIs), keine Bildschirm-Heuristik.
- Kadrell **installiert die Hook-/Plugin-Einträge selbst** (Menüpunkt je CLI). Vorher je CLI prüfen, ob es wie
  Claude (`--settings`) eine Konfiguration nur für den gestarteten Prozess erlaubt (Codex: `-c key=value`,
  Hermes: `--ignore-user-config`/Profil, OpenCode: `OPENCODE_CONFIG_CONTENT`?), dann entfällt der Installer.

## Zielbild

```
Kachel (PTY)  ──startet──▶  claude | opencode | codex | kimi | hermes   (AgentKind)
                                   │ Hook/Plugin (von Kadrell installiert), erbt KADRELL_SOCKET, KADRELL, KADRELL_SESSION_KEY
                                   ▼
                         "$KADRELL" status <working|waiting|idle> [--session-id ID] [--title T] [--waiting-for X] [--message M]
                                   │ Unix-Socket (ControlSocket, Aufrufer über pid-Baum bekannt)
                                   ▼
                 SessionRegistry.report: Status / sessionId / Titel je Kachel, ohne Polling
```

Ein Kommando, ein Wire-Format für alle Typen; pro CLI nur ein dünner Adapter (Skript oder Plugin), der das
Hook-Payload auf `kadrell status` abbildet. Resume bleibt CLI-spezifisch, gekapselt im Typ. Claude-Sonderfunktionen
(Usage-Anzeige, Übernahme von `--bg`, Transcript-Fallback) bleiben Claude-only.

Was **nicht** geht: Session-Id beim Start vorgeben (OpenCode, Codex, Hermes lehnen das ab, Feature-Requests
geschlossen; nur Claude `--session-id`). ACP (Agent Client Protocol) als Statuskanal: ersetzt die TUI durch den
Client, ungeeignet, solange die CLI im Terminal bleiben soll. OpenCode-SQLite als Statusquelle: kein Status-Feld.

## Modell

- `enum AgentKind: String, Codable { case claude, opencode, codex, kimi, hermes }` mit `binaryName`, `displayName`,
  `installDocsURL`, `launchArgs(...)`, `resumeArgs(sessionId:)`, `presetsSessionId` (nur Claude), `configDir`.
  Als switch-Properties, kein Protokoll mit fünf Klassen: die Unterschiede sind Daten.
- `Session.kind: AgentKind = .claude` (in `CodingKeys`, Default beim Decodieren, alte `sessions.json` bleibt gültig).
  `sessionId` bei Nicht-Claude leer, bis der Hook sie bei `SessionStart` meldet (innerhalb der ersten Sekunde).
- `ClaudeCLI` → `AgentCLI`: Login-Shell-Umgebung einmal lesen, Binary je Typ auflösen (`~/.local/bin/<name>`, sonst
  `command -v`). Claude-Spezifisches (`agents()`, `stop(id:)`, `checkVersion`, Marker-Strippen) in einer Extension.
- `AttachManager.attachNow`: der Claude-Zweig wird `session.kind.launch(...)`. Fehlt das Binary eines Typs: Kachel
  mit Hinweis (wie „Ordner weg“), keine App-weite Blockade mehr; `recheckCLI` prüft nur Typen, die in `sessions.json`
  vorkommen.
- Einstellungen: Abschnitt „Claude“ wird „Agenten“, je Typ Binary-Pfad (leer = automatisch), Standardmodell
  (Freitext), Integrationsstatus mit Installieren-Knopf. Claude-Zeilen (Modus, Effort, Bypass) bleiben.
- ⌘N bekommt ein Typ-Segment (Default: zuletzt benutzt), `startSession(kind:)`, `kadrell new --agent <typ>`.
  Kürzel neben dem Titel nur, wenn mehrere Typen im Baum sind.

## Start-Argumente je Typ

| Typ | neu | fortsetzen | Modell/Modus | Id vorgebbar | Titelquelle |
|---|---|---|---|---|---|
| Claude | `--session-id <uuid>` | `--resume <uuid>` | `--model`, `--permission-mode`, `--effort`, `--allow-dangerously-skip-permissions` | ja | Hook (`sessions/$CLAUDE_PID.json`, `nameSource != derived`) |
| OpenCode 1.18 | `opencode <dir>` | `opencode -s <ses_…>` | `-m provider/model`, `--agent`, `--auto` | nein | Plugin `session.updated` (`info.title`) |
| Codex 0.15x | `codex` | `codex resume <thread-id>` | `-m`, `--oss` (`oss_provider` ollama/lmstudio), Provider aus `config.toml` | nein | Hook `Stop` liest `~/.codex/session_index.jsonl` (`thread_name`) |
| Kimi Code 2.0 | `kimi` | `kimi --session <id>` | `-m`, Provider aus Config (`openai` + `base_url` für Ollama) | nicht dokumentiert | Hook-Payload `session_title` |
| Hermes 0.21 | `hermes chat --accept-hooks` | `hermes chat --resume <id> --accept-hooks` | `-m`, `--provider`, `--reasoning` | nein | Hook liest `sessions.title` aus `~/.hermes/state.db` |

OpenCode und Hermes lokal gegen `--help` verifiziert; Codex (learn.chatgpt.com/docs/hooks) und Kimi Code
(moonshotai.github.io/kimi-code) nur per Doku, beide sind nicht installiert: vor der Umsetzung installieren und
`--help` gegenprüfen. Kimi: nur die neue TypeScript-CLI „Kimi Code“ (`~/.kimi-code/`, v2.0.0), nicht das auslaufende
Python-`kimi-cli`. `--accept-hooks` bei Hermes: sonst fragt Hermes beim ersten Start nach Zustimmung zu den Shell-Hooks.

## Integrationen (Statuskanal je Typ)

| Typ | Datei von Kadrell | Eintrag in | Ereignisse → Status |
|---|---|---|---|
| Claude (fertig) | keine, Shell-Zeile `"$KADRELL" hook claude` | kein Eintrag: `--settings '<json>'` beim Start jedes Prozesses (`ClaudeHook.launchArgs`) | `SessionStart` → session-id; `UserPromptSubmit` → working; `PermissionRequest`, `Notification` (permission_prompt/agent_needs_input/elicitation_dialog) → waiting; `Notification` (idle_prompt) → idle; `Stop` (`last_assistant_message`) → idle |
| OpenCode | `~/.config/opencode/plugins/kadrell.js` | Datei im Plugin-Ordner genügt; Plugin läuft im eingebetteten Server der TUI, sieht die Kachel-Umgebung | `session.created/updated` → session-id + `info.title`; `session.status` `{type: busy}`, `tool.execute.*` → working; `permission.updated` (1.18) / `permission.asked` (2.x), `question.asked` → waiting; `permission.replied` → working; `session.status` `{type: idle}`, `session.idle` → idle. Kindsessions (`info.parentID`) ignorieren. Strukturvorlage: das herdr-Plugin im selben Ordner |
| Codex | `~/.config/kadrell/integrations/codex.sh` | `~/.codex/hooks.json` (Claude-kompatibel: `hook_event_name`, `session_id`, `cwd`, `transcript_path`) | `SessionStart` → session-id; `UserPromptSubmit` → working; `PermissionRequest` → waiting; `Stop` (`last_assistant_message`) → idle; `Interrupt` → idle. Kein `Notification`. Falle: fremde Hooks muss der Nutzer einmal per `/hooks` in Codex bestätigen, sonst laufen sie still nicht |
| Kimi Code | `~/.config/kadrell/integrations/kimi.sh` | Hook-Config laut Doku unter `~/.kimi-code/` (Format bei Installation prüfen) | `SessionStart` → session-id; `TurnStarted` → working; `PermissionRequest` → waiting; `PermissionResult` → working; `Stop`/`Interrupt` → idle; `session_title` in jedem Payload |
| Hermes | `~/.config/kadrell/integrations/hermes.sh` | `~/.hermes/config.yaml` `hooks:` (Shell-Hooks, stdin-JSON mit `hook_event_name`, `session_id`, `cwd`, `extra`) | `on_session_start` → session-id; `pre_llm_call` → working; `pre_approval_request` → waiting; `post_approval_response` → working; `on_session_end` (je Turn-Ende) → idle. Titel per `sqlite3 -readonly ~/.hermes/state.db` |

Alle Skripte: ohne `KADRELL_SOCKET` sofort `exit 0`, Aufruf `"$KADRELL" status …` mit kurzem Timeout, nie
blockierend. Eigene Einträge am Kommando erkennbar (idempotent), fremde nie anfassen, beim Deinstallieren nur
Eigenes entfernen, einmaliges Backup `<datei>.kadrell-bak`. Hinweis Codex: `notify` (nur Turn-Ende, Payload als
argv) bleibt außen vor, Hooks decken alles ab.

## Reihenfolge

1. Fertig: `kadrell status`, `kadrell hook claude`, Claude-Integration, Poll nur als Fallback.
2. `AgentKind` + `Session.kind` + `AgentCLI` je Typ + Start-Zweig in `AttachManager`, ⌘N-Segment, `kadrell new --agent`,
   Einstellungen „Agenten“. Erst mit **OpenCode** (installiert, Plugin-Vorlage vorhanden, Ollama/llama.cpp/Kimi laufen
   darüber). Etwa 1,5 Tage.
3. **Hermes** (Shell-Hooks, installiert, lokal testbar gegen pc-michael). Etwa ein halber Tag.
4. **Codex** und **Kimi Code** (Doku-basiert, beide vorher installieren und `--help` gegenprüfen). Etwa ein Tag.
5. Texte („Claude“ → Typname), README, Changelog, Version `minor`.

## Offene Punkte

- Kimi Code 2.0: ob `--session <neue-id>` eine Session anlegt (beim Python-Vorgänger ja), genauer Ort der Hook-Config.
- OpenCode: `question.asked` in 1.18 nicht in den SDK-Typen, nur in 2.x; beide Namen abfangen.
- Bei Nicht-Claude-Typen ist die Kachel bis zur ersten Hook-Meldung ohne sessionId; ein „fortsetzen“ davor startet eine
  neue Konversation. Akzeptiert.

## Nebenbefund: Claude Code mit fremden Backends

Ohne CLI-Wechsel geht auch `ANTHROPIC_BASE_URL` + `ANTHROPIC_AUTH_TOKEN` (Ollama ≥ 0.14 `/v1/messages`, llama.cpp
`llama-server`, Moonshot `api.moonshot.ai/anthropic`, Z.ai, MiniMax, DeepSeek, OpenRouter). Verloren gehen dabei
Subscription-Login, Prompt-Caching, 1M-Kontext, WebSearch; Anthropic nennt es „not supported“. Nicht Teil dieses Plans,
wäre aber nur ein Umgebungs-Preset je Session.
