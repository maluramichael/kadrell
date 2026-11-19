//! Was ein weiteres Fenster vom Hauptfenster übernimmt, damit es dieselbe Arbeitsfläche ist: Menüleiste und Titelzeile
//! (Windows), der Baum mit dem Zustand dieses Fensters, AUTO und SYNC je Fenster. In der Mac-App hat jedes Fenster seine
//! eigene `WorkspaceView` und damit eigenes `auto` und `sync`; GRP und SORT sind Einstellungen und gelten überall.

use gpui::{AnyElement, AppContext as _, Context, Entity, IntoElement, Keystroke, Window};
use kadrell_term::Forward;

use super::extra::ExtraWindow;
use crate::Shell;
use crate::ui::menubar::MenuBar;
use crate::workspace::auto::{Auto, Filter};
use crate::workspace::modes::matches_auto;
use crate::workspace::sync;

/// Menüleiste und Zustand AUTO/SYNC eines weiteren Fensters.
pub struct Modes {
    pub menubar: Entity<MenuBar>,
    /// Sprachstand, mit dem die Menüs zuletzt gebaut wurden.
    lang: u64,
    pub auto: Auto,
    /// Auswahl vor AUTO: daraus filtert AUTO, wenn es nicht alle Sessions nimmt, und dahin geht es beim Ausschalten zurück.
    before_auto: Vec<String>,
    pub sync: bool,
}

impl Modes {
    pub fn new<V: 'static>(cx: &mut Context<V>) -> Modes {
        Modes { menubar: cx.new(MenuBar::new), lang: kadrell_i18n::global().generation(), auto: Auto::default(), before_auto: Vec::new(), sync: false }
    }
}

/// Aus dem Pool (Baumreihenfolge oder Auswahl vor AUTO) und dem Filter: nur Bekannte bleiben.
pub fn auto_pool(all_sessions: bool, tree: &[String], before: &[String]) -> Vec<String> {
    if all_sessions { tree.to_vec() } else { before.iter().filter(|k| tree.contains(k)).cloned().collect() }
}

impl ExtraWindow {
    /// Menüleiste nachziehen: Schnappschuss der Shell (Fokus, Gruppen, Belegung) und Sprache.
    pub(super) fn sync_menubar(&mut self, cx: &mut Context<Self>) {
        let (built, lang) = (self.shell.read(cx).menu.built.clone(), kadrell_i18n::global().generation());
        let reload = lang != self.modes.lang;
        self.modes.lang = lang;
        self.modes.menubar.update(cx, |bar, cx| {
            if let Some(ctx) = &built {
                bar.set_context(ctx);
            }
            if reload {
                bar.reload(cx);
            }
        });
    }

    /// Der Baum dieses Fensters: dieselbe Seitenleiste wie im Hauptfenster, mit Auswahl und Fokus aus der Ansicht.
    pub(super) fn tree(&mut self, width: f32, cx: &mut Context<Self>) -> AnyElement {
        let (index, scroll, view) = (self.index, self.tree_scroll.clone(), self.view(cx));
        self.shell.update(cx, |sh, scx| {
            let sel = super::route::selection_of(&sh.groups, &view.selected, view.focus.as_deref());
            let mut ctx = sh.sidebar_ctx();
            (ctx.scroll, ctx.keys, ctx.win, ctx.width) = (scroll, false, Some(index), Some(width));
            crate::ui::sidebar::sidebar(&sh.groups, &sel, &ctx, scx).into_any_element()
        })
    }

    /// AUTO dieses Fensters ein oder aus. Aus: die Auswahl von vorher kommt zurück.
    pub(super) fn toggle_auto(&mut self, cx: &mut Context<Self>) {
        let (index, view) = (self.index, self.view(cx));
        if self.modes.auto.on {
            self.modes.auto.set(false);
            let before = std::mem::take(&mut self.modes.before_auto);
            if !before.is_empty() {
                self.shell.update(cx, |sh, _| sh.state.view_select(index, &before, false));
            }
        } else {
            self.modes.auto.set(true);
            self.modes.before_auto = view.selected;
        }
        self.shell.update(cx, |sh, _| sh.state.view_clear_zoom(index));
        self.changed(cx);
    }

    /// Gleicht AUTO mit dem Zustand der Sessions ab (Status, Nachlauf, Fokus): die Kacheln dieses Fensters werden zur
    /// Auswahl. Ohne Treffer bleibt das Fenster leer.
    pub(super) fn refresh_auto(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.modes.auto.on {
            return;
        }
        let (index, view) = (self.index, self.view(cx));
        let before = self.modes.before_auto.clone();
        let auto = &mut self.modes.auto;
        let changed = self.shell.update(cx, |sh, _| {
            let filter = Filter::from_settings(&sh.state.settings);
            let tree: Vec<String> = sh.groups.iter().flat_map(|g| g.sessions.iter().map(|s| s.key.clone())).collect();
            let pool = auto_pool(filter.all_sessions, &tree, &before);
            let state = &sh.state;
            let mut changed = auto.update(&pool, |k| matches_auto(state, &filter, k), crate::state::now_secs());
            let refocus = auto.refocus(view.focus.as_deref(), false);
            changed |= refocus.is_some();
            if changed {
                let shown = auto.shown.clone();
                sh.state.view_select(index, &shown, false);
                let keep = refocus.or(view.focus.clone());
                keep.iter().for_each(|f| sh.state.view_focus(index, f));
            }
            changed
        });
        if self.modes.auto.lingering() {
            window.request_animation_frame();
        }
        if changed {
            self.sync_main_later(cx);
        }
    }

    /// SYNC dieses Fensters ein oder aus (nicht gespeichert).
    pub(super) fn toggle_sync(&mut self, cx: &mut Context<Self>) {
        self.modes.sync = !self.modes.sync;
        cx.notify();
    }

    /// SYNC: dieselbe Taste an alle anderen Kacheln dieses Fensters, wenn das Fokus-Terminal selbst die Tastatur hat.
    pub(super) fn forward_sync(&mut self, ks: &Keystroke, window: &mut Window, cx: &mut Context<Self>) {
        if !self.modes.sync {
            return;
        }
        let view = self.view(cx);
        let Some(source) = view.focus.clone() else { return };
        let tiles = self.tile_keys(&view, cx);
        self.shell.update(cx, |sh: &mut Shell, scx| {
            let owns = sh.panes.get(&source).is_some_and(|p| p.read(scx).focus.is_focused(window));
            let forward = kadrell_term::forward_of(ks);
            if !owns || forward == Forward::Ignore {
                return;
            }
            let targets = sync::targets(true, &tiles, &source, |k| sh.state.term(k).is_some());
            for key in targets {
                match &forward {
                    Forward::Bytes(bytes) => sh.state.term(&key).iter().for_each(|t| t.write(bytes)),
                    Forward::Paste => sh.panes.get(&key).iter().for_each(|p| p.update(scx, |p, cx| p.paste(cx))),
                    Forward::Ignore => {}
                }
            }
        });
    }

    /// Startet, was die Auswahl neu braucht, im Hauptfenster (die Shell hält die Terminals); nicht mitten im Zeichnen.
    pub(super) fn sync_main_later(&self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let _ = this.update(cx, |this, cx| this.changed(cx));
        })
        .detach();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn auto_takes_every_session_or_only_the_selection_from_before() {
        let tree = keys(&["a", "b", "c"]);
        assert_eq!(auto_pool(true, &tree, &keys(&["b"])), tree);
        assert_eq!(auto_pool(false, &tree, &keys(&["c", "x", "a"])), keys(&["c", "a"]), "Unbekanntes fällt weg");
    }
}
