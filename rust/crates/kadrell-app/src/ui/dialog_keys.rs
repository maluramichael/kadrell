//! Tastatur der Dialoge wie in `OverlayPanel.swift` und `Theme+SwiftUI.swift`.
//!
//! - Enter, Strg/⌘+Enter und Esc laufen für alle Dialoge über dieselben Aktionen (`keys`): Esc bricht ab, ⌘⏎ löst immer
//!   die Haupttaste aus, ⏎ allein nur bei `plain_return` (`performKeyEquivalent`: Return mit ⌘ oder `plainReturn`).
//! - Tab und Umschalt+Tab laufen durch die Bedienelemente des Dialogs (Felder, Tasten, Auswahlfelder), wie AppKit mit
//!   der Tastaturnavigation. Ein Bedienelement ist ein Tab-Stopp (`control`), sein Fokus bleibt über Frames erhalten.
//! - Hat ein Bedienelement den Fokus per Tastatur, zeichnet der Aufrufer den Tastaturring (`kbdFocusRing`); nach einem
//!   Mausklick bleibt er weg (`shows_ring`).
//! - Fokussiert, lösen Enter und Leertaste das Element aus.

use std::rc::Rc;

use gpui::{
    App, ClickEvent, Context, Div, ElementId, FocusHandle, InteractiveElement, KeyBinding, KeyContext, Stateful, StatefulInteractiveElement, Window, actions,
};

actions!(kadrell_dialog_keys, [Primary, Force, Cancel, Activate, NextControl, PrevControl]);

/// Was ein Bedienelement auslöst.
pub type Activation = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// Kontext der Dialoge, die Enter und Esc über `keys` bekommen (Bezeichner neben dem eigenen Kontext des Dialogs).
pub const KEYS: &str = "KadrellDialogKeys";
/// Kontext des Hintergrunds, der Tab zwischen den Bedienelementen weiterreicht (`nav`).
pub const NAV: &str = "KadrellDialogNav";
/// Kontext eines fokussierbaren Bedienelements: Enter und Leertaste lösen es aus.
pub const CONTROL: &str = "KadrellControl";

/// Kontext eines Dialogs: sein eigener Name plus `KEYS`.
pub fn context(own: &str) -> KeyContext {
    KeyContext::parse(&format!("{own} {KEYS}")).unwrap_or_default()
}

pub fn bind_keys(cx: &mut App) {
    let force = if cfg!(target_os = "macos") { "cmd-enter" } else { "ctrl-enter" };
    cx.bind_keys([
        KeyBinding::new("enter", Primary, Some(KEYS)),
        KeyBinding::new(force, Force, Some(KEYS)),
        KeyBinding::new("escape", Cancel, Some(KEYS)),
        KeyBinding::new("tab", NextControl, Some(NAV)),
        KeyBinding::new("shift-tab", PrevControl, Some(NAV)),
        KeyBinding::new("enter", Activate, Some(CONTROL)),
        KeyBinding::new("space", Activate, Some(CONTROL)),
    ]);
}

/// Löst ⏎ allein die Haupttaste aus? Mit ⌘ immer, sonst nur, wenn der Dialog `plainReturn` hat.
pub fn return_fires(plain_return: bool, with_command: bool) -> bool {
    with_command || plain_return
}

/// Hängt Enter, Strg/⌘+Enter und Esc an einen Dialog (dessen Kontext `context(..)` enthält). `primary` ist, was die
/// Haupttaste der Fußzeile tut.
pub fn keys<V: 'static>(
    el: Div,
    plain_return: bool,
    cx: &mut Context<V>,
    primary: fn(&mut V, &mut Window, &mut Context<V>),
    cancel: fn(&mut V, &mut Window, &mut Context<V>),
) -> Div {
    el.on_action(cx.listener(move |v, _: &Primary, w, cx| {
        if return_fires(plain_return, false) {
            primary(v, w, cx);
        }
    }))
    .on_action(cx.listener(move |v, _: &Force, w, cx| primary(v, w, cx)))
    .on_action(cx.listener(move |v, _: &Cancel, w, cx| cancel(v, w, cx)))
}

/// Tab und Umschalt+Tab am Hintergrund des Dialogs: Fokus zum nächsten oder vorigen Bedienelement.
pub fn nav(backdrop: Div) -> Div {
    backdrop
        .key_context(NAV)
        .on_action(|_: &NextControl, window, cx| window.focus_next(cx))
        .on_action(|_: &PrevControl, window, cx| window.focus_prev(cx))
}

/// Dauerhafter Fokus eines Bedienelements, über Frames hinweg am Namen festgehalten; ein Tab-Stopp.
pub fn handle(id: impl Into<ElementId>, window: &mut Window, cx: &mut App) -> FocusHandle {
    window.use_keyed_state(id, cx, |_, cx| cx.focus_handle().tab_stop(true)).read(cx).clone()
}

/// Zeigt das Element den Tastaturring? Nur mit Fokus und nur, wenn zuletzt eine Taste gedrückt wurde (nicht nach Klick).
pub fn shows_ring(handle: &FocusHandle, window: &Window) -> bool {
    handle.is_focused(window) && window.last_input_was_keyboard()
}

/// Macht ein Element zum Tab-Stopp: Fokus am Namen, Enter und Leertaste lösen `on_click` aus. Zweiter Wert: Tastaturring
/// zeichnen? Das Element muss `relative` sein, wenn der Ring als Kind darin liegt.
pub fn control(
    el: Stateful<Div>,
    id: impl Into<ElementId>,
    window: &mut Window,
    cx: &mut App,
    on_click: Activation,
) -> (Stateful<Div>, bool) {
    let handle = handle(id, window, cx);
    let ring = shows_ring(&handle, window);
    let by_key = on_click.clone();
    let el = el
        .track_focus(&handle)
        .key_context(CONTROL)
        .on_action(move |_: &Activate, window, cx| by_key(&ClickEvent::default(), window, cx))
        .on_click(move |event, window, cx| on_click(event, window, cx));
    (el, ring)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `performKeyEquivalent`: Return löst mit ⌘ immer aus, ohne ⌘ nur bei `plainReturn`.
    #[test]
    fn return_needs_command_unless_plain() {
        assert!(return_fires(true, false));
        assert!(return_fires(true, true));
        assert!(return_fires(false, true));
        assert!(!return_fires(false, false));
    }

    #[test]
    fn dialog_context_names_the_dialog_and_the_shared_keys() {
        let c = context("KadrellRename");
        assert!(c.contains("KadrellRename") && c.contains(KEYS));
    }
}
