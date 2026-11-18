//! Extensions-Dialog (`ExtensionsView.swift`, `ExtensionDetail.swift`): alle gefundenen Extensions mit Zustand,
//! an/aus, Neu laden, Ordner, Log und Einstellungen; im Reiter „Entdecken“ die Suche im GitHub-Katalog. Regeln und
//! Zustand liegen in `dialog_model`. Das Netz wird nur auf Klick oder ⏎ angefasst, im Hintergrund.

use std::collections::HashMap;
use std::sync::Arc;

use gpui::{
    App, AppContext, ClickEvent, Context, Div, Entity, EventEmitter, FocusHandle, Focusable, FontWeight, InteractiveElement,
    IntoElement, KeyBinding, KeyDownEvent, ParentElement, Render, ScrollHandle, Stateful, StatefulInteractiveElement, Styled,
    SharedString, Subscription, Window, actions, div, prelude::*, px,
};
use kadrell_ext::github::RemoteExtension;
use kadrell_ext::http::HttpClient;
use kadrell_ext::manifest::{FoundExtension, Setting};
use kadrell_ext::panel::ThemeColor;
use kadrell_ext::supervisor::ExtensionState;
use kadrell_i18n::trd;
use serde_json::Value;

use super::dialog_model::{DialogModel, Key, Tab, Toggle, run_install, run_search};
use super::status_color;
use crate::theme::Theme;
use crate::ui::chrome::{h_seam, key_edge};
use crate::ui::overlay::{backdrop, dialog_frame, dialog_foot, overlay_height, overlay_width};
use crate::ui::settings::controls::{key_cap, row as form_row, table};
use crate::ui::text_input::TextInput;
use crate::ui::theme;

actions!(kadrell_extensions, [Cancel, Confirm]);

pub const CONTEXT: &str = "KadrellExtensions";
const DOCS_URL: &str = "https://kadrell.malura.de/extensions";
const WIDTH: f32 = 760.;
const HEIGHT: f32 = 720.;

