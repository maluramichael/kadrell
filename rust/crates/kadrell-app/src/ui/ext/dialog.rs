//! Extensions-Dialog (`ExtensionsView.swift`, `ExtensionDetail.swift`): alle gefundenen Extensions mit Zustand,
//! an/aus, Neu laden, Ordner, Log und Einstellungen, darunter der Block „ENTDECKEN“ mit der Suche im GitHub-Katalog.
//! Regeln und Zustand liegen in `dialog_model`. Das Netz wird nur auf Klick oder ⏎ angefasst, im Hintergrund.

use std::collections::HashMap;
use std::sync::Arc;

use gpui::{
    Anchor, App, AppContext, ClickEvent, Context, Div, ElementId, Entity, EventEmitter, FocusHandle, Focusable, FontWeight, InteractiveElement,
    IntoElement, KeyBinding, KeyDownEvent, MouseButton, ParentElement, Render, ScrollHandle, Stateful, StatefulInteractiveElement,
    Styled, SharedString, Subscription, Window, actions, anchored, deferred, div, point, prelude::*, px,
};
use kadrell_ext::github::RemoteExtension;
use kadrell_ext::http::HttpClient;
use kadrell_ext::manifest::{FoundExtension, Setting};
use kadrell_ext::panel::ThemeColor;
use kadrell_ext::supervisor::ExtensionState;
use kadrell_i18n::trd;
use serde_json::Value;

use kadrell_tray::beep;
use super::dialog_model::{Coalescer, DialogModel, Publish, panel_key, run_install, run_search, should_commit};
use super::spinner::spinner;
use super::status_color;
use crate::theme::Theme;
use crate::ui::chrome::h_seam;
use crate::ui::dialog_keys;
use crate::ui::instrument::focus_ring;
use crate::ui::overlay::{backdrop, dialog_frame, overlay_height, overlay_width};
use crate::ui::settings::controls::{Geometry, key_cap, measure, popover, popover_item, row as form_row, seam_ring, select_face, table};
use crate::ui::text_input::TextInput;
use crate::ui::theme;

actions!(kadrell_extensions, [Cancel, Confirm]);

pub const CONTEXT: &str = "KadrellExtensions";
const DOCS_URL: &str = "https://kadrell.malura.de/extensions";
const WIDTH: f32 = 760.;
const HEIGHT: f32 = 720.;

pub fn bind_keys(cx: &mut App) {
    let c = Some(CONTEXT);
    dialog_keys::bind_keys(cx);
    cx.bind_keys([KeyBinding::new("escape", Cancel, c), KeyBinding::new("enter", Confirm, c)]);
}

/// Kleine Taste als Tab-Stopp (`kbdFocusRing`): Enter und Leertaste lösen sie aus, mit Tastaturfokus zeichnet sie den Ring.
fn cap_control(
    id: impl Into<ElementId> + Clone,
    label: String,
    destructive: bool,
    t: &Theme,
    window: &mut Window,
    cx: &mut App,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    let key = key_cap(div().child(label), destructive, t).id(id.clone()).relative();
    let (key, ring) = dialog_keys::control(key, id, window, cx, std::rc::Rc::new(on_click));
    if ring { key.child(focus_ring(t)) } else { key }
}

pub enum DialogEvent {
    Close,
}

impl EventEmitter<DialogEvent> for ExtensionsDialog {}

/// Ein Eingabefeld des Formulars (`string`, `number`, `secret`) mit seiner Blur-Überwachung.
struct Field {
    input: Entity<TextInput>,
    _blur: Subscription,
}

pub struct ExtensionsDialog {
    focus: FocusHandle,
    pub model: DialogModel,
    http: Arc<dyn HttpClient>,
    /// Felder der gewählten Extension nach Schlüssel.
    fields: HashMap<String, Field>,
    fields_for: Option<String>,
    query: Entity<TextInput>,
    scroll: ScrollHandle,
    close_hint: String,
    /// Beim Schließen verfällt jeder Entwurf: ein Blur danach darf nichts mehr speichern.
    closing: bool,
    /// Bündelt die Meldungen des Managers auf höchstens eine alle 0,2 s.
    coalescer: Coalescer,
    /// Scrollstand des Logs: bleibt am Ende, solange neue Zeilen kommen (`defaultScrollAnchor(.bottom)`).
    log_scroll: ScrollHandle,
    log_lines: usize,
    log_for: Option<String>,
    /// Die offene Auswahl einer `choice`-Einstellung (ihr Schlüssel) und die gemessenen Flächen für die Lage der Liste.
    open_choice: Option<String>,
    geometry: Geometry,
}

