//! Das Fenster: links der Baum Gruppe › Session, rechts die fokussierte Session als Terminal, unten eine Zeile mit
//! Hinweisen. Bedienung wie tmux: Präfix Strg+B, danach eine Taste; dazu Alt+N/Alt+P und Alt+1…9 wie die
//! Standardkürzel der Mac-App. Alles andere geht an das Terminal.

use crate::app::App;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

pub const SIDEBAR: u16 = 32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    Terminal,
    Prefix,
    /// ↑↓ blättert durch den Baum und zeigt die Session, ⏎ oder Esc zurück ins Terminal.
    Tree,
    Rename(String),
    Confirm(Confirm),
    Help,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Confirm {
    Kill(String),
    Quit,
}

pub struct Ui {
    pub mode: Mode,
    pub quit: bool,
    pub sidebar: bool,
    pub notice: Option<String>,
}

impl Ui {
    pub fn new() -> Ui {
        Ui { mode: Mode::Terminal, quit: false, sidebar: true, notice: None }
    }
}

/// Fläche der Kachel innen (ohne Rahmen) bei dieser Fenstergröße.
pub fn tile_area(full: Rect, sidebar: bool) -> Rect {
    let [main, _] = Layout::vertical([Constraint::Min(3), Constraint::Length(1)]).areas(full);
    let tile = if sidebar && main.width > SIDEBAR + 20 {
        Layout::horizontal([Constraint::Length(SIDEBAR), Constraint::Min(10)]).areas::<2>(main)[1]
    } else {
        main
    };
    Block::default().borders(Borders::ALL).inner(tile)
}

pub fn draw(f: &mut Frame, app: &App, ui: &Ui) {
    let [main, status] = Layout::vertical([Constraint::Min(3), Constraint::Length(1)]).areas(f.area());
    let tile = if ui.sidebar && main.width > SIDEBAR + 20 {
        let [tree, tile] = Layout::horizontal([Constraint::Length(SIDEBAR), Constraint::Min(10)]).areas(main);
        draw_tree(f, app, ui, tree);
        tile
    } else {
        main
    };
    draw_tile(f, app, ui, tile);
    draw_status(f, ui, status);
    match &ui.mode {
        Mode::Help => draw_popup(f, "Hilfe", HELP),
        Mode::Rename(text) => draw_popup(f, "Umbenennen (⏎ übernimmt, leer = Titel von Claude Code)", &format!("{text}▏")),
        Mode::Confirm(Confirm::Kill(key)) => draw_popup(f, "Session beenden?", &format!("„{}“ beenden und entfernen? j/n", title(app, key))),
        Mode::Confirm(Confirm::Quit) => draw_popup(f, "Kadrell beenden?", "Alle Sessions enden, beim nächsten Start geht es mit --resume weiter. j/n"),
        _ => {}
    }
}

fn title(app: &App, key: &str) -> String {
    app.store.session(key).map(|s| s.title()).unwrap_or_default()
}

fn parse_hex(c: &str) -> Color {
    let hex = c.trim_start_matches('#');
    match (u8::from_str_radix(hex.get(0..2).unwrap_or("ff"), 16), u8::from_str_radix(hex.get(2..4).unwrap_or("ff"), 16), u8::from_str_radix(hex.get(4..6).unwrap_or("ff"), 16)) {
        (Ok(r), Ok(g), Ok(b)) => Color::Rgb(r, g, b),
        _ => Color::White,
    }
}

fn status_glyph(status: &str) -> (&'static str, Color) {
    match status {
        "running" => ("●", Color::Yellow),
        "waiting" => ("◆", Color::LightRed),
        "error" => ("✕", Color::Red),
        "idle" => ("○", Color::Green),
        "ended" => ("×", Color::DarkGray),
        _ => ("·", Color::DarkGray),
    }
}

