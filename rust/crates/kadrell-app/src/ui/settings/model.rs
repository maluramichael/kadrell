//! Datenmodell des Einstellungsdialogs: Abschnitte, Zeilen und ihre Steuerelemente als reine Daten. Gebaut wird
//! es aus `Settings` (`sections.rs`), gefiltert, auf Spalten verteilt und gezeichnet wird es aus denselben Daten.
//! Jede Zeile trägt ihren Setter, ein Klick ruft ihn direkt auf, die Änderung gilt sofort.

use std::rc::Rc;

use kadrell_core::Settings;
use kadrell_core::hotkeys::HotkeyAction;

use super::filter::Filter;
use crate::lang::tx;

/// Schreibt einen Wert in die Einstellungen.
pub type Setter<T> = Rc<dyn Fn(&mut Settings, T)>;

/// Farbe eines Infotextes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Normal,
    Accent,
    Muted,
    Error,
}

/// Aktion einer kleinen Taste in einer Zeile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Msg {
    SwitchAccount(String),
    RemoveAccount(String),
    AddAccount,
    Reload(Provider),
    DeleteModel(String),
    /// Ordner oder Datei wählen und in das Textfeld der Zeile mit dieser Id schreiben.
    Browse { row: String, files: bool },
    /// Textfeld der Zeile leeren.
    Clear(String),
    /// Modell laden (Name aus dem Textfeld `row`).
    Pull(String),
    /// Namen (`name:tag` aus einem Suchtreffer) ins Feld „Modell laden“ setzen.
    PickPull(String),
    /// Laufenden Download abbrechen bzw. Fehlermeldung wegnehmen (`provider|name`).
    CancelPull(String),
    /// Empfohlenes Modell einer Funktion wählen, einschalten und laden (`feature` ist der Schlüsselteil, `model` der Name).
    UseSuggested { feature: String, model: String },
}

/// Anbieter lokaler Modelle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    Ollama,
    LlamaCpp,
}

/// Symbol auf einer kleinen Taste statt Text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Folder,
    Image,
    Cross,
}

/// Kleine Taste in einer Zeile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Btn {
    pub label: String,
    pub icon: Option<Icon>,
    pub msg: Msg,
    pub destructive: bool,
    /// Hilfetext beim Überfahren (`.help(...)`).
    pub tip: Option<String>,
}

impl Btn {
    pub fn text(label: &str, msg: Msg) -> Btn {
        Btn { label: tx(label), icon: None, msg, destructive: false, tip: None }
    }

    pub fn icon(icon: Icon, msg: Msg) -> Btn {
        Btn { label: String::new(), icon: Some(icon), msg, destructive: false, tip: None }
    }

    /// Mit Hilfetext.
    pub fn tip(mut self, tip: String) -> Btn {
        self.tip = Some(tip);
        self
    }
}

/// Steuerelement der rechten Spalte einer Zeile.
#[derive(Clone)]
pub enum Control {
    /// Dropdown; `options` sind Paare (Wert, Anzeige).
    Select { options: Vec<(String, String)>, value: String, set: Setter<String> },
    /// Schieberegler in Anzeigeeinheiten (Prozent, Minuten ...), `live: false` übernimmt erst beim Loslassen.
    Slider { value: f64, min: f64, max: f64, step: f64, unit: String, live: bool, set: Setter<f64> },
    /// Textfeld, optional mit Tasten daneben. Ohne `set` ist der Text flüchtig.
    Text { placeholder: String, value: String, set: Option<Setter<String>>, buttons: Vec<Btn> },
    /// Kürzel-Feld.
    Hotkey(HotkeyAction),
    /// Text mit Tasten in der rechten Spalte (Konten, Status).
    Info { text: String, tone: Tone, buttons: Vec<Btn> },
    /// Modellzeile über die ganze Breite: Name der Zeile links, Größe und Taste rechts.
    Model { detail: String, buttons: Vec<Btn> },
    /// Hinweistext über die ganze Breite.
    Note(String),
    /// Download: Name links, Fortschritt oder Fehler und Taste rechts, darunter der Balken (nur ohne Fehler).
    Pull { text: String, tone: Tone, fraction: Option<f64>, buttons: Vec<Btn> },
}