impl ExtensionsDialog {
    pub fn new(model: DialogModel, http: Arc<dyn HttpClient>, close_hint: String, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let query = cx.new(|cx| TextInput::new(&trd!("Auf GitHub suchen (Topic kadrell-extension)"), cx).compact());
        let this = Self {
            focus: cx.focus_handle(),
            model,
            http,
            fields: HashMap::new(),
            fields_for: None,
            query,
            scroll: ScrollHandle::new(),
            close_hint,
            closing: false,
            coalescer: Coalescer::default(),
            log_scroll: ScrollHandle::new(),
            log_lines: 0,
            log_for: None,
            open_choice: None,
            geometry: Geometry::default(),
        };
        this.focus.focus(window, cx);
        this
    }

    /// Der Manager hat sich gemeldet (bis zu 50-mal je Sekunde): höchstens alle 0,2 s Auswahl nachziehen und neu zeichnen,
    /// sonst einmal am Ende der Wartezeit.
    pub fn changed(&mut self, cx: &mut Context<Self>) {
        match self.coalescer.changed(std::time::Instant::now()) {
            Publish::Now => self.publish(cx),
            Publish::Skip => {}
            Publish::After(wait) => {
                cx.spawn(async move |this, cx| {
                    cx.background_executor().timer(wait).await;
                    this.update(cx, |this, cx| {
                        this.coalescer.flushed(std::time::Instant::now());
                        this.publish(cx);
                    })
                    .ok();
                })
                .detach();
            }
        }
    }

    fn publish(&mut self, cx: &mut Context<Self>) {
        self.model.clamp_selection();
        cx.notify();
    }

    /// Spielt die Pieptöne, die das Modell vorgemerkt hat.
    fn sound_beeps(&mut self) {
        for _ in 0..self.model.take_beeps() {
            beep();
        }
    }

    /// Esc: schließt zuerst eine offene Auswahl, dann den Dialog.
    fn cancel(&mut self, cx: &mut Context<Self>) {
        if self.open_choice.take().is_some() {
            cx.notify();
        } else {
            self.close(cx);
        }
    }

    fn close(&mut self, cx: &mut Context<Self>) {
        self.closing = true;
        cx.emit(DialogEvent::Close);
    }

    // MARK: Formularfelder

    /// Legt die Felder der gewählten Extension an, einmal je Auswahl (der Inhalt gehört danach dem Feld).
    fn ensure_fields(&mut self, ext: &FoundExtension, window: &mut Window, cx: &mut Context<Self>) {
        if self.fields_for.as_deref() == Some(ext.name.as_str()) {
            return;
        }
        self.fields.clear();
        self.fields_for = Some(ext.name.clone());
        for s in ext.manifest.iter().flat_map(|m| m.settings.iter()).filter(|s| matches!(s.kind.as_str(), "string" | "number" | "secret" | "")) {
            let placeholder = if s.kind == "secret" { trd!("neuer Wert") } else { String::new() };
            let input = cx.new(|cx| TextInput::new(&placeholder, cx).compact());
            let text = if s.kind == "secret" { String::new() } else { self.model.field_text(&s.key) };
            input.update(cx, |i, cx| i.set_text(&text, window, cx));
            let key = s.key.clone();
            let blur = cx.on_blur(&input.read(cx).focus.clone(), window, move |this, _, cx| this.commit_field(&key, cx));
            self.fields.insert(s.key.clone(), Field { input, _blur: blur });
        }
    }

    fn any_field_focused(&self, window: &Window, cx: &App) -> bool {
        self.fields.values().any(|f| f.input.read(cx).focus.is_focused(window)) || self.query.read(cx).focus.is_focused(window)
    }

