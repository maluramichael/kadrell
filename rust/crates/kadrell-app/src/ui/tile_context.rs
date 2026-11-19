//! Terminal in der Kachel: Kopieren, Suchleiste (⌘F, ⌘G, ⇧⌘G) und das Kontextmenü der Kachel. Port von
//! `performTextFinderAction` und `menu(for:)` (`AppDelegate.swift`, `WorkspaceView.swift`). Die Aktionen wirken auf das
//! Terminal der Fokus-Kachel; Ausschneiden gibt es dort nicht (SwiftTerm kennt kein `cut`).

use gpui::{App, Context, Div, InteractiveElement, KeyBinding, Pixels, Point, actions};
use kadrell_i18n::trd;
use kadrell_term::TerminalPane;

use super::context_menu::{Cmd, Item};
use crate::Shell;

actions!(kadrell_term_find, [FindInTerminal, FindNext, FindPrevious]);

/// Kürzel: ⌘ auf dem Mac, Alt unter Windows (Strg gehört dem Terminal).
pub fn bind_keys(cx: &mut App) {
    let k = |mac: &'static str, win: &'static str| if cfg!(target_os = "macos") { mac } else { win };
    cx.bind_keys([
        KeyBinding::new(k("cmd-f", "alt-f"), FindInTerminal, None),
        KeyBinding::new(k("cmd-g", "alt-g"), FindNext, None),
        KeyBinding::new(k("cmd-shift-g", "alt-shift-g"), FindPrevious, None),
        KeyBinding::new(k("cmd-c", "alt-c"), super::text_input::Copy, None),
    ]);
}

/// Die Aktionen am Fensterwurzel-Element.
pub fn attach_actions(root: Div, cx: &mut Context<Shell>) -> Div {
    root.on_action(cx.listener(|this, _: &super::text_input::Copy, _, cx| this.with_focused_pane(cx, |p, cx| { p.copy(cx); })))
        .on_action(cx.listener(|this, _: &FindInTerminal, _, cx| this.with_focused_pane(cx, |p, cx| p.open_search(cx))))
        .on_action(cx.listener(|this, _: &FindNext, _, cx| this.with_focused_pane(cx, |p, cx| p.step_search(true, cx))))
        .on_action(cx.listener(|this, _: &FindPrevious, _, cx| this.with_focused_pane(cx, |p, cx| p.step_search(false, cx))))
}

/// Einträge des Rechtsklicks: mit markiertem Text Kopieren und Einsetzen (am Terminal), sonst `None` (Kontextmenü der
/// Session).
pub fn text_items(key: &str, has_selection: bool) -> Option<Vec<Item>> {
    has_selection.then(|| {
        vec![Item::Entry(trd!("Kopieren"), Cmd::TermCopy(key.to_string())), Item::Entry(trd!("Einsetzen"), Cmd::TermPaste(key.to_string()))]
    })
}

impl Shell {
    fn with_focused_pane(&mut self, cx: &mut Context<Self>, f: impl FnOnce(&mut TerminalPane, &mut Context<TerminalPane>)) {
        let Some(pane) = self.state.focused.as_deref().and_then(|k| self.panes.get(k)).cloned() else { return };
        pane.update(cx, f);
    }

    /// Rechtsklick auf eine Kachel: über markiertem Terminaltext Kopieren und Einsetzen, sonst das Menü der Session.
    pub(crate) fn tile_context_menu(&mut self, key: &str, at: Point<Pixels>, cx: &mut Context<Self>) {
        let selected = self.panes.get(key).is_some_and(|p| p.read(cx).has_selection());
        let items = text_items(key, selected).unwrap_or_else(|| self.session_menu_items(key));
        self.open_context(at, items, cx);
    }

    /// „Kopieren“ aus dem Kontextmenü der Kachel `key`.
    pub(crate) fn tile_copy(&mut self, key: &str, cx: &mut Context<Self>) {
        if let Some(pane) = self.panes.get(key).cloned() {
            pane.update(cx, |p, cx| { p.copy(cx); });
        }
    }

    /// „Einsetzen“ aus dem Kontextmenü der Kachel `key`.
    pub(crate) fn tile_paste(&mut self, key: &str, cx: &mut Context<Self>) {
        if let Some(pane) = self.panes.get(key).cloned() {
            pane.update(cx, |p, cx| p.paste(cx));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `testRightClickOffersCopyPasteWhileTextIsSelected`: ohne Auswahl bleibt das Menü der Session, mit Auswahl
    /// Kopieren und Einsetzen an das Terminal der Kachel.
    #[test]
    fn right_click_offers_copy_paste_while_text_is_selected() {
        assert!(text_items("s1", false).is_none());
        let items = text_items("s1", true).expect("Auswahl");
        assert_eq!(items.len(), 2);
        assert_eq!(items[0], Item::Entry(trd!("Kopieren"), Cmd::TermCopy("s1".into())));
        assert_eq!(items[1], Item::Entry(trd!("Einsetzen"), Cmd::TermPaste("s1".into())));
    }
}
