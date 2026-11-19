//! Bedienung des Baums, die nicht an einer Zeile hängt: „Alle Gruppen auf-/zuklappen“ (⇧⌘B, `toggleAllGroups`), ⌘/Strg
//! gehalten (Symbol der Gruppen-Leiste) und die Zeilenmaße für Scrollen und ⇧F10 (`groups::rows::focus_row_top`).

use gpui::{App, Context, Div, InteractiveElement, KeyBinding, ModifiersChangedEvent, actions};

use super::groups::rows::Rows;
use super::tree_rules;
use crate::Shell;
use crate::appearance::Appearance;

actions!(kadrell_tree, [ToggleAllGroups]);

/// ⇧⌘B (Mac) bzw. Alt+Umschalt+B (Windows).
pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([KeyBinding::new(crate::keys::platform("cmd-shift-b"), ToggleAllGroups, None)]);
}

/// Aktionen des Fensters.
pub fn attach_actions(root: Div, cx: &mut Context<Shell>) -> Div {
    root.on_action(cx.listener(|this, _: &ToggleAllGroups, _, cx| this.toggle_all_groups(cx)))
        .on_modifiers_changed(cx.listener(|this, ev: &ModifiersChangedEvent, _, cx| {
            if this.tree_anim.set_cmd(ev.modifiers.secondary(cx)) {
                cx.notify();
            }
        }))
}

/// Zeilenmaße des Baums in Pixeln (mit UI-Größe) für `focus_row_top`: mit Nachrichtenzeile höhere Session-Zeilen, im
/// kompakten Design die 2 px hohe Fuge vor jeder Gruppe außer der ersten (`h_seam`).
pub fn row_measures(a: &Appearance) -> Rows {
    let m = a.rows();
    let seam = if m.path { 0. } else { 2. };
    Rows { top_pad: a.s(m.top_inset), group_row: a.s(m.group_row), session_row: a.s(m.session_height(a.show_message)), group_gap: a.s(m.group_gap + seam) }
}

impl Shell {
    /// Nur für Screenshots: `KADRELL_DEV_TREE_FOCUS=1` gibt dem Baum die Tastatur (Tastaturring an der Fokus-Zeile).
    pub(crate) fn apply_dev_tree_focus(&mut self) {
        self.ws.tree_focus = std::env::var_os("KADRELL_DEV_TREE_FOCUS").is_some();
    }

    /// Ist irgendeine Gruppe offen, gehen alle zu, sonst alle auf (`SidebarView.toggleAllGroups`).
    pub(crate) fn toggle_all_groups(&mut self, cx: &mut Context<Self>) {
        let ids: Vec<String> = self.state.store.groups.groups.iter().map(|g| g.id.clone()).collect();
        let states: Vec<bool> = ids.iter().map(|id| self.state.collapsed.contains(id)).collect();
        let target = tree_rules::collapse_all(&states);
        for (id, was) in ids.iter().zip(states) {
            if was != target {
                self.state.toggle_collapsed(id);
            }
        }
        self.rebuild();
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::appearance::metrics::RowMetrics;

    /// Nachrichtenzeile und Fuge verschieben die Zeilen, die ⇧F10 und das Scrollen ansteuern.
    #[test]
    fn measures_include_message_line_and_compact_seam() {
        let mut a = Appearance::default();
        let plain = row_measures(&a);
        assert_eq!((plain.top_pad, plain.group_row, plain.session_row, plain.group_gap), (6., 38., 24., 6.));
        a.show_message = true;
        assert_eq!(row_measures(&a).session_row, 24. + 16.);
        a.sidebar = kadrell_core::settings::SidebarStyle::Compact;
        let compact = row_measures(&a);
        assert_eq!(compact.group_gap, RowMetrics::COMPACT.group_gap + 2.);
        assert_eq!(compact.session_row, 22. + 16.);
    }
}
