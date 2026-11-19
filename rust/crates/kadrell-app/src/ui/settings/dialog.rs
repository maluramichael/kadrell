//! Zustand und Verhalten des Einstellungsdialogs (`SettingsView.swift`): Filter, Tastaturzeile, Dropdown,
//! Schieberegler, Kürzel-Aufnahme. Gezeichnet wird in `view.rs`. Jede Änderung geht sofort in die Einstellungen
//! (`store`), der Dialog hält keinen eigenen Entwurf außer dem Wert eines Reglers, der erst beim Loslassen gilt.

use kadrell_i18n::trd;
use std::collections::HashMap;

use gpui::{
    App, AppContext, Context, Entity, EventEmitter, FocusHandle, Focusable, KeyDownEvent, Pixels, ScrollHandle,
    Subscription, Window, point, px,
};
use kadrell_core::Settings;
use kadrell_core::hotkeys::{Hotkey, HotkeyAction};

use super::backend::Backend;
use super::controls::Geometry;
use super::filter::Filter;
use super::model::{Block, Btn, Control, Msg, Provider, Row, Setter};
use super::paths;
use super::sections::{self, Context as BuildContext};
use super::shortcut::{self, KeyInput, Recorded};
use super::store::{self, AppSettings};
use crate::ThemeGlobal;
use crate::theme::Theme;
use crate::ui::folders;
use crate::ui::text_input::{TextInput, TextInputEvent};

/// Pause nach der letzten Eingabe in einem Adressfeld, bevor der Anbieter neu geprüft wird.
const RECHECK_DELAY: std::time::Duration = std::time::Duration::from_millis(700);

pub enum SettingsEvent {
    Close,
    /// Die Kopie der Einstellungen (`store`) hat sich geändert.
    Changed,
}

impl EventEmitter<SettingsEvent> for SettingsDialog {}

/// Offene Auswahlliste eines Dropdowns.
pub struct Popover {
    pub row: String,
    pub options: Vec<(String, String)>,
    pub value: String,
    pub set: Setter<String>,
    /// Markierte Zeile der Liste.
    pub cursor: usize,
}

/// Regler, der gerade gezogen wird.
pub struct Drag {
    pub row: String,
    pub min: f64,
    pub max: f64,
    pub step: f64,
    pub live: bool,
    pub set: Setter<f64>,
}

pub struct SettingsDialog {
    pub(super) focus: FocusHandle,
    pub(super) backend: Box<dyn Backend>,
    pub(super) filter: Filter,
    pub(super) filter_input: Entity<TextInput>,
    pub(super) inputs: HashMap<String, Entity<TextInput>>,
    pub(super) geometry: Geometry,
    /// Breite und Abschnittshöhen, mit denen der letzte Frame die Spalten verteilt hat.
    pub(super) layout_basis: (f32, Vec<f32>),
    pub(super) scroll: ScrollHandle,
    pub(super) pop_scroll: ScrollHandle,
    pub(super) popover: Option<Popover>,
    /// Zeile, die die Tastatur hat (Tab, Pfeile).
    pub(super) cursor: Option<String>,
    pub(super) recording: Option<HotkeyAction>,
    pub(super) drag: Option<Drag>,
    /// Wert des gezogenen Reglers, solange er noch nicht übernommen ist.
    pub(super) draft: Option<(String, f64)>,
    /// Modell, dessen „Löschen“ schon einmal gedrückt wurde.
    pub(super) armed: Option<String>,
    /// Dropdown, das der letzte Mausdruck geschlossen hat: der folgende Klick auf dasselbe Feld öffnet es nicht neu.
    pub(super) just_closed: Option<String>,
    /// Fokus der Haupttaste „Fertig“ (letzter Tab-Stopp nach den Zeilen), aus dem letzten Frame.
    pub(super) foot_focus: Option<FocusHandle>,
    record_sub: Option<Subscription>,
    /// Zählt mit jedem Tippen im Suchfeld der Modelle hoch: nur die letzte Eingabe sucht (nach kurzer Pause).
    pub(super) search_token: u64,
    /// Wie `search_token` für die Neuprüfung der lokalen Anbieter nach einer Eingabe in Adressfeldern.
    reload_token: u64,
    _subs: Vec<Subscription>,
}