    /// Speichert, was geändert ist: mit ⏎ oder beim Wechsel in ein anderes Feld, nie pro Tastendruck (jede
    /// gespeicherte Änderung startet die Extension neu). Ein leeres Geheimnisfeld heißt „nicht angefasst“.
    fn commit_field(&mut self, key: &str, cx: &mut Context<Self>) {
        if self.closing {
            return;
        }
        let (Some(field), Some(ext)) = (self.fields.get(key), self.model.current()) else { return };
        let text = field.input.read(cx).text().to_string();
        let secret = ext.manifest.iter().flat_map(|m| m.settings.iter()).any(|s| s.key == key && s.kind == "secret");
        if !should_commit(secret, &text, &self.model.field_text(key)) {
            return;
        }
        self.model.save(&ext, key, &Value::from(text));
        cx.notify();
    }

    /// Verwirft alle Entwürfe, bevor die Auswahl wechselt (ein Klick auf eine Zeile ist kein Feldwechsel).
    fn discard_drafts(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for (key, field) in &self.fields {
            let text = self.model.field_text(key);
            let is_secret = self.model.secrets_set.contains(key);
            let reset = if is_secret { String::new() } else { text };
            field.input.update(cx, |i, cx| i.set_text(&reset, window, cx));
        }
    }

    fn commit_key(&mut self, window: &Window, cx: &mut Context<Self>) {
        let focused = self.fields.iter().find(|(_, f)| f.input.read(cx).focus.is_focused(window)).map(|(k, _)| k.clone());
        if let Some(key) = focused {
            self.commit_field(&key, cx);
        } else if self.query.read(cx).focus.is_focused(window) {
            self.search(cx);
        } else {
            self.close(cx);
        }
    }

    // MARK: Aktionen

