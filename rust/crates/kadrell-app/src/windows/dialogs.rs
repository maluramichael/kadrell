//! Menü- und Kürzelbefehle eines weiteren Fensters: sie laufen in diesem Fenster (`current::on_main`), nicht als Aktion im
//! Hauptfenster. Dialoge, die sie öffnen, stehen so im Fenster des Befehls (`ui::overlay_host`), wie `SheetPresenter.present`
//! sie an das Fenster hängt, das den Befehl gab. Das Hauptfenster kommt nicht nach vorn.

use gpui::{Context, InteractiveElement, Stateful, Window};

use super::extra::ExtraWindow;
use crate::Shell;

impl ExtraWindow {
    /// Lässt die Shell `run` in diesem Fenster ausführen; `dialog`: der Befehl öffnet einen Dialog (der „Pin“ hält dieses
    /// Fenster als aktuelles, bis er zu ist).
    pub(super) fn run_here(&mut self, dialog: bool, cx: &mut Context<Self>, run: impl FnOnce(&mut Shell, &mut Window, &mut Context<Shell>) + 'static) {
        super::current::on_main(&self.shell, self.index, dialog, cx, run);
    }
}

type Root = Stateful<gpui::Div>;

macro_rules! here {
    ($root:expr, $cx:expr, $dialog:expr; $($a:ty => |$sh:ident, $w:ident, $c:ident| $body:expr),+ $(,)?) => {{
        let mut root = $root;
        $( root = root.on_action($cx.listener(|this, _: &$a, _, cx| this.run_here($dialog, cx, |$sh, $w, $c| $body))); )+
        root
    }};
}

/// Befehle, die einen Dialog oder ein Panel öffnen.
fn dialog_commands(root: Root, cx: &mut Context<ExtraWindow>) -> Root {
    use crate::menus::*;
    use crate::misc::{OpenAbout, OpenWhatsNew};
    use crate::ui::ext::OpenExtensions;
    let root = here!(root, cx, true;
        NewSession => |sh, w, c| sh.new_session_action(w, c),
        NewRemote => |sh, w, c| sh.open_palette("@", w, c),
        OpenPalette => |sh, w, c| sh.open_palette("", w, c),
        FindAll => |sh, w, c| sh.open_palette("/", w, c),
        SearchHistory => |sh, w, c| sh.open_palette("#", w, c),
        QuickStart => |sh, w, c| sh.open_quick_start(w, c),
        NewGroup => |sh, w, c| sh.open_new_group(w, c),
        OpenSettings => |sh, w, c| sh.open_settings_action(w, c),
        OpenStats => |sh, w, c| sh.toggle_stats(w, c),
    );
    here!(root, cx, true;
        OpenExtensions => |sh, w, c| sh.toggle_extensions(w, c),
        OpenAbout => |sh, w, c| sh.toggle_help(crate::misc::help_view::Kind::About, w, c),
        OpenWhatsNew => |sh, w, c| sh.open_whats_new(true, w, c),
        RenameSession => |sh, w, c| sh.rename_focused(w, c),
        RenameGroup => |sh, w, c| sh.edit_focused_group(w, c),
        InstallCli => |sh, w, c| sh.install_cli_action(w, c),
    )
}

/// Befehle auf die fokussierte Session: sie öffnen höchstens eine Rückfrage.
fn session_commands(root: Root, cx: &mut Context<ExtraWindow>) -> Root {
    use crate::menus::*;
    use crate::ui::context_menu::Cmd;
    let root = here!(root, cx, false;
        NewSessionHere => |sh, w, c| sh.new_session_here(w, c),
        NewTerminal => |sh, w, c| sh.new_terminal(w, c),
        NewWindow => |sh, _w, c| sh.new_window_action(c),
        CloseSession => |sh, w, c| sh.remove_focused(w, c),
        StopSession => |sh, w, c| sh.stop_focused(w, c),
        ResumeSession => |sh, w, c| sh.resume_focused(w, c),
        SelectGroups => |sh, w, c| sh.select_groups_action(w, c),
        SelectEverything => |sh, w, c| sh.select_everything_action(w, c),
    );
    here!(root, cx, false;
        IsolateSession => |sh, w, c| sh.focused_cmd(|key, _| Cmd::ShowOnly(key), w, c),
        RemoveFromView => |sh, w, c| sh.focused_cmd(|key, _| Cmd::HideFromView(key), w, c),
        NewSessionBeside => |sh, w, c| sh.focused_cmd(|key, _| Cmd::NewSessionHere(key), w, c),
        CopyPath => |sh, w, c| sh.focused_cmd(|_, cwd| Cmd::CopyPath(cwd), w, c),
        ShowInFolder => |sh, w, c| sh.focused_cmd(|_, cwd| Cmd::Reveal(cwd), w, c),
    )
}

/// Befehle mit Angabe (Gruppe, Layout, Kürzel): die Angabe geht in den Befehl mit.
fn parameter_commands(root: Root, cx: &mut Context<ExtraWindow>) -> Root {
    use crate::menu_ctx::{MoveToGroup, PerformHotkey, SelectLayout};
    root.on_action(cx.listener(|this, a: &MoveToGroup, _, cx| {
        let group = a.group.clone();
        this.run_here(false, cx, move |sh, w, c| sh.move_focused_to_group(&group, w, c));
    }))
    .on_action(cx.listener(|this, a: &SelectLayout, _, cx| {
        let mode = a.mode.clone();
        this.run_here(false, cx, move |sh, _, c| sh.select_layout_action(&mode, c));
    }))
    .on_action(cx.listener(|this, a: &PerformHotkey, _, cx| {
        let raw = a.action.clone();
        this.run_here(false, cx, move |sh, w, c| sh.perform_hotkey_action(&raw, w, c));
    }))
}

/// Alle Befehle des Menüs und der Kürzel, die auf die Sessions und Dialoge der App wirken, in diesem Fenster.
pub fn attach_commands(root: Root, cx: &mut Context<ExtraWindow>) -> Root {
    parameter_commands(session_commands(dialog_commands(root, cx), cx), cx)
}