fn draw_tree(f: &mut Frame, app: &App, ui: &Ui, area: Rect) {
    let mut lines = Vec::new();
    let mut number = 0;
    for g in &app.store.groups {
        let fav = if g.favorite { " ★" } else { "" };
        lines.push(Line::from(Span::styled(format!("▾ {}{fav}", g.name), Style::default().fg(parse_hex(&g.color)).add_modifier(Modifier::BOLD))));
        for s in g.session_ids.iter().filter_map(|k| app.store.session(k)) {
            number += 1;
            let (glyph, color) = status_glyph(app.status_of(s));
            let focused = app.focused.as_deref() == Some(s.id.as_str());
            let style = if focused { Style::default().add_modifier(Modifier::REVERSED) } else { Style::default() };
            let label = if number <= 9 { format!("{number} ") } else { "  ".into() };
            lines.push(Line::from(vec![Span::raw("  "), Span::styled(glyph, Style::default().fg(color)), Span::styled(format!(" {label}{}", s.title()), style)]));
        }
    }
    if lines.is_empty() {
        lines.push(Line::from(Span::styled("Keine Sessions. Strg+B c startet eine.", Style::default().fg(Color::DarkGray))));
    }
    let border = if ui.mode == Mode::Tree { Color::Cyan } else { Color::DarkGray };
    let block = Block::default().borders(Borders::ALL).border_style(Style::default().fg(border)).title(" Kadrell ");
    f.render_widget(Paragraph::new(lines).block(block), area);
}

fn draw_tile(f: &mut Frame, app: &App, ui: &Ui, area: Rect) {
    let key = app.focused.clone().unwrap_or_default();
    let session = app.store.session(&key);
    let header = session.map(|s| format!(" {} · {} ", s.title(), app.status_of(s))).unwrap_or_else(|| " keine Session ".into());
    let border = if ui.mode == Mode::Terminal { Color::Cyan } else { Color::DarkGray };
    let block = Block::default().borders(Borders::ALL).border_style(Style::default().fg(border)).title(header);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let Some(tile) = app.tiles.get(&key) else {
        let hint = if session.is_some() { "Startet beim Fokussieren. Strg+B r startet neu." } else { "" };
        f.render_widget(Paragraph::new(hint).style(Style::default().fg(Color::DarkGray)), inner);
        return;
    };
    let Ok(parser) = tile.pane.parser.lock() else { return };
    let screen = parser.screen();
    f.render_widget(Paragraph::new(screen_lines(screen, inner)), inner);
    if !tile.alive {
        let note = Rect { x: inner.x, y: inner.y + inner.height.saturating_sub(1), width: inner.width, height: 1.min(inner.height) };
        f.render_widget(Paragraph::new(" beendet · Strg+B r startet neu ").style(Style::default().bg(Color::DarkGray).fg(Color::White)), note);
    } else if ui.mode == Mode::Terminal && !screen.hide_cursor() {
        let (row, col) = screen.cursor_position();
        f.set_cursor_position((inner.x + col.min(inner.width.saturating_sub(1)), inner.y + row.min(inner.height.saturating_sub(1))));
    }
}

/// Bildschirm des VT-Parsers als Zeilen; Folgezellen breiter Zeichen fallen weg, die zeichnet ratatui selbst.
fn screen_lines(screen: &vt100::Screen, area: Rect) -> Vec<Line<'static>> {
    let (rows, cols) = screen.size();
    (0..rows.min(area.height))
        .map(|row| {
            let spans: Vec<Span> = (0..cols.min(area.width))
                .filter_map(|col| screen.cell(row, col))
                .filter(|c| !c.is_wide_continuation())
                .map(|c| {
                    let text = if c.has_contents() { c.contents().to_string() } else { " ".to_string() };
                    Span::styled(text, cell_style(c))
                })
                .collect();
            Line::from(spans)
        })
        .collect()
}

fn cell_style(c: &vt100::Cell) -> Style {
    let mut style = Style::default().fg(color(c.fgcolor())).bg(color(c.bgcolor()));
    for (on, m) in [(c.bold(), Modifier::BOLD), (c.dim(), Modifier::DIM), (c.italic(), Modifier::ITALIC), (c.underline(), Modifier::UNDERLINED), (c.inverse(), Modifier::REVERSED)] {
        if on {
            style = style.add_modifier(m);
        }
    }
    style
}

