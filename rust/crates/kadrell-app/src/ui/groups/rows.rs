//! Kleine Bausteine für die Zeilen des Baums: Zeilenschlüssel (Mittelklick), Lage einer Zeile für ⇧F10, Einfügelinie
//! und Rahmen beim Ziehen, die Ghost-Ansicht am Mauszeiger und der Start des Editors.

use gpui::{Context, IntoElement, ParentElement, Render, Styled, Window, div, px};

use super::drag::{DragRow, Plan};
use crate::ui::model::Group;
use crate::ui::theme;

/// Zeile des Baums, die der Mittelklick trifft.
#[derive(Clone, Debug, PartialEq)]
pub enum RowKey {
    Session(String),
    Group(String),
}

/// Mittlere Taste losgelassen: war es dieselbe Zeile wie beim Drücken (dann schließen)? Merkt sich nichts mehr.
pub fn middle_released(pressed: &mut Option<RowKey>, on: &RowKey) -> bool {
    pressed.take().as_ref() == Some(on)
}

/// Wie die Ablage in der Zeile dargestellt wird.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mark {
    /// Einfügelinie über dem Ziel (Ziehen nach oben).
    Above,
    /// Einfügelinie unter dem Ziel (Ziehen nach unten, bei Gruppen unter deren letzter Zeile).
    Below,
    /// Rahmen um die ganze Zielgruppe: die Session fällt hinein, statt umzusortieren.
    Ring,
}

/// Markierung einer Session-Zeile mit der Ordnungsnummer `target_order` beim Umsortieren.
pub fn row_mark(plan: &Option<Plan>, drag: &DragRow, target_order: usize) -> Option<Mark> {
    match plan {
        Some(Plan::ReorderSession { .. } | Plan::ReorderFlat { .. }) => Some(if target_order > drag.order { Mark::Below } else { Mark::Above }),
        _ => None,
    }
}

/// Markierung eines Gruppenblocks (Kopf und Sessions), dessen Kopf die Nummer `head_order` trägt.
pub fn block_mark(plan: &Option<Plan>, drag: &DragRow, head_order: usize) -> Option<Mark> {
    match plan {
        Some(Plan::MoveToGroup { .. }) => Some(Mark::Ring),
        Some(Plan::MoveGroup { .. }) => Some(if head_order > drag.order { Mark::Below } else { Mark::Above }),
        _ => None,
    }
}

/// Zeilenmaße des Baums in Pixeln (mit UI-Größe): Abstand oben, Höhe von Gruppen- und Session-Zeile, Luft vor jeder
/// Gruppe außer der ersten.
#[derive(Clone, Copy)]
pub struct Rows {
    pub top_pad: f32,
    pub group_row: f32,
    pub session_row: f32,
    pub group_gap: f32,
}

/// Obere Kante der Zeile, die für die Fokus-Session steht (Baum ohne Scrollen, ab Oberkante der Fläche), und ob es
/// die Gruppenzeile ist: steckt die Session in einer zugeklappten Gruppe, steht ihre Gruppenzeile für sie.
pub fn focus_row_top(groups: &[Group], key: &str, rows: Rows) -> Option<(f32, bool)> {
    let Rows { top_pad, group_row, session_row, group_gap } = rows;
    let mut y = top_pad;
    for (n, g) in groups.iter().enumerate() {
        y += if n > 0 { group_gap } else { 0. };
        let has = g.sessions.iter().position(|s| s.key == key);
        if let Some(i) = has {
            return Some(if g.collapsed { (y, true) } else { (y + group_row + i as f32 * session_row, false) });
        }
        y += group_row + if g.collapsed { 0. } else { g.sessions.len() as f32 * session_row };
    }
    None
}

/// Befehlszeile für den Editor der Einstellungen (`openEditor`): über die Shell, damit `code` und Co. im PATH
/// liegen. Programm und Argumente; `None` ohne Befehl.
pub fn editor_command_line(command: &str) -> Option<(String, Vec<String>)> {
    let command = command.trim();
    if command.is_empty() {
        return None;
    }
    let line = format!("{command} .");
    Some(if cfg!(windows) {
        ("cmd".to_string(), vec!["/C".to_string(), line])
    } else if cfg!(target_os = "macos") {
        ("/bin/zsh".to_string(), vec!["-lc".to_string(), line])
    } else {
        ("/bin/sh".to_string(), vec!["-lc".to_string(), line])
    })
}