    fn on_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let m = ev.keystroke.modifiers;
        let focused = self.any_field_focused(window, cx);
        let Some(key) = panel_key(ev.keystroke.key.as_str(), m.control, m.alt, m.platform, focused) else { return };
        if self.model.handle_key(key) {
            self.sound_beeps();
            self.after_selection(window, cx);
            cx.stop_propagation();
        }
    }

    fn after_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let _ = window;
        cx.notify();
    }

    fn select(&mut self, i: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.discard_drafts(window, cx);
        self.focus.focus(window, cx);
        self.model.select(i);
        cx.notify();
    }

    fn toggle(&mut self, ext: &FoundExtension, cx: &mut Context<Self>) {
        self.model.toggle(ext);
        self.sound_beeps();
        cx.notify();
    }

    fn open_folder(&self, ext: &FoundExtension, cx: &mut App) {
        cx.open_with_system(&ext.dir);
    }

    /// Legt den Ordner bei Bedarf an: wer noch keine Extension hat, soll trotzdem wissen, wohin damit.
    fn open_catalog(&self, cx: &mut App) {
        let dir = self.model.manager.catalog_dir().to_path_buf();
        let _ = std::fs::create_dir_all(&dir);
        cx.open_with_system(&dir);
    }

    fn search(&mut self, cx: &mut Context<Self>) {
        if self.model.searching {
            return;
        }
        let query = self.query.read(cx).text().to_string();
        self.model.query = query.clone();
        self.model.searching = true;
        self.model.store_error = None;
        let http = self.http.clone();
        cx.spawn(async move |this, cx| {
            let found = cx.background_executor().spawn(async move { run_search(http, &query) }).await;
            this.update(cx, |this, cx| {
                this.model.searching = false;
                match found {
                    Ok(list) => this.model.results = list,
                    Err(e) => this.model.store_error = Some(e),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn install(&mut self, remote: RemoteExtension, cx: &mut Context<Self>) {
        let id = remote.full_name();
        if !self.model.installing.insert(id.clone()) {
            return;
        }
        self.model.store_error = None;
        let (http, dir, origins) = (self.http.clone(), self.model.manager.catalog_dir().to_path_buf(), self.model.origins());
        let job = remote.clone();
        cx.spawn(async move |this, cx| {
            let done = cx.background_executor().spawn(async move { run_install(http, &job, &dir, &origins) }).await;
            this.update(cx, |this, cx| {
                this.model.installing.remove(&id);
                match done {
                    Ok(name) => this.model.installed(&name, &remote),
                    Err(e) => this.model.store_error = Some(e),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn commit_value(&mut self, ext: &FoundExtension, key: &str, value: Value, cx: &mut Context<Self>) {
        self.model.save(ext, key, &value);
        cx.notify();
    }

    // MARK: Zeichnen

    /// Kopf: „EXTENSIONS“ als gesperrtes Versal-Label (gedämpft), rechts die Taste für den Ordner und der Link zur Doku.
    fn header(&self, t: &Theme, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let on_catalog = cx.listener(|this, _, _, cx| this.open_catalog(cx));
        let folder = cap_control("open-catalog", trd!("Extensions-Ordner öffnen"), false, t, window, cx, on_catalog);
        let docs = div().id("docs").flex_none().text_size(px(11.)).text_color(t.accent_text).cursor_pointer().child(trd!("Doku")).on_click(|_, _, cx| cx.open_url(DOCS_URL));
        div()
            .flex()
            .flex_none()
            .items_baseline()
            .gap(px(14.))
            .px(px(16.))
            .pt(px(12.))
            .text_size(px(11.))
            .child(crate::ui::instrument::kerned("EXTENSIONS", 11., crate::ui::instrument::LABEL_KERN, false, t.muted))
            .child(div().flex_1())
            .child(folder)
            .child(docs)
    }

    /// Einmalige Warnung vor dem ersten Einschalten: fremder Code läuft ohne Sandbox mit den Rechten des Nutzers.
    /// Gruppenfläche mit 1,5 px Rand in der Fehlerfarbe, ohne den Fugenring der Gruppe darunter zu verdecken.
    fn enable_warning(&self, name: &str, window: &mut Window, t: &Theme, cx: &mut Context<Self>) -> Div {
        let rights = self.model.items().into_iter().find(|e| e.name == name).and_then(|e| e.manifest).map(|m| m.permissions).filter(|p| !p.is_empty());
        let on_yes = cx.listener(|this, _, _, cx| {
            this.model.confirm_yes();
            cx.notify();
        });
        let yes = cap_control("confirm-yes", trd!("Einschalten"), true, t, window, cx, on_yes);
        let on_no = cx.listener(|this, _, _, cx| {
            this.model.confirm_no();
            cx.notify();
        });
        let no = cap_control("confirm-no", trd!("Abbrechen"), false, t, window, cx, on_no);
        let mut col = div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .p(px(12.))
            .text_size(px(12.))
            .child(div().text_color(t.fg).child(trd!("%@ läuft ohne Sandbox und darf auf Dateien, Netzwerk und Programme zugreifen. Wirklich einschalten?", name)));
        if let Some(p) = rights {
            col = col.child(div().text_color(t.muted).child(trd!("Angegebene Rechte: %@ (nicht erzwungen)", p.join(", "))));
        }
        let col = col.child(div().flex().gap(px(14.)).child(yes).child(no));
        let ring = div().absolute().top_0().bottom_0().left_0().right_0().rounded(px(9.)).border(px(1.5)).border_color(t.error_text);
        div().px(px(16.)).py(px(12.)).child(seam_ring(t, 9., t.bg, col).relative().child(ring))
    }

    /// Eine Zeile der Liste: Punkt, Name, Version, Zustand, Schalter, Beschreibung, Rechte. Gewählt trägt sie die
    /// Zeilenfläche in `bg` mit 1,5 px Umriss in `fg` (`instrumentRow`: 6 px seitlich und 1 px oben und unten eingerückt,
    /// Radius 6).
    fn ext_row(&self, i: usize, ext: &FoundExtension, t: &Theme, cx: &mut Context<Self>) -> Stateful<Div> {
        let selected = i == self.model.selected;
        let state = self.model.state(&ext.name);
        let (status, failed) = match (&ext.problem, &state) {
            (Some(p), _) => (p.clone(), true),
            (None, ExtensionState::Failed(_)) => (state.label(), true),
            _ => (state.label(), false),
        };
        let dot = state_dot(&state, ext.problem.is_some());
        let on = self.model.is_enabled(&ext.name);
        let version = ext.manifest.as_ref().map(|m| m.version.clone()).unwrap_or_default();
        let name = ext.name.clone();
        let ext_for_toggle = ext.clone();
        let switch = switch(on, DialogModel::can_toggle(ext), t).id(("switch", i)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
            cx.stop_propagation();
            this.toggle(&ext_for_toggle, cx);
        }));
        let mut info = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .gap(px(3.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(div().font_weight(FontWeight::BOLD).text_color(t.fg).child(name))
                    .child(div().text_color(t.muted).child(version))
                    .child(div().flex_1())
                    .child(switch),
            )
            .child(div().text_color(if failed { t.error_text } else { t.muted }).child(status));
        if let Some(d) = ext.manifest.as_ref().map(|m| m.description.clone()).filter(|d| !d.is_empty()) {
            info = info.child(div().text_color(t.fg).child(d));
        }
        let rights = ext.manifest.as_ref().is_some_and(|m| !m.permissions.is_empty());
        if let Some(p) = ext.manifest.as_ref().map(|m| m.permissions.clone()).filter(|p| !p.is_empty()) {
            info = info.child(div().text_size(px(11.)).line_height(px(14.)).text_color(t.muted).child(trd!("Rechte: %@", p.join(", "))));
        }
        let face = div()
            .flex()
            .items_start()
            .gap(px(10.))
            .relative()
            .px(px(10.))
            .pt(px(7.))
            .pb(px(if rights { 7. } else { 7.5 }))
            .rounded(px(6.))
            // Zeile 16 pt seitlich, 8 pt oben und unten vom Zeilenrand, die Fläche liegt 6/1 pt darin (`instrumentRow`); der
            // Umriss ist ein Overlay und verschiebt nichts.
            .when(selected, |d| d.bg(t.bg).child(crate::ui::overlay::ring_inside(t.fg, 6., 1.5)))
            .child(div().mt(px(4.)).size(px(8.)).flex_none().rounded_full().bg(dot_color(dot, t)))
            .child(info);
        div().id(("ext", i)).cursor_pointer().px(px(6.)).py(px(1.)).child(face).on_click(cx.listener(move |this, _, window, cx| this.select(i, window, cx)))
    }

    /// Die Zeilen mit einer Fuge dazwischen.
    fn list(&self, t: &Theme, cx: &mut Context<Self>) -> Div {
        let items = self.model.items();
        let mut col = div().flex().flex_col().text_size(px(12.));
        if items.is_empty() {
            col = col.child(div().p(px(16.)).text_color(t.muted).child(trd!("Noch keine Extensions. Ordner hineinlegen oder per git clone holen, sie erscheinen sofort.")));
        }
        for (i, ext) in items.iter().enumerate() {
            if i > 0 {
                col = col.child(h_seam(t));
            }
            col = col.child(self.ext_row(i, ext, t, cx));
        }
        col
    }

    fn detail(&mut self, ext: &FoundExtension, window: &mut Window, t: &Theme, cx: &mut Context<Self>) -> Div {
        self.ensure_fields(ext, window, cx);
        let can_reload = DialogModel::can_toggle(ext) && self.model.is_enabled(&ext.name);
        let e1 = ext.clone();
        let e2 = ext.clone();
        let on_reload = cx.listener(move |this, _, _, cx| {
            if can_reload {
                this.model.reload(&e1);
            }
            cx.notify();
        });
        let reload = cap_control("reload", trd!("Neu laden  R"), false, t, window, cx, on_reload);
        let reload = if can_reload { reload.into_any_element() } else { div().opacity(0.4).child(reload).into_any_element() };
        let on_folder = cx.listener(move |this, _, _, cx| this.open_folder(&e2, cx));
        let folder = cap_control("folder", trd!("Ordner öffnen"), false, t, window, cx, on_folder);
        let log_label = if self.model.show_log { trd!("Log ausblenden  L") } else { trd!("Log  L") };
        let on_log = cx.listener(|this, _, _, cx| {
            this.model.show_log = !this.model.show_log;
            cx.notify();
        });
        let log = cap_control("log", log_label, false, t, window, cx, on_log);
        let head = div()
            .flex()
            .items_center()
            .gap(px(16.))
            .px(px(16.))
            .pt(px(12.))
            .pb(px(8.))
            .child(crate::ui::instrument::kerned(&ext.name.to_uppercase(), 11., crate::ui::instrument::LABEL_KERN, true, t.accent_text))
            .child(div().flex_1())
            .child(reload)
            .child(folder)
            .child(log);
        let mut col = div().flex().flex_col().pb(px(12.)).text_size(px(12.)).child(head);
        if let Some(form) = self.form(ext, window, t, cx) {
            col = col.child(form);
        }
        if self.model.show_log {
            col = col.child(self.log_view(&ext.name, t));
        }
        col
    }

    fn form(&self, ext: &FoundExtension, window: &mut Window, t: &Theme, cx: &mut Context<Self>) -> Option<Div> {
        let settings = &ext.manifest.as_ref()?.settings;
        if settings.is_empty() {
            return None;
        }
        let rows = settings.iter().map(|s| form_row(&s.label, self.control(ext, s, window, t, cx), 300., t).into_any_element()).collect();
        Some(table(rows, t))
    }

    fn control(&self, ext: &FoundExtension, s: &Setting, window: &mut Window, t: &Theme, cx: &mut Context<Self>) -> gpui::AnyElement {
        match s.kind.as_str() {
            "bool" => {
                let on = self.model.stored.get(&s.key).and_then(Value::as_bool).unwrap_or(false);
                let (e, key) = (ext.clone(), s.key.clone());
                div().flex().child(switch(on, true, t).id(SharedString::from(format!("bool-{}", s.key))).on_click(cx.listener(move |this, _, _, cx| this.commit_value(&e, &key, Value::Bool(!on), cx)))).into_any_element()
            }
            "choice" => self.choice(s, window, t, cx).into_any_element(),
            "secret" => {
                let label = if self.model.secrets_set.contains(&s.key) { trd!("gesetzt") } else { trd!("nicht gesetzt") };
                let input = self.fields.get(&s.key).map(|f| f.input.clone());
                div().flex().items_center().gap(px(8.)).child(div().flex_none().text_color(t.muted).child(label)).child(div().flex_1().min_w_0().children(input)).into_any_element()
            }
            _ => div().children(self.fields.get(&s.key).map(|f| f.input.clone())).into_any_element(),
        }
    }

    /// Auswahl für `choice` (`ChoiceMenu`): Fläche mit dem gewählten Namen (sonst dem Rohwert oder „–“) und Auf-Ab-Pfeilen,
    /// ein Klick öffnet die Liste mit dem gewählten Eintrag hervorgehoben.
    fn choice(&self, s: &Setting, window: &mut Window, t: &Theme, cx: &mut Context<Self>) -> gpui::AnyElement {
        let value = self.model.stored.get(&s.key).and_then(Value::as_str).map(str::to_string);
        let label = choice_label(s.options.as_deref().unwrap_or(&[]), value.as_deref());
        let key = s.key.clone();
        let open = self.open_choice.as_deref() == Some(s.key.as_str());
        let face = select_face(&label, open, t).relative().child(measure(&self.geometry, format!("choice:{}", s.key)));
        let id = SharedString::from(format!("choice-{}", s.key));
        let toggle = cx.listener(move |this, _, _, cx| {
            this.open_choice = if this.open_choice.as_deref() == Some(key.as_str()) { None } else { Some(key.clone()) };
            cx.notify();
        });
        let el = div().id(id.clone()).relative().cursor_pointer().child(face);
        let (el, ring) = dialog_keys::control(el, id, window, cx, std::rc::Rc::new(toggle));
        if ring { el.child(focus_ring(t)).into_any_element() } else { el.into_any_element() }
    }

    /// Die offene Liste einer Auswahl unter ihrer Fläche (über allem, am Fenster gehalten).
    fn choice_popover(&self, t: &Theme, cx: &mut Context<Self>) -> Option<gpui::AnyElement> {
        let key = self.open_choice.clone()?;
        let ext = self.model.current()?;
        let setting = ext.manifest.as_ref()?.settings.iter().find(|s| s.key == key)?.clone();
        let target = self.geometry.borrow().get(&format!("choice:{key}")).copied()?;
        let current = self.model.stored.get(&key).and_then(Value::as_str).map(str::to_string);
        let mut list = popover(f32::from(target.size.width), t).id("choice-list").occlude().on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());
        for (i, o) in setting.options.iter().flatten().enumerate() {
            let on = current.as_deref() == Some(o.value.as_str());
            let (e, k, v) = (ext.clone(), key.clone(), o.value.clone());
            list = list.child(popover_item(&o.label, on, false, t).id(("choice-opt", i)).cursor_pointer().on_click(cx.listener(move |this, _, _, cx| {
                this.open_choice = None;
                this.commit_value(&e, &k, Value::from(v.clone()), cx);
            })));
        }
        let placed = anchored().anchor(Anchor::TopLeft).snap_to_window_with_margin(px(8.)).position(point(target.origin.x, target.origin.y + target.size.height + px(2.))).child(list);
        Some(deferred(placed).priority(1).into_any_element())
    }

    /// Log der gewählten Extension: Höhe 180, am Ende verankert (neue Zeilen scrollen mit). Auswählen lässt sich der
    /// Text hier nicht (GPUI-Text ist nicht markierbar).
    fn log_view(&mut self, name: &str, t: &Theme) -> Div {
        let lines = self.model.manager.log(name);
        if self.log_for.as_deref() != Some(name) || lines.len() != self.log_lines {
            self.log_for = Some(name.to_string());
            self.log_lines = lines.len();
            self.log_scroll.scroll_to_bottom();
        }
        let text = if lines.is_empty() { trd!("Noch keine Logzeilen.") } else { lines.join("\n") };
        let body = div().id("ext-log").h(px(180.)).overflow_y_scroll().track_scroll(&self.log_scroll).px(px(8.)).py(px(5.)).rounded(px(6.)).bg(t.well()).text_size(px(11.)).text_color(t.muted).child(text);
        div().px(px(16.)).pt(px(10.)).child(body)
    }

    fn result_row(&self, i: usize, remote: &RemoteExtension, window: &mut Window, t: &Theme, cx: &mut Context<Self>) -> Div {
        let busy = self.model.installing.contains(&remote.full_name());
        let label = if self.model.is_installed(remote) { trd!("Aktualisieren") } else { trd!("Installieren") };
        let action: gpui::AnyElement = if busy {
            spinner(t.muted).into_any_element()
        } else {
            let r = remote.clone();
            let on_install = cx.listener(move |this, _, _, cx| this.install(r.clone(), cx));
            cap_control(("install", i), label, false, t, window, cx, on_install).into_any_element()
        };
        let url = remote.html_url.clone();
        let title = div()
            .flex()
            .gap(px(8.))
            .child(div().id(("repo", i)).font_weight(FontWeight::BOLD).text_color(t.accent_text).cursor_pointer().child(remote.full_name()).on_click(move |_, _, cx| cx.open_url(&url)))
            .child(div().text_color(t.muted).child(format!("★ {}", remote.stars)));
        let mut info = div().flex().flex_col().flex_1().min_w_0().gap(px(2.)).child(title);
        if !remote.description.is_empty() {
            info = info.child(div().text_color(t.muted).child(remote.description.clone()));
        }
        div().flex().items_start().gap(px(10.)).text_size(px(12.)).child(info).child(action)
    }

    /// Marktplatz-Block: „ENTDECKEN“ mit Spinner während der Suche, Suchfeld und Taste, Fehlertext, Treffer.
    fn discover(&self, window: &mut Window, t: &Theme, cx: &mut Context<Self>) -> Div {
        let title = div()
            .flex()
            .items_center()
            .gap(px(10.))
            .child(crate::ui::instrument::kerned("ENTDECKEN", 11., crate::ui::instrument::LABEL_KERN, true, t.accent_text))
            .child(div().flex_1())
            .children(self.model.searching.then(|| spinner(t.muted)));
        let on_search = cx.listener(|this, _, _, cx| this.search(cx));
        let search = cap_control("search", trd!("Suchen"), false, t, window, cx, on_search);
        let bar = div().flex().items_center().gap(px(10.)).child(div().flex_1().min_w_0().child(self.query.clone())).child(search);
        let mut col = div().flex().flex_col().gap(px(8.)).px(px(16.)).py(px(10.)).text_size(px(12.)).child(title).child(bar);
        if let Some(e) = &self.model.store_error {
            col = col.child(div().text_color(t.error_text).child(e.clone()));
        }
        for (i, r) in self.model.results.iter().enumerate() {
            col = col.child(self.result_row(i, r, window, t, cx));
        }
        col
    }

    /// Alles zwischen Kopf und Fußzeile: Warnung, Liste, Detail der Auswahl, Marktplatz.
    fn body(&mut self, window: &mut Window, t: &Theme, cx: &mut Context<Self>) -> Div {
        let mut col = div().flex().flex_col();
        if let Some(name) = self.model.confirm.clone() {
            col = col.child(self.enable_warning(&name, window, t, cx));
        }
        col = col.child(self.list(t, cx));
        if let Some(ext) = self.model.current() {
            col = col.child(h_seam(t)).child(self.detail(&ext, window, t, cx));
        }
        col.child(h_seam(t)).child(self.discover(window, t, cx))
    }
}

/// Name der gewählten Option, sonst der Rohwert, sonst „–“.
pub fn choice_label(options: &[kadrell_ext::manifest::SettingOption], value: Option<&str>) -> String {
    options.iter().find(|o| Some(o.value.as_str()) == value).map(|o| o.label.clone()).or_else(|| value.map(str::to_string)).unwrap_or_else(|| "–".to_string())
}

/// Zustandsfarbe der Punkte: läuft ok, startet warn, Fehler err, aus muted (mit Problem err).
fn state_dot(state: &ExtensionState, problem: bool) -> ThemeColor {
    match state {
        ExtensionState::Running => ThemeColor::Ok,
        ExtensionState::Starting | ExtensionState::Reloading => ThemeColor::Warn,
        ExtensionState::Failed(_) => ThemeColor::Err,
        ExtensionState::Off if problem => ThemeColor::Err,
        ExtensionState::Off => ThemeColor::Muted,
    }
}

fn dot_color(c: ThemeColor, t: &Theme) -> gpui::Hsla {
    status_color(c, t)
}

/// Schalter wie im Mac-Dialog (`Toggle` im Stil `.switch`, `.mini`): Spur 36 × 16 in Akzent (an) oder Linie (aus),
/// flacher Knopf 21 × 13. Ausgegraut, wenn nicht schaltbar.
fn switch(on: bool, enabled: bool, t: &Theme) -> Div {
    let knob_color = if on { gpui::rgb(0xEAF1FF) } else { gpui::rgb(0xE1E1E4) };
    let knob = div().absolute().top(px(1.5)).left(px(if on { 13.5 } else { 1.5 })).w(px(21.)).h(px(13.)).rounded_full().bg(knob_color);
    let track = div().relative().flex_none().w(px(36.)).h(px(16.)).rounded_full().bg(if on { t.accent } else { t.line }).cursor_pointer().child(knob);
    if enabled { track } else { track.opacity(0.4) }
}

impl Render for ExtensionsDialog {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme(cx);
        let body = self.body(window, &t, cx);
        let hint = if self.close_hint.is_empty() { trd!("Esc schließt") } else { trd!("Esc oder %@ schließen", self.close_hint.clone()) };
        let hint = trd!("↑↓ wählen · Leertaste an/aus · R neu laden · L Log · %@", hint);
        let scroll = div()
            .id("extensions-scroll")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .on_scroll(cx.listener(|this, _, _, cx| {
                if this.open_choice.take().is_some() {
                    cx.notify();
                }
            }))
            .child(body);
        let foot = div()
            .flex()
            .flex_col()
            .flex_none()
            .child(h_seam(&t))
            .child(div().flex().justify_end().px(px(16.)).py(px(10.)).text_size(px(11.)).text_color(t.muted).child(hint));
        let frame = overlay_height(overlay_width(dialog_frame(&t), WIDTH), HEIGHT)
            .key_context(CONTEXT)
            .track_focus(&self.focus)
            .text_color(t.fg)
            .on_action(cx.listener(|this, _: &Cancel, _, cx| this.cancel(cx)))
            .on_action(cx.listener(|this, _: &Confirm, window, cx| this.commit_key(window, cx)))
            .on_key_down(cx.listener(|this, ev, window, cx| this.on_key(ev, window, cx)))
            .child(self.header(&t, window, cx))
            .child(div().pt(px(12.)).child(h_seam(&t)))
            .child(scroll)
            .child(foot)
            .children(self.choice_popover(&t, cx));
        dialog_keys::nav(backdrop(cx.listener(|this, _, _, cx| this.close(cx)))).child(frame)
    }
}

impl Focusable for ExtensionsDialog {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