fn color(c: vt100::Color) -> Color {
    match c {
        vt100::Color::Default => Color::Reset,
        vt100::Color::Idx(i) => Color::Indexed(i),
        vt100::Color::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}

fn draw_status(f: &mut Frame, ui: &Ui, area: Rect) {
    let text = match (&ui.notice, &ui.mode) {
        (Some(n), _) => n.clone(),
        (_, Mode::Prefix) => "Präfix: n/p nächste/vorige · 1…9 direkt · c neu · x beenden · s stop · r neu starten · , umbenennen · w Baum · b Baum ein/aus · q Ende · ? Hilfe".into(),
        (_, Mode::Tree) => "Baum: ↑↓ zeigt die Session · ⏎/Esc zurück ins Terminal · x beenden · , umbenennen".into(),
        _ => "Strg+B ? Hilfe · Alt+N/Alt+P nächste/vorige · Alt+1…9 direkt".into(),
    };
    f.render_widget(Paragraph::new(text).style(Style::default().fg(Color::DarkGray)), area);
}

fn draw_popup(f: &mut Frame, title: &str, body: &str) {
    let area = f.area();
    let width = area.width.saturating_sub(8).min(84);
    let height = (body.lines().count() as u16 + 2).min(area.height.saturating_sub(2));
    let rect = Rect { x: area.x + (area.width - width) / 2, y: area.y + (area.height.saturating_sub(height)) / 3, width, height };
    f.render_widget(Clear, rect);
    let block = Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::Cyan)).title(format!(" {title} "));
    f.render_widget(Paragraph::new(body.to_string()).wrap(Wrap { trim: false }).block(block), rect);
}

const HELP: &str = "Strg+B ist das Präfix (wie tmux), danach:
  n / p        nächste / vorige Session
  1 … 9        Session direkt (Nummer im Baum)
  c            neue Session im Ordner der fokussierten
  x            fokussierte Session beenden und entfernen
  s / r        Claude stoppen / neu starten, die Kachel bleibt
  ,            umbenennen
  w            Tastatur in den Baum (↑↓ zeigt, ⏎ zurück)
  b            Baum ein- und ausblenden
  q            Kadrell beenden (Sessions setzen beim nächsten Start fort)
  Strg+B       ein Strg+B an das Terminal schicken
Ohne Präfix: Alt+N / Alt+P nächste / vorige, Alt+1 … 9 direkt.
Von außen steuern: kadrell help";

/// Taste als Bytes für das Terminal, wie xterm sie schickt.
pub fn key_bytes(key: &KeyEvent, app_cursor: bool) -> Option<Vec<u8>> {
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);
    let modifier = 1 + u8::from(shift) + 2 * u8::from(alt) + 4 * u8::from(ctrl);
    let bytes: Vec<u8> = match key.code {
        KeyCode::Char(c) if ctrl => vec![control_byte(c)?],
        KeyCode::Char(c) => c.to_string().into_bytes(),
        KeyCode::Enter if shift || alt => return Some(b"\x1b\r".to_vec()),
        KeyCode::Enter => b"\r".to_vec(),
        KeyCode::Tab => b"\t".to_vec(),
        KeyCode::BackTab => return Some(b"\x1b[Z".to_vec()),
        KeyCode::Backspace if ctrl => vec![0x08],
        KeyCode::Backspace => vec![0x7f],
        KeyCode::Esc => vec![0x1b],
        KeyCode::Up | KeyCode::Down | KeyCode::Right | KeyCode::Left | KeyCode::Home | KeyCode::End => return Some(cursor_key(key.code, modifier, app_cursor)),
        KeyCode::PageUp => tilde(5, modifier),
        KeyCode::PageDown => tilde(6, modifier),
        KeyCode::Insert => tilde(2, modifier),
        KeyCode::Delete => tilde(3, modifier),
        KeyCode::F(n) => return function_key(n, modifier),
        _ => return None,
    };
    let special = matches!(key.code, KeyCode::PageUp | KeyCode::PageDown | KeyCode::Insert | KeyCode::Delete);
    Some(if alt && !special { [vec![0x1b], bytes].concat() } else { bytes })
}