/// Startet den Editor im Ordner `cwd`, ohne zu warten.
pub fn spawn_editor(command: &str, cwd: &str) -> std::io::Result<()> {
    let Some((program, args)) = editor_command_line(command) else { return Ok(()) };
    if cwd.is_empty() {
        return Ok(());
    }
    let mut cmd = std::process::Command::new(program);
    cmd.current_dir(cwd).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // Die Zeile geht unverändert an cmd (Anführungszeichen im Befehl bleiben), ohne Konsolenfenster.
        cmd.raw_arg("/C").raw_arg(format!("\"{}\"", args[1])).creation_flags(0x0800_0000);
    }
    #[cfg(not(windows))]
    cmd.args(&args);
    cmd.spawn().map(|_| ())
}

/// Anzeige am Mauszeiger beim Ziehen einer Zeile oder Kachel.
pub struct DragGhost {
    pub label: String,
}

impl Render for DragGhost {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme(cx);
        div()
            .px(px(10.))
            .py(px(4.))
            .rounded(px(6.))
            .bg(t.raised())
            .border_1()
            .border_color(t.accent)
            .text_size(px(12.))
            .text_color(t.fg)
            .child(self.label.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::groups::drag::Dragged;

    fn drag(order: usize) -> DragRow {
        DragRow { item: Dragged::Session("a".into()), group: "g1".into(), order, label: "a".into() }
    }

    fn view_group(keys: &[&str], collapsed: bool) -> Group {
        let sessions = keys
            .iter()
            .map(|k| crate::ui::model::test_session(k))
            .collect();
        Group { id: "g".into(), name: "g".into(), color: 0, path: String::new(), cwd: String::new(), favorite: false, collapsed, host: None, sessions }
    }

    #[test]
    fn middle_click_closes_only_on_the_same_row() {
        let a = RowKey::Session("a".into());
        let mut pressed = Some(a.clone());
        assert!(middle_released(&mut pressed, &a));
        assert_eq!(pressed, None);
        let mut pressed = Some(a.clone());
        assert!(!middle_released(&mut pressed, &RowKey::Group("a".into())), "andere Zeile");
        assert_eq!(pressed, None, "auch dann ist der Druck verbraucht");
        assert!(!middle_released(&mut None, &a));
    }

    #[test]
    fn marks_follow_the_drag_direction() {
        let same = Some(Plan::ReorderSession { id: "a".into(), target: "b".into() });
        assert_eq!(row_mark(&same, &drag(5), 3), Some(Mark::Above));
        assert_eq!(row_mark(&same, &drag(2), 3), Some(Mark::Below));
        assert_eq!(row_mark(&None, &drag(2), 3), None);
        let into = Some(Plan::MoveToGroup { id: "a".into(), group: "g2".into() });
        assert_eq!(block_mark(&into, &drag(5), 0), Some(Mark::Ring));
        assert_eq!(row_mark(&into, &drag(5), 0), None);
        let group = Some(Plan::MoveGroup { id: "g1".into(), target: "g2".into() });
        assert_eq!(block_mark(&group, &drag(0), 4), Some(Mark::Below));
        assert_eq!(block_mark(&group, &drag(6), 4), Some(Mark::Above));
    }

    #[test]
    fn focus_row_lies_where_the_tree_draws_it() {
        let groups = vec![view_group(&["a", "b"], false), view_group(&["c"], true), view_group(&["d"], false)];
        let at = |k: &str| focus_row_top(&groups, k, Rows { top_pad: 6., group_row: 24., session_row: 24., group_gap: 0. });
        assert_eq!(at("a"), Some((6. + 24., false)));
        assert_eq!(at("b"), Some((6. + 48., false)));
        assert_eq!(at("c"), Some((6. + 24. * 3., true)), "zugeklappt: die Gruppenzeile");
        assert_eq!(at("d"), Some((6. + 24. * 4. + 24., false)));
        assert_eq!(at("x"), None);
    }

    #[test]
    fn editor_line_goes_through_a_shell() {
        assert_eq!(editor_command_line("  "), None);
        let (program, args) = editor_command_line("code").unwrap();
        assert_eq!(args.last().map(String::as_str), Some("code ."));
        assert!(if cfg!(windows) { program == "cmd" && args[0] == "/C" } else { args[0] == "-lc" });
    }
}