/// Eine Tabellenzeile.
#[derive(Clone)]
pub struct Row {
    /// Stabile Id (Fokus, Textfelder).
    pub id: String,
    pub title: String,
    pub control: Control,
    /// Texte, nach denen der Filter die Zeile findet. Leer: der Filter blendet die Zeile aus, es sei denn, der
    /// Abschnitt passt als Ganzes (Hinweiszeilen, Modellzeilen).
    pub search: Vec<String>,
    /// Hilfetext beim Überfahren des Textes der Zeile (Status der lokalen Modelle).
    pub tip: Option<String>,
    /// Zeigt vor dem Text einen Drehkreis (Status „prüft …“).
    pub busy: bool,
}

impl Row {
    pub fn new(id: &str, title: &str, control: Control) -> Row {
        let title = tx(title);
        Row { id: id.to_string(), title: title.clone(), control, search: vec![title], tip: None, busy: false }
    }

    pub fn with_tip(mut self, tip: String) -> Row {
        self.tip = Some(tip);
        self
    }

    pub fn busy(mut self) -> Row {
        self.busy = true;
        self
    }

    /// Zeile ohne eigenen Suchtext.
    pub fn unsearchable(mut self) -> Row {
        self.search.clear();
        self
    }

    /// Tasten der Zeile in Tab-Reihenfolge.
    pub fn buttons(&self) -> &[Btn] {
        match &self.control {
            Control::Text { buttons, .. } | Control::Info { buttons, .. } | Control::Model { buttons, .. } | Control::Pull { buttons, .. } => buttons,
            _ => &[],
        }
    }

    /// Hat das Steuerelement selbst einen Tab-Stopp (Feld, Dropdown, Regler, Kürzel)? Zeilen aus Text und Tasten haben
    /// nur die Tasten als Stopps (`kbdFocusRing` sitzt an jeder Taste).
    pub fn own_stop(&self) -> bool {
        !matches!(self.control, Control::Info { .. } | Control::Model { .. } | Control::Pull { .. } | Control::Note(_))
    }

    /// Tab-Stopps der Zeile: `None` ist das Steuerelement selbst, `Some(i)` die Taste `i`.
    pub fn stops(&self) -> Vec<Option<usize>> {
        let own = self.own_stop().then_some(None);
        own.into_iter().chain((0..self.buttons().len()).map(Some)).collect()
    }

    /// Nimmt die Zeile die Tastatur an (Tab, Pfeile)?
    pub fn focusable(&self) -> bool {
        match &self.control {
            Control::Note(_) => false,
            Control::Info { buttons, .. } | Control::Model { buttons, .. } | Control::Pull { buttons, .. } => !buttons.is_empty(),
            _ => true,
        }
    }
}

/// Ein Abschnitt: Überschrift mit Hinweis, optional ein Zwischentitel (Kürzel), darunter die Tabelle.
#[derive(Clone)]
pub struct Block {
    /// Überschrift in Versalien mit Hinweis daneben. Der zweite Teil eines Kürzelbereichs hat keine.
    pub heading: Option<(String, String)>,
    pub subtitle: Option<String>,
    pub rows: Vec<Row>,
    /// Passt einer dieser Texte, bleiben alle Zeilen sichtbar.
    pub keep_all: Vec<String>,
}

impl Block {
    /// Indizes der Zeilen, die der Filter stehen lässt. Passt der Abschnitt als Ganzes, alle; sonst die Zeilen,
    /// deren Suchtext passt. Leer heißt: der Abschnitt verschwindet samt Überschrift.
    pub fn visible_rows(&self, filter: &Filter) -> Vec<usize> {
        let all = || (0..self.rows.len()).collect();
        if !filter.is_active() {
            return all();
        }
        let keys: Vec<&str> = self.keep_all.iter().map(String::as_str).collect();
        if filter.keeps(&keys) {
            return all();
        }
        let hit = |r: &Row| !r.search.is_empty() && filter.keeps(&r.search.iter().map(String::as_str).collect::<Vec<_>>());
        self.rows.iter().enumerate().filter(|(_, r)| hit(r)).map(|(i, _)| i).collect()
    }

    /// Geschätzte Höhe für die Teilung in Spalten.
    pub fn estimated_height(&self, rows: usize) -> f32 {
        super::layout::estimated_height(rows, self.heading.is_some(), self.subtitle.is_some())
    }
}