fn control_byte(c: char) -> Option<u8> {
    match c.to_ascii_lowercase() {
        l @ 'a'..='z' => Some(l as u8 - b'a' + 1),
        ' ' | '@' | '2' => Some(0),
        '[' | '3' => Some(0x1b),
        '\\' | '4' => Some(0x1c),
        ']' | '5' => Some(0x1d),
        '^' | '6' => Some(0x1e),
        '_' | '-' | '7' => Some(0x1f),
        _ => None,
    }
}

fn cursor_key(code: KeyCode, modifier: u8, app_cursor: bool) -> Vec<u8> {
    let letter = match code {
        KeyCode::Up => 'A',
        KeyCode::Down => 'B',
        KeyCode::Right => 'C',
        KeyCode::Left => 'D',
        KeyCode::Home => 'H',
        _ => 'F',
    };
    match (modifier, app_cursor) {
        (1, true) => format!("\x1bO{letter}"),
        (1, false) => format!("\x1b[{letter}"),
        (m, _) => format!("\x1b[1;{m}{letter}"),
    }
    .into_bytes()
}

fn tilde(n: u8, modifier: u8) -> Vec<u8> {
    if modifier == 1 { format!("\x1b[{n}~") } else { format!("\x1b[{n};{modifier}~") }.into_bytes()
}

fn function_key(n: u8, modifier: u8) -> Option<Vec<u8>> {
    let code = match n {
        1..=4 if modifier == 1 => return Some(format!("\x1bO{}", (b'P' + n - 1) as char).into_bytes()),
        1..=4 => return Some(format!("\x1b[1;{modifier}{}", (b'P' + n - 1) as char).into_bytes()),
        5 => 15,
        6..=10 => 11 + n,
        11 | 12 => 12 + n,
        _ => return None,
    };
    Some(tilde(code, modifier))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode, m: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, m)
    }

    #[test]
    fn keys_like_xterm() {
        let none = KeyModifiers::NONE;
        assert_eq!(key_bytes(&key(KeyCode::Char('c'), KeyModifiers::CONTROL), false).unwrap(), [3]);
        assert_eq!(key_bytes(&key(KeyCode::Char('x'), KeyModifiers::ALT), false).unwrap(), b"\x1bx");
        assert_eq!(key_bytes(&key(KeyCode::Up, none), false).unwrap(), b"\x1b[A");
        assert_eq!(key_bytes(&key(KeyCode::Up, none), true).unwrap(), b"\x1bOA");
        assert_eq!(key_bytes(&key(KeyCode::Left, KeyModifiers::CONTROL), true).unwrap(), b"\x1b[1;5D");
        assert_eq!(key_bytes(&key(KeyCode::Enter, KeyModifiers::SHIFT), false).unwrap(), b"\x1b\r");
        assert_eq!(key_bytes(&key(KeyCode::F(5), none), false).unwrap(), b"\x1b[15~");
        assert_eq!(key_bytes(&key(KeyCode::F(12), none), false).unwrap(), b"\x1b[24~");
        assert_eq!(key_bytes(&key(KeyCode::F(1), none), false).unwrap(), b"\x1bOP");
        assert_eq!(key_bytes(&key(KeyCode::Char('ä'), none), false).unwrap(), "ä".as_bytes());
    }

    #[test]
    fn tile_area_leaves_room_for_tree_and_status() {
        let area = tile_area(Rect::new(0, 0, 120, 40), true);
        assert_eq!(area, Rect::new(SIDEBAR + 1, 1, 120 - SIDEBAR - 2, 37));
        assert_eq!(tile_area(Rect::new(0, 0, 40, 20), true).width, 38, "schmal: ohne Baum");
    }
}
