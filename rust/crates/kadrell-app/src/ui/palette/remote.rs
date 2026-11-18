//! `@`-Modus der Palette (`PaletteWindow.remoteItems`): ssh-Hosts, `@host:` die tmux-Sessions des Hosts. Die Listen
//! bildet der Zustand des Remote-Dialogs (`crate::remote::dialog::RemoteState`), hier werden sie zu Trefferzeilen.

use std::rc::Rc;

use crate::remote::dialog::{Item as RemoteItem, RemoteState};

use super::model::{Item, Target};

/// Liefert (zuletzt verbundene Hosts, Hosts aus `~/.ssh/config`). Wird erst beim ersten `@` gerufen, nicht bei jedem
/// Öffnen der Palette (liest `~/.ssh/config` samt `Include`).
pub type HostSource = Rc<dyn Fn() -> (Vec<String>, Vec<String>)>;

/// Zustand für `@`: einmal pro Öffnen angelegt, behält die geholten tmux-Sessions je Host.
pub fn state_from(source: &HostSource) -> RemoteState {
    let (recent, config) = source();
    RemoteState::new(&recent, &config)
}

fn row(item: RemoteItem) -> Item {
    let target = item.connect.map_or(Target::Nothing, Target::Connect);
    Item::new(item.label, item.sub, target)
}

/// Trefferzeilen für den aktuellen Begriff (Text hinter dem `@`).
pub fn items(state: &RemoteState) -> Vec<Item> {
    state.items().into_iter().map(row).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::remote::dialog::{Connect, Sessions};

    fn state(query: &str) -> RemoteState {
        let mut s = RemoteState::new(&["vault".to_string()], &["box".to_string(), "vault".to_string()]);
        s.set_query(query);
        s
    }

    #[test]
    fn hosts_show_recent_first_with_hint() {
        let rows = items(&state(""));
        assert_eq!(rows.iter().map(|r| r.label.as_str()).collect::<Vec<_>>(), ["vault", "box"]);
        assert_eq!(rows[0].sub, "ssh · ⏎ tmux attach · „vault:“ wählt die Session");
        assert_eq!(rows[0].target, Target::Connect(Connect { host: "vault".into(), tmux: None }));
    }

    #[test]
    fn colon_lists_loading_then_sessions_with_new_row_first() {
        let mut s = state("vault: x");
        let rows = items(&s);
        assert_eq!((rows[0].label.as_str(), rows[1].label.as_str()), ("Neu: x", "lädt …"));
        assert_eq!(rows[0].sub, "tmux new -As auf vault");
        assert_eq!(rows[1].target, Target::Nothing);
        s.sessions.insert("vault".into(), Sessions::Found(vec!["xterm".into(), "main".into()]));
        let rows = items(&s);
        assert_eq!(rows.iter().map(|r| r.label.as_str()).collect::<Vec<_>>(), ["Neu: x", "xterm"]);
        assert_eq!(rows[1].sub, "tmux-Session auf vault");
        assert_eq!(rows[1].target, Target::Connect(Connect { host: "vault".into(), tmux: Some("xterm".into()) }));
    }

    #[test]
    fn unreachable_and_empty_hosts_are_notes() {
        let mut s = state("vault:");
        s.sessions.insert("vault".into(), Sessions::Unreachable);
        let rows = items(&s);
        assert_eq!((rows[0].label.as_str(), rows[0].sub.as_str()), ("vault nicht erreichbar", "„Neu: …“ versucht es trotzdem"));
        s.sessions.insert("vault".into(), Sessions::Found(vec![]));
        assert_eq!(items(&s)[0].label, "kein tmux-Server auf vault");
    }
}