impl SettingsDialog {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        super::bind_keys(cx);
        let filter_input = cx.new(|cx| TextInput::new(&trd!("Filtern …"), cx).compact());
        let filter_sub = cx.subscribe(&filter_input, |this, input, _: &TextInputEvent, cx| {
            this.filter.query = input.read(cx).text().to_string();
            cx.notify();
        });
        let mut this = Self {
            focus: cx.focus_handle(),
            backend: super::backend::default_backend(cx),
            filter: Filter::default(),
            filter_input,
            inputs: HashMap::new(),
            geometry: Geometry::default(),
            layout_basis: (0., Vec::new()),
            scroll: ScrollHandle::new(),
            pop_scroll: ScrollHandle::new(),
            popover: None,
            cursor: None,
            recording: None,
            drag: None,
            draft: None,
            armed: None,
            just_closed: None,
            foot_focus: None,
            record_sub: None,
            search_token: 0,
            reload_token: 0,
            _subs: vec![filter_sub],
        };
        this.focus.focus(window, cx);
        this.start_model_sync(cx);
        this
    }

    /// Beim Öffnen die lokalen Anbieter neu lesen und danach neu zeichnen, sobald das Backend im Hintergrund etwas
    /// ändert (Status, Modelle, Downloads, Suche). Nichts davon läuft beim Zeichnen.
    fn start_model_sync(&mut self, cx: &mut Context<Self>) {
        self.backend.reload(Provider::Ollama);
        self.backend.reload(Provider::LlamaCpp);
        cx.spawn(async move |this, cx| {
            let mut seen = u64::MAX;
            loop {
                cx.background_executor().timer(std::time::Duration::from_millis(250)).await;
                let alive = this.update(cx, |this, cx| {
                    let now = this.backend.generation();
                    if now != seen {
                        seen = now;
                        cx.notify();
                    }
                });
                if alive.is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    // MARK: Daten

    /// Alle Abschnitte aus den aktuellen Einstellungen.
    pub(super) fn blocks(&self, cx: &App) -> Vec<Block> {
        let settings = &cx.global::<AppSettings>().settings;
        let theme_id = cx.global::<ThemeGlobal>().0.id;
        sections::build(&BuildContext { settings, backend: self.backend.as_ref(), theme_id, platform: shortcut::current() }, self.armed.as_deref())
    }

    pub(super) fn find_row(&self, id: &str, cx: &App) -> Option<Row> {
        self.blocks(cx).into_iter().flat_map(|b| b.rows).find(|r| r.id == id)
    }

    /// Zeilen, die Tab und Pfeile erreichen, in Anzeigereihenfolge (nach dem Filter).
    fn focusable_rows(&self, cx: &App) -> Vec<String> {
        let blocks = self.blocks(cx);
        let shown = super::model::visible(&blocks, &self.filter);
        shown.into_iter().flat_map(|(b, rows)| rows.into_iter().map(move |r| (b, r))).map(|(b, r)| &blocks[b].rows[r]).filter(|r| r.focusable()).map(|r| r.id.clone()).collect()
    }

    // MARK: Änderungen

    /// Schreibt in die Einstellungen und zieht danach das Farbschema nach. Alles gilt sofort.
    pub(super) fn change(&mut self, f: impl FnOnce(&mut Settings), cx: &mut Context<Self>) {
        store::update(cx, f);
        sync_theme(cx);
        cx.emit(SettingsEvent::Changed);
        cx.notify();
    }

    pub(super) fn input_changed(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(input) = self.inputs.get(id).cloned() else { return };
        let text = input.read(cx).text().to_string();
        if id == sections::SEARCH_ROW {
            return self.search_later(text, cx);
        }
        if let Some(Row { control: Control::Text { set: Some(set), .. }, .. }) = self.find_row(id, cx) {
            self.change(|s| set(s, text), cx);
        }
        self.recheck_later(id, cx);
    }

    /// Adresse oder Pfad eines lokalen Anbieters geändert: nach 0,7 s ohne weitere Eingabe neu prüfen.
    fn recheck_later(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(provider) = sections::provider_of(id) else { return };
        self.reload_token += 1;
        let token = self.reload_token;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(RECHECK_DELAY).await;
            this.update(cx, |this, cx| {
                if this.reload_token == token {
                    this.backend.reload(provider);
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// Modellsuche auf ollama.com, eine halbe Sekunde nach dem letzten Tastendruck.
    fn search_later(&mut self, query: String, cx: &mut Context<Self>) {
        self.search_token += 1;
        let token = self.search_token;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(std::time::Duration::from_millis(500)).await;
            this.update(cx, |this, cx| {
                if this.search_token == token {
                    this.backend.search(&query);
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// Text eines Eingabefeldes der Zeile `id`.
    fn input_text(&self, id: &str, cx: &App) -> String {
        self.inputs.get(id).map(|i| i.read(cx).text().trim().to_string()).unwrap_or_default()
    }

    /// Legt für jede Textzeile ein Eingabefeld an, einmal; danach gehört der Text dem Feld.
    pub(super) fn ensure_inputs(&mut self, blocks: &[Block], window: &mut Window, cx: &mut Context<Self>) {
        for row in blocks.iter().flat_map(|b| &b.rows) {
            let Control::Text { placeholder, value, .. } = &row.control else { continue };
            if self.inputs.contains_key(&row.id) {
                continue;
            }
            let input = cx.new(|cx| TextInput::new(placeholder, cx).compact());
            input.update(cx, |i, cx| i.set_text(value, window, cx));
            let id = row.id.clone();
            self._subs.push(cx.subscribe(&input, move |this, _, _: &TextInputEvent, cx| this.input_changed(&id, cx)));
            self.inputs.insert(row.id.clone(), input);
        }
    }

    fn set_input_text(&mut self, id: &str, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(input) = self.inputs.get(id).cloned() {
            input.update(cx, |i, cx| i.set_text(text, window, cx));
        }
    }

    /// Eine kleine Taste in einer Zeile.
    pub(super) fn press(&mut self, btn: &Btn, window: &mut Window, cx: &mut Context<Self>) {
        match &btn.msg {
            Msg::SwitchAccount(id) => self.backend.switch_account(id),
            Msg::RemoveAccount(id) => self.backend.remove_account(id),
            Msg::AddAccount => self.backend.add_account(),
            Msg::Reload(p) => self.backend.reload(*p),
            Msg::DeleteModel(id) => self.delete_model(id),
            Msg::Browse { row, files } => self.browse(row, *files, window, cx),
            Msg::Clear(row) => self.set_input_text(row, "", window, cx),
            Msg::Pull(row) => self.start_pull(row, window, cx),
            Msg::PickPull(name) => self.set_input_text(sections::PULL_ROW, name, window, cx),
            Msg::CancelPull(id) => self.backend.cancel_pull(id),
            Msg::UseSuggested { feature, model } => self.use_suggested(feature, model, cx),
        }
        cx.notify();
    }

    /// „Laden“: der Name aus dem Feld geht an den Anbieter, das Feld wird leer.
    fn start_pull(&mut self, row: &str, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.input_text(row, cx);
        if !name.is_empty() {
            self.backend.pull(&name);
            self.set_input_text(row, "", window, cx);
        }
    }

    /// Empfohlenes Modell: für die Funktion wählen, Funktion einschalten, Modell laden.
    fn use_suggested(&mut self, feature: &str, model: &str, cx: &mut Context<Self>) {
        let (on_key, model_key) = (format!("local.feature.{feature}.enabled"), format!("local.feature.{feature}.model"));
        let value = format!("ollama|{model}");
        self.change(|s| {
            s.set(&model_key, serde_json::Value::String(value));
            s.set(&on_key, serde_json::Value::Bool(true));
        }, cx);
        self.backend.pull(model);
    }

    /// Zweistufig: der erste Druck markiert das Modell, der zweite löscht es.
    fn delete_model(&mut self, id: &str) {
        if self.armed.as_deref() == Some(id) {
            self.backend.delete_model(id);
            self.armed = None;
        } else {
            self.armed = Some(id.to_string());
        }
    }

    /// Ordner oder Datei im Systemdialog wählen und ins Feld der Zeile schreiben.
    fn browse(&mut self, row: &str, files: bool, window: &mut Window, cx: &mut Context<Self>) {
        // Wie `chooseFolder(start:images:)`: Ordner, oder mit `files` genau eine Bilddatei, Titel „Wählen“.
        let row = row.to_string();
        crate::ui::pick::choose_with(files, window, cx, move |this, path, window, cx| {
            let text = folders::short_path(&path.replace('\\', "/"), &folders::home_dir());
            this.set_input_text(&row, &text, window, cx);
        });
    }

    // MARK: Dropdown

    pub(super) fn open_select(&mut self, row: &Row, window: &mut Window, cx: &mut Context<Self>) {
        let Control::Select { options, value, set } = &row.control else { return };
        if self.just_closed.as_deref() == Some(row.id.as_str()) {
            self.just_closed = None;
            return;
        }
        self.focus.focus(window, cx);
        let cursor = options.iter().position(|(v, _)| v == value).unwrap_or(0);
        self.pop_scroll.scroll_to_item(cursor);
        self.popover = Some(Popover { row: row.id.clone(), options: options.clone(), value: value.clone(), set: set.clone(), cursor });
        self.cursor = Some(row.id.clone());
        cx.notify();
    }

    pub(super) fn close_popover(&mut self, cx: &mut Context<Self>) {
        if self.popover.take().is_some() {
            cx.notify();
        }
    }

    /// Mausdruck irgendwo im Dialog: eine offene Liste schließt, und der folgende Klick auf ihr Feld öffnet sie nicht neu.
    pub(super) fn press_outside(&mut self, cx: &mut Context<Self>) {
        self.just_closed = self.popover.as_ref().map(|p| p.row.clone());
        self.close_popover(cx);
    }

    pub(super) fn pick(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(p) = self.popover.take() else { return };
        if let Some((value, _)) = p.options.get(index) {
            let value = value.clone();
            self.change(|s| (p.set)(s, value), cx);
            // An- und Ausschalten eines Anbieters prüft sofort neu.
            if let Some(provider) = sections::provider_of(&p.row) {
                self.backend.reload(provider);
            }
        }
        cx.notify();
    }

    pub(super) fn move_popover(&mut self, delta: isize, cx: &mut Context<Self>) {
        let Some(p) = self.popover.as_mut() else { return };
        p.cursor = (p.cursor as isize + delta).clamp(0, p.options.len().saturating_sub(1) as isize) as usize;
        self.pop_scroll.scroll_to_item(p.cursor);
        cx.notify();
    }

    pub(super) fn hover_option(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(p) = self.popover.as_mut().filter(|p| p.cursor != index) {
            p.cursor = index;
            cx.notify();
        }
    }

    // MARK: Regler

    pub(super) fn begin_drag(&mut self, row: &Row, x: Pixels, cx: &mut Context<Self>) {
        let Control::Slider { min, max, step, live, set, .. } = &row.control else { return };
        self.press_outside(cx);
        self.drag = Some(Drag { row: row.id.clone(), min: *min, max: *max, step: *step, live: *live, set: set.clone() });
        self.drag_to(x, cx);
    }

    pub(super) fn drag_to(&mut self, x: Pixels, cx: &mut Context<Self>) {
        let Some(d) = self.drag.as_ref() else { return };
        let Some(track) = self.geometry.borrow().get(&format!("track:{}", d.row)).copied() else { return };
        let value = slider_value(f32::from(x - track.left()) / f32::from(track.size.width).max(1.), d.min, d.max, d.step);
        let (row, live, set) = (d.row.clone(), d.live, d.set.clone());
        if live {
            self.change(|s| set(s, value), cx);
        } else {
            self.draft = Some((row, value));
            cx.notify();
        }
    }

    /// Maus losgelassen: ein Regler ohne Live-Wert übernimmt jetzt.
    pub(super) fn end_drag(&mut self, cx: &mut Context<Self>) {
        let Some(d) = self.drag.take() else { return };
        if let Some((row, value)) = self.draft.take().filter(|(row, _)| *row == d.row) {
            let _ = row;
            self.change(|s| (d.set)(s, value), cx);
        }
    }

    /// Pfeil links und rechts am fokussierten Regler: ein Schritt.
    pub(super) fn nudge(&mut self, direction: f64, cx: &mut Context<Self>) {
        let Some(row) = self.cursor.as_deref().and_then(|id| self.find_row(id, cx)) else { return };
        let Control::Slider { value, min, max, step, set, .. } = row.control else { return };
        let next = (value + direction * step).clamp(min, max);
        self.change(|s| set(s, next), cx);
    }

    // MARK: Kürzel

    pub(super) fn start_recording(&mut self, action: HotkeyAction, cx: &mut Context<Self>) {
        self.stop_recording();
        self.recording = Some(action);
        let this = cx.entity().downgrade();
        // Vor allen Kürzeln der App und vor den Aktionen: sonst löst Alt+← oder Strg+N aus, statt aufgenommen zu werden.
        self.record_sub = Some(cx.intercept_keystrokes(move |ev, window, cx| {
            if ev.keystroke.is_modifier_key() {
                return;
            }
            cx.stop_propagation();
            this.update(cx, |d, cx| d.record_key(&ev.keystroke.modifiers, &ev.keystroke.key, window, cx)).ok();
        }));
        cx.notify();
    }

    pub(super) fn stop_recording(&mut self) {
        self.recording = None;
        self.record_sub = None;
    }

    fn record_key(&mut self, m: &gpui::Modifiers, key: &str, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(action) = self.recording else { return };
        let input = KeyInput { control: m.control, alt: m.alt, shift: m.shift, platform: m.platform, key: key.to_string() };
        let recorded = shortcut::classify_for(&input, shortcut::current(), Some(action));
        match recorded {
            // Swift: `guard let k = Hotkey(event:), k.isUsable else { NSSound.beep(); return nil }`, die Aufnahme läuft weiter.
            Recorded::Reject => return kadrell_tray::beep(),
            // Windows: ⌘ und ⌥ sind beide Alt, eine andere Aktion hat diese Taste schon (anderes gespeichertes Kürzel).
            Recorded::Set(ref h) if self.alt_clash(action, h, cx) => return kadrell_tray::beep(),
            Recorded::Cancel => {}
            Recorded::Clear => self.change(|s| shortcut::assign(s, action, None), cx),
            Recorded::Set(h) => self.change(|s| shortcut::assign(s, action, Some(h)), cx),
        }
        self.stop_recording();
        cx.notify();
    }

    pub(super) fn reset_hotkey(&mut self, action: HotkeyAction, cx: &mut Context<Self>) {
        self.change(|s| shortcut::assign(s, action, Some(action.default_key())), cx);
    }

    fn alt_clash(&self, action: HotkeyAction, h: &Hotkey, cx: &Context<Self>) -> bool {
        shortcut::current() == shortcut::Platform::Windows && crate::workspace::keys::clashes(&cx.global::<AppSettings>().settings, action, h)
    }

    // MARK: Filter

    pub(super) fn open_filter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.filter.open = true;
        let focus = self.filter_input.read(cx).focus.clone();
        focus.focus(window, cx);
        cx.notify();
    }

    pub(super) fn close_filter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.filter = Filter::default();
        self.filter_input.update(cx, |i, cx| i.set_text("", window, cx));
        self.focus.focus(window, cx);
        cx.notify();
    }

    /// ⏎ im Filter: die Tastatur geht an den ersten Treffer.
    fn jump_to_first_hit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.cursor = self.focusable_rows(cx).into_iter().next();
        self.focus.focus(window, cx);
        self.ensure_visible(cx);
        cx.notify();
    }

    // MARK: Tastatur

    /// Tab, Pfeil hoch und runter: zur nächsten oder vorigen bedienbaren Zeile.
    pub(super) fn move_cursor(&mut self, delta: isize, window: &mut Window, cx: &mut Context<Self>) {
        let rows = self.focusable_rows(cx);
        if rows.is_empty() {
            return;
        }
        let at = self.cursor.as_ref().and_then(|c| rows.iter().position(|r| r == c));
        let next = match at {
            Some(i) => (i as isize + delta).rem_euclid(rows.len() as isize) as usize,
            None if delta > 0 => 0,
            None => rows.len() - 1,
        };
        self.cursor = Some(rows[next].clone());
        self.focus.focus(window, cx);
        self.ensure_visible(cx);
        cx.notify();
    }

    /// Tab und Umschalt+Tab: die Zeilen, danach die Haupttaste „Fertig“ (Tab-Stopp mit Ring), dann wieder die Zeilen.
    pub(super) fn tab_stop(&mut self, delta: isize, window: &mut Window, cx: &mut Context<Self>) {
        let rows = self.focusable_rows(cx);
        let at = self.cursor.as_ref().and_then(|c| rows.iter().position(|r| r == c));
        let in_foot = self.foot_focus.as_ref().is_some_and(|f| f.is_focused(window));
        match tab_target(at, rows.len(), delta, in_foot) {
            TabStop::Foot => {
                self.cursor = None;
                if let Some(foot) = self.foot_focus.clone() {
                    foot.focus(window, cx);
                }
                cx.notify();
            }
            TabStop::Row(i) => {
                self.cursor = Some(rows[i].clone());
                self.focus.focus(window, cx);
                self.ensure_visible(cx);
                cx.notify();
            }
            TabStop::None => {}
        }
    }

    /// Tab: in einem Pfadfeld das Kürzel ausschreiben (das Feld behält Tab, auch ohne Treffer), sonst zur nächsten Zeile.
    pub(super) fn next_row(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let focused = self.inputs.iter().find(|(id, i)| paths::kind_of(id).is_some() && i.read(cx).focus.is_focused(window)).map(|(id, _)| id.clone());
        let Some(id) = focused else { return self.tab_stop(1, window, cx) };
        if paths::kind_of(&id) == Some(paths::PathKind::Folder) {
            let typed = self.input_text(&id, cx);
            if let Some(full) = paths::tab_expansion(&typed, &folders::home_dir(), &folders::list_dirs) {
                self.set_input_text(&id, &full, window, cx);
            }
        }
    }

    /// Datei oder Ordner auf ein Pfadfeld gezogen: passt er zur Art des Feldes, steht er im Feld (und gilt sofort).
    pub(super) fn drop_on_path(&mut self, row: &str, dropped: &[std::path::PathBuf], window: &mut Window, cx: &mut Context<Self>) {
        let Some(kind) = paths::kind_of(row) else { return };
        if let Some(text) = paths::dropped(kind, dropped, &folders::home_dir()) {
            self.set_input_text(row, &text, window, cx);
        }
    }

    /// Scrollt die Zeile mit der Tastatur ins Bild (Rechtecke aus dem letzten Frame).
    fn ensure_visible(&self, cx: &mut Context<Self>) {
        let Some(id) = self.cursor.clone() else { return };
        let Some(row) = self.geometry.borrow().get(&format!("row:{id}")).copied() else { return };
        let view = self.scroll.bounds();
        let pad = px(8.);
        let shift = if row.top() < view.top() + pad {
            view.top() + pad - row.top()
        } else if row.bottom() > view.bottom() - pad {
            (view.bottom() - pad) - row.bottom()
        } else {
            return;
        };
        let offset = self.scroll.offset();
        self.scroll.set_offset(point(offset.x, offset.y + shift));
        cx.notify();
    }

    /// Leertaste: die Zeile mit der Tastatur bedienen.
    pub(super) fn activate_cursor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(row) = self.cursor.as_deref().and_then(|id| self.find_row(id, cx)) else { return };
        match &row.control {
            Control::Select { .. } => self.open_select(&row, window, cx),
            Control::Text { .. } => self.focus_input(&row.id, window, cx),
            Control::Hotkey(action) => self.start_recording(*action, cx),
            Control::Info { buttons, .. } | Control::Model { buttons, .. } | Control::Pull { buttons, .. } => {
                if let Some(b) = buttons.first() {
                    self.press(&b.clone(), window, cx);
                }
            }
            Control::Slider { .. } | Control::Note(_) => {}
        }
    }

    fn focus_input(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(input) = self.inputs.get(id) {
            let focus = input.read(cx).focus.clone();
            focus.focus(window, cx);
        }
    }

    /// Esc: erst Liste, dann Aufnahme (läuft über den Abfänger), dann Filter, dann der Dialog selbst.
    pub(super) fn cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.popover.is_some() {
            self.close_popover(cx);
        } else if self.filter.open {
            self.close_filter(window, cx);
        } else {
            self.stop_recording();
            cx.emit(SettingsEvent::Close);
        }
    }

    /// ⏎: Liste übernehmen, im Filter zum ersten Treffer, in einem Textfeld zurück an den Dialog, sonst fertig.
    pub(super) fn confirm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(p) = &self.popover {
            let index = p.cursor;
            self.pick(index, cx);
        } else if self.filter_input.read(cx).focus.is_focused(window) {
            self.jump_to_first_hit(window, cx);
        } else if !self.focus.is_focused(window) {
            self.focus.focus(window, cx);
        } else {
            self.stop_recording();
            cx.emit(SettingsEvent::Close);
        }
    }

    /// `/` öffnet den Filter, Leertaste bedient die Zeile. Nur wenn der Dialog selbst den Fokus hat: in einem
    /// Textfeld ist `/` ein normales Zeichen. Gewählt wird über `key_char`: auf deutscher Tastatur ist `/` Umschalt+7.
    pub(super) fn on_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let m = ev.keystroke.modifiers;
        if !self.focus.is_focused(window) || m.control || m.alt || m.platform || self.recording.is_some() {
            return;
        }
        match ev.keystroke.key_char.as_deref() {
            Some("/") => {
                self.open_filter(window, cx);
                cx.stop_propagation();
            }
            Some(" ") => {
                self.activate_cursor(window, cx);
                cx.stop_propagation();
            }
            _ => {}
        }
    }

    /// Hat eines der Textfelder (oder der Filter) den Fokus? Dann gehören Pfeiltasten dem Feld.
    pub(super) fn text_focused(&self, window: &Window, cx: &App) -> bool {
        !self.focus.is_focused(window) && (self.filter_input.read(cx).focus.is_focused(window) || self.inputs.values().any(|i| i.read(cx).focus.is_focused(window)))
    }
}

impl Focusable for SettingsDialog {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

/// Wert zu einer Stelle auf der Spur (0 bis 1): auf `step` gerundet, in `min..=max`.
pub fn slider_value(fraction: f32, min: f64, max: f64, step: f64) -> f64 {
    let raw = min + f64::from(fraction.clamp(0., 1.)) * (max - min);
    ((raw / step).round() * step).clamp(min, max)
}

/// Zieht das Farbschema der App nach, wenn in den Einstellungen eines gewählt ist, das nicht gilt.
fn sync_theme(cx: &mut App) {
    let settings = &cx.global::<AppSettings>().settings;
    if !settings.contains("colorTheme") {
        return;
    }
    let theme = Theme::named(&settings.color_theme());
    if cx.global::<ThemeGlobal>().0.id == theme.id {
        return;
    }
    crate::set_theme(cx, theme);
    cx.refresh_windows();
}

/// Aktuelle Belegung einer Aktion, unter Windows mit den Windows-Standards (`workspace::keys::windows_default`).
pub fn hotkey_of(settings: &Settings, action: HotkeyAction) -> Option<Hotkey> {
    crate::workspace::keys::effective_map(settings, shortcut::current()).remove(&action)
}

/// Wohin Tab geht.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum TabStop {
    Row(usize),
    Foot,
    None,
}

/// Tab-Reihenfolge: Zeile für Zeile, nach der letzten die Haupttaste, von dort zurück zur ersten Zeile. Ohne Markierung
/// beginnt Tab bei der ersten Zeile, Umschalt+Tab bei der Haupttaste.
pub(super) fn tab_target(at: Option<usize>, rows: usize, delta: isize, in_foot: bool) -> TabStop {
    if rows == 0 {
        return if in_foot { TabStop::None } else { TabStop::Foot };
    }
    let forward = delta > 0;
    match (in_foot, at) {
        (true, _) => TabStop::Row(if forward { 0 } else { rows - 1 }),
        (false, None) if forward => TabStop::Row(0),
        (false, None) => TabStop::Foot,
        (false, Some(i)) if forward && i + 1 == rows => TabStop::Foot,
        (false, Some(0)) if !forward => TabStop::Foot,
        (false, Some(i)) => TabStop::Row(i.saturating_add_signed(delta).min(rows - 1)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tab läuft durch die Zeilen und dann durch die Haupttaste (`dialog_foot_in`), Umschalt+Tab zurück.
    #[test]
    fn tab_visits_rows_then_the_primary_key() {
        assert_eq!(tab_target(Some(0), 3, 1, false), TabStop::Row(1));
        assert_eq!(tab_target(Some(2), 3, 1, false), TabStop::Foot);
        assert_eq!(tab_target(None, 3, 1, false), TabStop::Row(0));
        assert_eq!(tab_target(None, 3, -1, false), TabStop::Foot);
        assert_eq!(tab_target(Some(0), 3, -1, false), TabStop::Foot);
        assert_eq!(tab_target(Some(2), 3, -1, false), TabStop::Row(1));
        assert_eq!(tab_target(None, 3, 1, true), TabStop::Row(0));
        assert_eq!(tab_target(None, 3, -1, true), TabStop::Row(2));
        assert_eq!(tab_target(None, 0, 1, false), TabStop::Foot);
        assert_eq!(tab_target(None, 0, 1, true), TabStop::None);
    }

    #[test]
    fn slider_value_rounds_to_steps_and_clamps() {
        assert_eq!(slider_value(0., 0., 240., 5.), 0.);
        assert_eq!(slider_value(1., 0., 240., 5.), 240.);
        assert_eq!(slider_value(0.5, 0., 240., 5.), 120.);
        assert_eq!(slider_value(0.51, 0., 240., 5.), 120.);
        assert_eq!(slider_value(0.9, 50., 99., 1.), 94.);
        assert_eq!(slider_value(-3., 50., 99., 1.), 50.);
        assert_eq!(slider_value(7., 50., 99., 1.), 99.);
    }
}