/// Zeilen, die nach dem Filter in allen Abschnitten bleiben, zusammen mit ihrem Abschnitt.
pub fn visible(blocks: &[Block], filter: &Filter) -> Vec<(usize, Vec<usize>)> {
    blocks.iter().enumerate().map(|(i, b)| (i, b.visible_rows(filter))).filter(|(_, rows)| !rows.is_empty()).collect()
}

/// Anzahl der Treffer (nur sinnvoll bei aktivem Filter).
pub fn hit_count(blocks: &[Block], filter: &Filter) -> usize {
    visible(blocks, filter).iter().map(|(_, rows)| rows.len()).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(id: &str, title: &str) -> Row {
        Row::new(id, title, Control::Note(String::new()))
    }

    /// Swift: `kbdFocusRing` sitzt an jeder Taste, ein Textfeld mit Tasten hat das Feld und danach jede Taste als Stopp.
    #[test]
    fn buttons_are_their_own_tab_stops() {
        let pick = Btn::icon(Icon::Folder, Msg::Browse { row: "f".into(), files: false });
        let clear = Btn::icon(Icon::Cross, Msg::Clear("f".into()));
        let field = Row::new("f", "Ordner", Control::Text { placeholder: String::new(), value: String::new(), set: None, buttons: vec![pick.clone(), clear.clone()] });
        assert_eq!(field.stops(), vec![None, Some(0), Some(1)]);
        let status = Row::new("s", "Status", Control::Info { text: String::new(), tone: Tone::Normal, buttons: vec![pick] });
        assert_eq!(status.stops(), vec![Some(0)]);
        assert_eq!(info("n", "Hinweis").stops(), Vec::<Option<usize>>::new());
    }

    fn block(title: &str, note: &str, rows: &[&str]) -> Block {
        Block {
            heading: Some((title.to_string(), note.to_string())),
            subtitle: None,
            rows: rows.iter().map(|r| info(r, r)).collect(),
            keep_all: vec![title.to_string(), note.to_string()],
        }
    }

    fn active(q: &str) -> Filter {
        Filter { open: true, query: q.to_string() }
    }

    #[test]
    fn without_filter_all_rows_stay() {
        let b = block("Claude", "", &["Modell", "Effort"]);
        assert_eq!(b.visible_rows(&Filter::default()), vec![0, 1]);
    }

    #[test]
    fn matching_title_or_note_keeps_every_row() {
        let b = block("Konten", "mehrere Claude-Accounts", &["Wechseln", "Entfernen"]);
        assert_eq!(b.visible_rows(&active("konten")), vec![0, 1]);
        assert_eq!(b.visible_rows(&active("claude-accounts")), vec![0, 1]);
    }

    #[test]
    fn otherwise_only_rows_with_a_matching_title() {
        let b = block("Claude", "", &["Startmodus", "Modell", "Effort"]);
        assert_eq!(b.visible_rows(&active("modell")), vec![1]);
        assert_eq!(b.visible_rows(&active("o")), vec![0, 1, 2]);
    }

    #[test]
    fn a_section_without_hits_disappears() {
        let blocks = vec![block("Claude", "", &["Modell"]), block("Updates", "", &["Nach Updates suchen"])];
        let shown = visible(&blocks, &active("updates"));
        assert_eq!(shown, vec![(1, vec![0])]);
        assert_eq!(hit_count(&blocks, &active("zzz")), 0);
        assert_eq!(hit_count(&blocks, &active("modell")), 1);
    }

    #[test]
    fn rows_without_search_text_only_show_with_their_section() {
        let mut b = block("Lokale Modelle", "", &["Status"]);
        b.rows.push(info("m1", "llama").unsearchable());
        assert_eq!(b.visible_rows(&active("status")), vec![0]);
        assert_eq!(b.visible_rows(&active("lokale")), vec![0, 1]);
    }

    #[test]
    fn hotkey_rows_are_found_by_their_shortcut() {
        let mut b = Block { heading: None, subtitle: Some("Navigation".into()), rows: vec![], keep_all: vec!["Navigation".into(), "Tastenkürzel".into()] };
        let mut r = info("focusLeft", "Fokus links");
        r.search.push("Alt+←".into());
        b.rows.push(r);
        b.rows.push(info("focusRight", "Fokus rechts"));
        assert_eq!(b.visible_rows(&active("alt")), vec![0]);
        assert_eq!(b.visible_rows(&active("tastenkürzel")), vec![0, 1]);
    }
}
