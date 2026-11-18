//! Alles, was Kadrell mit Claude Code (und Pi) macht, ohne Oberfläche. Portiert aus `app/Kadrell/Services` (Swift ist die
//! Quelle der Wahrheit), die Swift-Tests laufen als Unit-Tests mit.
//!
//! Dieses Crate hängt bewusst nicht von `kadrell-core` ab. Eigene, kleine Ein- und Ausgabetypen stehen stattdessen hier;
//! die Verdrahtung in Welle 2 bildet sie auf den Kern ab:
//!
//! | Typ hier | Kern |
//! | --- | --- |
//! | `agent::Status` | `SessionStatus` (running, waiting, idle, error), dazu `map_status` und `is_auto_name` (`Session.mapStatus`, `Session.isAutoName`) |
//! | `agent::AgentInfo` | `Agent` (Eintrag aus `claude agents --json` bzw. `sessions/<pid>.json`) |
//! | `registry::SessionRecord` | der Teil von `Session`, den Poll und Hooks schreiben: id, cwd, startedAt, sessionId, name, customName, localTitle, model, rawStatus, pid, waitingFor, branch, activeWorktree, firstPrompt, hasRunningShell; `SessionKind` ersetzt `isShell`/`isRemote`/`isPi` |
//! | `registry::ClosedSession` | `ClosedSession` (closed.json) |
//! | `hook::HookReport` | Argumente von `kadrell status` und von `SessionRegistry.report` (state, sessionId, title, waitingFor, message, firstPrompt) |
//! | `cli::LaunchFlags` | Claude-Einstellungen (`claudeAllowBypass`, `claudeMode`, `claudeModel`, `claudeEffort`, je Session überschrieben) |
//! | `cli::LaunchSpec`, `cli::ClaudeStart` | Eingabe und Ergebnis des Startens einer Kachel (Programm, Argumente, Ordner, ganze Umgebung) |
//! | `quick_start::GroupInfo`, `quick_start::SessionInfo` | Felder von `Group` bzw. `Session` (offene und geschlossene) für den Schnellstart |
//! | `attention::AttentionSession`, `attention::AttentionStore` | `Session` (id, activeWorktree) und die Profil-Einstellungen (`sessions.unseen`, Tipp-Flags) |
//! | `session_title::TitleCandidate` | Felder von `Session` für den lokalen Sessiontitel |
//! | `pi::LocalModelRef`, `pi::PiModel`, `pi::LocalProviderKind` | `LocalModelRef`, `PiModel`, `LocalProviderKind` der lokalen Modelle |
//! | `local_model::LocalModel` | die HTTP-Anbindung von Ollama und llama.cpp (kommt aus der App) |
//!
//! Prozesse, Zeit und Dateisystem-Orte kommen von außen: `cli::CommandRunner` (`SystemRunner` startet echte Prozesse),
//! `registry::PollEnv` (`FsPollEnv` für die Dateien), `background_shells::ProcessTable`. Pfade rechnen mit `/` und laufen
//! über `paths` (Windows: `\` und `C:` werden erkannt, `canonicalize` kommt nirgends vor).

pub mod agent;
pub mod attention;
pub mod background_shells;
pub mod cli;
pub mod folder_index;
pub mod git;
pub mod history;
pub mod hook;
pub mod link_target;
pub mod local_model;
pub mod paths;
pub mod pi;
pub mod quick_start;
pub mod registry;
pub mod report_fallback;
mod runner;
pub mod session_title;
pub mod ssh_config;
pub mod text;
pub mod transcript;
pub mod worktree;