pub fn bind_keys(cx: &mut App) {
    let c = Some(CONTEXT);
    cx.bind_keys([KeyBinding::new("escape", Cancel, c), KeyBinding::new("enter", Confirm, c)]);
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
        };
        this.focus.focus(window, cx);
        this
    }

    /// Der Manager hat sich gemeldet (höchstens alle 0,2 s): Auswahl nachziehen, neu zeichnen.
    pub fn changed(&mut self, cx: &mut Context<Self>) {
        self.model.clamp_selection();
        cx.notify();
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
        if (secret && text.is_empty()) || (!secret && text == self.model.field_text(key)) {
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
        if m.control || m.alt || m.platform || self.any_field_focused(window, cx) {
            return;
        }
        let key = match ev.keystroke.key.as_str() {
            "up" => Key::Up,
            "down" => Key::Down,
            "space" => Key::Space,
            k if k.chars().count() == 1 => Key::Char(k.chars().next().unwrap_or(' ')),
            _ => return,
        };
        if self.model.tab == Tab::Installed && self.model.handle_key(key) {
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
        if self.model.toggle(ext) == Toggle::Blocked {
            eprintln!("kadrell: Extension {} lässt sich nicht schalten", ext.name);
        }
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

    fn set_tab(&mut self, tab: Tab, cx: &mut Context<Self>) {
        self.model.tab = tab;
        cx.notify();
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

    fn header(&self, t: &Theme, cx: &mut Context<Self>) -> Div {
        let tab = |id: &'static str, label: String, tab: Tab, this: &Self, cx: &mut Context<Self>| {
            let on = this.model.tab == tab;
            let cap = div()
                .px(px(10.))
                .py(px(3.))
                .rounded(px(4.))
                .text_size(px(11.))
                .font_weight(FontWeight::BOLD)
                .when(on, |d| d.bg(t.accent).text_color(t.pill_text()))
                .when(!on, |d| d.bg(t.surface).text_color(t.key_label()))
                .child(label);
            div().id(id).flex_none().pb(px(2.)).rounded(px(5.)).bg(key_edge(t)).cursor_pointer().child(cap).on_click(cx.listener(move |this, _, _, cx| this.set_tab(tab, cx)))
        };
        let folder = key_cap(div().child(trd!("Extensions-Ordner öffnen")), false, t).id("open-catalog").on_click(cx.listener(|this, _, _, cx| this.open_catalog(cx)));
        let docs = div().id("docs").flex_none().text_size(px(11.)).text_color(t.accent_text).cursor_pointer().child(trd!("Doku")).on_click(|_, _, cx| cx.open_url(DOCS_URL));
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(8.))
            .px(px(16.))
            .pt(px(12.))
            .child(tab("tab-installed", trd!("Extensions").to_uppercase(), Tab::Installed, self, cx))
            .child(tab("tab-discover", trd!("ENTDECKEN"), Tab::Discover, self, cx))
            .child(div().flex_1())
            .child(folder)
            .child(docs)
    }

    /// Einmalige Warnung vor dem ersten Einschalten: fremder Code läuft ohne Sandbox mit den Rechten des Nutzers.
    fn enable_warning(&self, name: &str, t: &Theme, cx: &mut Context<Self>) -> Div {
        let rights = self.model.items().into_iter().find(|e| e.name == name).and_then(|e| e.manifest).map(|m| m.permissions).filter(|p| !p.is_empty());
        let yes = key_cap(div().child(trd!("Einschalten")), true, t).id("confirm-yes").on_click(cx.listener(|this, _, _, cx| {
            this.model.confirm_yes();
            cx.notify();
        }));
        let no = key_cap(div().child(trd!("Abbrechen")), false, t).id("confirm-no").on_click(cx.listener(|this, _, _, cx| {
            this.model.confirm_no();
            cx.notify();
        }));
        let mut col = div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .mx(px(16.))
            .my(px(12.))
            .p(px(12.))
            .rounded(px(9.))
            .bg(t.bg)
            .border_2()
            .border_color(t.error_text)
            .text_size(px(12.))
            .child(div().text_color(t.fg).child(trd!("%@ läuft ohne Sandbox und darf auf Dateien, Netzwerk und Programme zugreifen. Wirklich einschalten?", name)));
        if let Some(p) = rights {
            col = col.child(div().text_color(t.muted).child(trd!("Angegebene Rechte: %@ (nicht erzwungen)", p.join(", "))));
        }
        col.child(div().flex().gap(px(14.)).child(yes).child(no))
    }

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
        if let Some(p) = ext.manifest.as_ref().map(|m| m.permissions.clone()).filter(|p| !p.is_empty()) {
            info = info.child(div().text_size(px(11.)).text_color(t.muted).child(trd!("Rechte: %@", p.join(", "))));
        }
        let face = div()
            .flex()
            .items_start()
            .gap(px(10.))
            .mx(px(10.))
            .my(px(1.))
            .px(px(6.))
            .py(px(7.))
            .rounded(px(7.))
            .border_2()
            .border_color(if selected { t.fg } else { gpui::transparent_black() })
            .when(selected, |d| d.bg(t.bg))
            .child(div().mt(px(4.)).size(px(8.)).flex_none().rounded_full().bg(dot_color(dot, t)))
            .child(info);
        div().id(("ext", i)).cursor_pointer().child(face).on_click(cx.listener(move |this, _, window, cx| this.select(i, window, cx)))
    }

    fn list(&self, t: &Theme, cx: &mut Context<Self>) -> Div {
        let items = self.model.items();
        let mut col = div().flex().flex_col().text_size(px(12.)).py(px(6.));
        if items.is_empty() {
            col = col.child(div().p(px(16.)).text_color(t.muted).child(trd!("Noch keine Extensions. Ordner hineinlegen oder per git clone holen, sie erscheinen sofort.")));
        }
        for (i, ext) in items.iter().enumerate() {
            col = col.child(self.ext_row(i, ext, t, cx));
        }
        col
    }

    fn small_button(&self, id: &'static str, label: String, t: &Theme) -> Stateful<Div> {
        key_cap(div().child(label), false, t).id(id)
    }

    fn detail(&mut self, ext: &FoundExtension, window: &mut Window, t: &Theme, cx: &mut Context<Self>) -> Div {
        self.ensure_fields(ext, window, cx);
        let can_reload = DialogModel::can_toggle(ext) && self.model.is_enabled(&ext.name);
        let e1 = ext.clone();
        let e2 = ext.clone();
        let reload = self.small_button("reload", trd!("Neu laden  R"), t).on_click(cx.listener(move |this, _, _, cx| {
            this.model.reload(&e1);
            cx.notify();
        }));
        let reload = if can_reload { reload.into_any_element() } else { div().opacity(0.4).child(reload).into_any_element() };
        let folder = self.small_button("folder", trd!("Ordner öffnen"), t).on_click(cx.listener(move |this, _, _, cx| this.open_folder(&e2, cx)));
        let log_label = if self.model.show_log { trd!("Log ausblenden  L") } else { trd!("Log  L") };
        let log = self.small_button("log", log_label, t).on_click(cx.listener(|this, _, _, cx| {
            this.model.show_log = !this.model.show_log;
            cx.notify();
        }));
        let head = div()
            .flex()
            .items_center()
            .gap(px(16.))
            .px(px(16.))
            .pt(px(12.))
            .pb(px(8.))
            .child(div().text_size(px(11.)).font_weight(FontWeight::BOLD).text_color(t.accent_text).child(ext.name.to_uppercase()))
            .child(div().flex_1())
            .child(reload)
            .child(folder)
            .child(log);
        let mut col = div().flex().flex_col().pb(px(12.)).text_size(px(12.)).child(head);
        if let Some(form) = self.form(ext, t, cx) {
            col = col.child(form);
        }
        if self.model.show_log {
            col = col.child(self.log_view(&ext.name, t));
        }
        col
    }

    fn form(&self, ext: &FoundExtension, t: &Theme, cx: &mut Context<Self>) -> Option<Div> {
        let settings = &ext.manifest.as_ref()?.settings;
        if settings.is_empty() {
            return None;
        }
        let rows = settings.iter().map(|s| form_row(&s.label, self.control(ext, s, t, cx), 300., t).into_any_element()).collect();
        Some(table(rows, t))
    }

    fn control(&self, ext: &FoundExtension, s: &Setting, t: &Theme, cx: &mut Context<Self>) -> gpui::AnyElement {
        match s.kind.as_str() {
            "bool" => {
                let on = self.model.stored.get(&s.key).and_then(Value::as_bool).unwrap_or(false);
                let (e, key) = (ext.clone(), s.key.clone());
                div().flex().child(switch(on, true, t).id(SharedString::from(format!("bool-{}", s.key))).on_click(cx.listener(move |this, _, _, cx| this.commit_value(&e, &key, Value::Bool(!on), cx)))).into_any_element()
            }
            "choice" => self.choice(ext, s, t, cx).into_any_element(),
            "secret" => {
                let label = if self.model.secrets_set.contains(&s.key) { trd!("gesetzt") } else { trd!("nicht gesetzt") };
                let input = self.fields.get(&s.key).map(|f| f.input.clone());
                div().flex().items_center().gap(px(8.)).child(div().flex_none().text_color(t.muted).child(label)).child(div().flex_1().min_w_0().children(input)).into_any_element()
            }
            _ => div().children(self.fields.get(&s.key).map(|f| f.input.clone())).into_any_element(),
        }
    }

    /// Auswahl für `choice`: ein Taster je Option, der gewählte gefüllt.
    fn choice(&self, ext: &FoundExtension, s: &Setting, t: &Theme, cx: &mut Context<Self>) -> Div {
        let current = self.model.stored.get(&s.key).and_then(Value::as_str).map(str::to_string);
        let mut row = div().flex().flex_wrap().gap(px(6.));
        for (i, o) in s.options.iter().flatten().enumerate() {
            let on = current.as_deref() == Some(o.value.as_str());
            let cap = div()
                .px(px(8.))
                .py(px(2.))
                .rounded(px(4.))
                .text_size(px(11.))
                .when(on, |d| d.bg(t.accent).text_color(t.pill_text()))
                .when(!on, |d| d.bg(t.surface).text_color(t.key_label()))
                .child(o.label.clone());
            let (e, key, value) = (ext.clone(), s.key.clone(), o.value.clone());
            row = row.child(
                div()
                    .id(SharedString::from(format!("choice-{}-{i}", s.key)))
                    .pb(px(2.))
                    .rounded(px(5.))
                    .bg(key_edge(t))
                    .cursor_pointer()
                    .child(cap)
                    .on_click(cx.listener(move |this, _, _, cx| this.commit_value(&e, &key, Value::from(value.clone()), cx))),
            );
        }
        row
    }

    fn log_view(&self, name: &str, t: &Theme) -> Div {
        let lines = self.model.manager.log(name);
        let text = if lines.is_empty() { trd!("Noch keine Logzeilen.") } else { lines.join("\n") };
        let body = div().id("ext-log").flex().flex_col().h(px(180.)).overflow_y_scroll().px(px(8.)).py(px(5.)).rounded(px(6.)).bg(t.well()).text_size(px(11.)).text_color(t.muted).child(text);
        div().px(px(16.)).pt(px(10.)).child(body)
    }

    fn installed_tab(&mut self, window: &mut Window, t: &Theme, cx: &mut Context<Self>) -> Div {
        let mut col = div().flex().flex_col();
        if let Some(name) = self.model.confirm.clone() {
            col = col.child(self.enable_warning(&name, t, cx));
        }
        col = col.child(self.list(t, cx));
        if let Some(ext) = self.model.current() {
            col = col.child(h_seam(t)).child(self.detail(&ext, window, t, cx));
        }
        col
    }

    fn result_row(&self, i: usize, remote: &RemoteExtension, t: &Theme, cx: &mut Context<Self>) -> Div {
        let busy = self.model.installing.contains(&remote.full_name());
        let label = if self.model.is_installed(remote) { trd!("Aktualisieren") } else { trd!("Installieren") };
        let action: gpui::AnyElement = if busy {
            div().text_color(t.muted).child("…").into_any_element()
        } else {
            let r = remote.clone();
            self.small_button("install", label, t).id(("install", i)).on_click(cx.listener(move |this, _, _, cx| this.install(r.clone(), cx))).into_any_element()
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
        div().flex().items_start().gap(px(12.)).px(px(16.)).py(px(5.)).text_size(px(12.)).child(info).child(action)
    }

    fn discover_tab(&self, t: &Theme, cx: &mut Context<Self>) -> Div {
        let search = self.small_button("search", trd!("Suchen"), t).on_click(cx.listener(|this, _, _, cx| this.search(cx)));
        let bar = div().flex().items_center().gap(px(10.)).px(px(16.)).pt(px(14.)).pb(px(8.)).child(div().flex_1().min_w_0().child(self.query.clone())).child(search);
        let mut col = div().flex().flex_col().pb(px(12.)).child(bar);
        if self.model.searching {
            col = col.child(div().px(px(16.)).text_size(px(12.)).text_color(t.muted).child("…"));
        }
        if let Some(e) = &self.model.store_error {
            col = col.child(div().px(px(16.)).py(px(4.)).text_size(px(12.)).text_color(t.error_text).child(e.clone()));
        }
        for (i, r) in self.model.results.iter().enumerate() {
            col = col.child(self.result_row(i, r, t, cx));
        }
        col
    }
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

/// Schalter wie im Mac-Dialog: Spur in Akzent (an) oder Linie (aus), runder Knopf. Ausgegraut, wenn nicht schaltbar.
fn switch(on: bool, enabled: bool, t: &Theme) -> Div {
    let knob = div().absolute().top(px(2.)).left(px(if on { 14. } else { 2. })).size(px(12.)).rounded_full().bg(gpui::white());
    let track = div().relative().flex_none().w(px(28.)).h(px(16.)).rounded_full().bg(if on { t.accent } else { t.line }).cursor_pointer().child(knob);
    if enabled { track } else { track.opacity(0.4) }
}

impl Render for ExtensionsDialog {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme(cx);
        let body = match self.model.tab {
            Tab::Installed => self.installed_tab(window, &t, cx),
            Tab::Discover => self.discover_tab(&t, cx),
        };
        let hint = if self.close_hint.is_empty() { trd!("Esc schließt") } else { trd!("Esc oder %@ schließen", self.close_hint.clone()) };
        let hint = trd!("↑↓ wählen · Leertaste an/aus · R neu laden · L Log · %@", hint);
        let scroll = div().id("extensions-scroll").flex_1().min_h_0().overflow_y_scroll().track_scroll(&self.scroll).child(body);
        let foot = dialog_foot(hint, trd!("Fertig ⏎"), &t, cx.listener(|this, _, _, cx| this.close(cx)));
        let frame = overlay_height(overlay_width(dialog_frame(&t), WIDTH), HEIGHT)
            .key_context(CONTEXT)
            .track_focus(&self.focus)
            .text_color(t.fg)
            .on_action(cx.listener(|this, _: &Cancel, _, cx| this.close(cx)))
            .on_action(cx.listener(|this, _: &Confirm, window, cx| this.commit_key(window, cx)))
            .on_key_down(cx.listener(|this, ev, window, cx| this.on_key(ev, window, cx)))
            .child(self.header(&t, cx))
            .child(div().pt(px(12.)).child(h_seam(&t)))
            .child(scroll)
            .child(foot);
        backdrop(cx.listener(|this, _, _, cx| this.close(cx))).child(frame)
    }
}

impl Focusable for ExtensionsDialog {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
