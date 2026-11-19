//! Leiste eines weiteren Fensters: jedes Fenster der Mac-App hat seine eigene (`MainWindowController.bar`, gefüllt von
//! `syncWindow`). Links Layout, Spalten und Zoom dieses Fensters, rechts Konto, Zähler, Nutzung und Status der Extensions
//! wie im Hauptfenster (derselbe Stand der `Shell`) und mit derselben Darstellung (`ui::bar`). AUTO und SYNC gelten je
//! Fenster (`extra_modes`), GRP und SORT sind Einstellungen und wirken in allen Fenstern. Menüs der Leiste (Layout,
//! Konto) öffnen in diesem Fenster.

use std::rc::Rc;

use gpui::{AnyElement, Context, IntoElement, MouseDownEvent, Pixels, Point, Window, point, px};

use super::extra::ExtraWindow;
use crate::bar_wire::layout_items;
use crate::ui::bar::{LayoutCtl, bar};
use crate::ui::bar_keys::Press;
use crate::ui::context_menu::Item;
use crate::ui::layout::LayoutMode;

#[derive(Clone, Copy)]
enum Toggle {
    Auto,
    Sync,
}

/// Was die Leiste von der `Shell` braucht: Zähler, Status der Extensions und die rechte Seite.
struct Shared {
    usage: crate::ui::usage_bar::BarCtl,
    clock: String,
    running: usize,
    detached: usize,
    waiting: usize,
    ext: Vec<crate::ui::bar::ExtStatus>,
    flash: crate::ui::bar_keys::Flash,
    pressed: Option<&'static str>,
}

impl ExtraWindow {
    /// Die Leiste unter dem Fenstertitel.
    pub(super) fn bar(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let view = self.view(cx);
        let tiles = self.tile_keys(&view, cx).len();
        let shared = self.shared_bar(window, cx);
        let mut usage = shared.usage;
        usage.on_account = press(cx, |this, at, cx| this.open_account_menu(at, cx));
        usage.on_detach = press(cx, |this, _, cx| this.detach_idle(cx));
        usage.on_waiting = press(cx, |this, _, cx| this.select_waiting(cx));
        let noop: Press = Rc::new(|_, _, _| {});
        let mode = self.layout.mode;
        let (grouped, sort) = self.shell.read(cx).tree_look();
        let ctl = LayoutCtl {
            mode,
            cols_label: self.layout.cols_label(),
            cols: match mode {
                LayoutMode::Grid => self.layout.cols,
                LayoutMode::Scroll => self.layout.scroll_cols,
                _ => 0,
            },
            pick_layout: press(cx, |this, at, cx| this.open_layout_menu(at, cx)),
            dec: press(cx, |this, _, cx| this.step_columns(-1, cx)),
            inc: press(cx, |this, _, cx| this.step_columns(1, cx)),
            split: None,
            cycle_split: noop.clone(),
            auto: self.modes.auto.on,
            sync: self.modes.sync,
            zoom: view.zoom && tiles > 1,
            toggle_auto: press(cx, |this, _, cx| this.press_toggle(Toggle::Auto, cx)),
            toggle_sync: press(cx, |this, _, cx| this.press_toggle(Toggle::Sync, cx)),
            toggle_zoom: press(cx, |this, _, cx| this.toggle_zoom(cx)),
            grouped,
            sort,
            toggle_grouped: press(cx, |this, _, cx| this.shell.update(cx, |sh, scx| sh.toggle_grouped(scx))),
            cycle_sort: press(cx, |this, _, cx| this.shell.update(cx, |sh, scx| sh.cycle_sort(scx))),
            pressed: shared.pressed,
            flash: shared.flash,
            running: shared.running,
            detached: shared.detached,
            waiting: shared.waiting,
            ext: shared.ext,
            usage: Some(usage),
            compact: false,
        };
        bar(&shared.clock, &ctl, cx).into_any_element()
    }

    /// AUTO und SYNC gelten je Fenster; der Ton ist der des Hauptfensters.
    fn press_toggle(&mut self, which: Toggle, cx: &mut Context<Self>) {
        self.shell.update(cx, |sh, _| sh.play_sound(kadrell_tray::Sound::Toggle));
        match which {
            Toggle::Auto => self.toggle_auto(cx),
            Toggle::Sync => self.toggle_sync(cx),
        }
    }

    /// Der Stand der `Shell`, den jedes Fenster gleich zeigt.
    fn shared_bar(&self, window: &mut Window, cx: &mut Context<Self>) -> Shared {
        self.shell.update(cx, |sh, scx| {
            let mut usage = sh.bar_ctl(window, scx);
            sh.animate_usage(&mut usage, window);
            usage.update = sh.update_pill(scx);
            let running = sh.state.running_count();
            Shared {
                usage,
                clock: sh.clock.clone(),
                running,
                detached: sh.state.store.sessions.sessions.len().saturating_sub(running),
                waiting: sh.waiting_count(),
                ext: sh.ext_status(scx),
                flash: sh.flash_fn(scx),
                pressed: sh.usage.pressed,
            }
        })
    }

    /// Layout-Menü unter der Layout-Taste.
    fn open_layout_menu(&mut self, at: Point<Pixels>, cx: &mut Context<Self>) {
        self.open_menu(point(at.x - px(10.), at.y + px(14.)), layout_items(self.layout.mode), cx);
    }

    /// Konto-Menü unter dem Konto in der Leiste.
    fn open_account_menu(&mut self, at: Point<Pixels>, cx: &mut Context<Self>) {
        let items: Vec<Item> = self.shell.read(cx).account_menu_items();
        self.open_menu(point(at.x, at.y + px(14.)), items, cx);
    }

    /// „N trennen“: alle fertigen Sessions trennen (die Shell hält Terminals und Start).
    fn detach_idle(&mut self, cx: &mut Context<Self>) {
        super::current::on_main(&self.shell, self.index, false, cx, |sh, window, cx| sh.detach_idle_pill(window, cx));
    }

    /// „N warten“: alle wartenden Sessions in diesem Fenster zeigen (ersetzt seine Auswahl).
    fn select_waiting(&mut self, cx: &mut Context<Self>) {
        let index = self.index;
        let shown = self.shell.update(cx, |sh, _| {
            let keys = sh.waiting_keys();
            if !keys.is_empty() {
                sh.state.view_select(index, &keys, false);
                sh.state.view_clear_zoom(index);
            }
            !keys.is_empty()
        });
        if shown {
            self.changed(cx);
        }
    }
}

/// Eine Taste der Leiste, die in diesem Fenster wirkt.
fn press(cx: &mut Context<ExtraWindow>, run: impl Fn(&mut ExtraWindow, Point<Pixels>, &mut Context<ExtraWindow>) + 'static) -> Press {
    Rc::new(cx.listener(move |this, ev: &MouseDownEvent, _, cx| run(this, ev.position, cx)))
}
