//! Kadrell-Steuerkanal: die Kommandozeile `kadrell <befehl>` und der Kanal zur laufenden App (wie `tmux`).
//! Portiert aus der Mac-App (`Services/Control.swift`, `ControlSocket.swift`, `AppDelegate+Control.swift`,
//! `Hooks.swift`) und der Windows-Version (`windows/src`). Befehle, Flags, Meldungen und `ls --json` gelten
//! unverändert nach `docs/cli-contract.md` und `docs/contract/ls.json`; auf beiden Systemen gibt es jetzt alle Befehle.
//!
//! | Modul | Inhalt |
//! | --- | --- |
//! | [`command`] | Befehle parsen, Hilfetext, Tastennamen |
//! | [`protocol`] | Anfrage und Antwort (eine JSON-Zeile je Richtung) |
//! | [`caller`] | Aufrufer aus pid und Prozessbaum, nie aus dem Feld `caller` |
//! | [`handler`] | `ControlHandler`: was die App liefert |
//! | [`exec`] | Ziele, Freigaben, Ausführung, `send` und `capture`; `Controller` für den Server |
//! | [`list`] | Ausgabe von `ls` und `ssh-hosts` |
//! | [`server`] | Server in der App (Unix-Socket mit Peer-pid, Windows TCP + Token + TCP-Tabelle) |
//! | [`client`] | CLI: verbinden, App starten, warten, `hook claude`-Weg |
//! | [`hook`] | `kadrell hook claude`, Transportseite |
//! | [`user_hooks`] | Skripte unter `~/.config/kadrell/hooks/` |
//! | [`endpoint`] | Pfad des Kanals im Profilordner, Endpunktdatei |
//! | [`testing`] | `MockHandler` für Tests |

pub mod caller;
pub mod client;
pub mod command;
pub mod endpoint;
pub mod exec;
pub mod handler;
pub mod hook;
pub mod json_pretty;
pub mod list;
pub mod protocol;
pub mod server;
pub mod testing;
pub mod user_hooks;

pub use caller::{ProcessTree, SystemProcessTree, caller_tile, caller_tile_in};
pub use command::{Command, ControlError, LayoutMode, NewSession, Status};
pub use exec::{Controller, Followup, Outcome, execute, handle};
pub use handler::{Capture, ControlHandler, NewRequest, ResumeBlocker, RunState, SshHosts};
pub use protocol::{ControlRequest, ControlResponse, STARTING_STATUS};
pub use server::{ControlServer, Service};
