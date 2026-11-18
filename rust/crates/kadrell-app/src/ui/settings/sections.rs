//! Die Abschnitte des Einstellungsdialogs in der Reihenfolge von `SettingsView.sections` der Mac-App, aus
//! `Settings` gebaut. Titel sind die deutschen Texte aus `Localizable.xcstrings` (Englisch kommt mit Fluent).

use std::rc::Rc;

use kadrell_core::Settings;
use kadrell_core::hotkeys::{self, HotkeyAction};
use kadrell_core::settings::{
    Ask, AUTO_DETACH_RANGE, AUTOSWITCH_RANGE, CLAUDE_EFFORTS, CLAUDE_MODELS, CLAUDE_MODES, FONT_SIZES,
    LINE_SPACING_PERCENT, Language, NotificationLevel, PIXEL_RANGE, RawEnum, SCROLLBACK_RANGE, SidebarStyle,
    SoundLevel, StatusBarDisplay, UI_SCALE_PERCENT,
};
use serde_json::Value;

use super::backend::{Backend, ProviderStatus};
use super::model::{Block, Btn, Control, Icon, Msg, Provider, Row, Setter, Tone};
use super::shortcut::{self, Platform};
use crate::lang::{de, tx};
use kadrell_i18n::trd;
use crate::theme::Theme;

/// Alles, was die Abschnitte zum Bauen brauchen.
pub struct Context<'a> {
    pub settings: &'a Settings,
    pub backend: &'a dyn Backend,
    /// Id des Schemas, das gerade gilt (kann vom gespeicherten abweichen, solange nichts gewählt wurde).
    pub theme_id: &'a str,
    pub platform: Platform,
}

fn t(s: &str) -> String {
    s.to_string()
}

fn setter<T: 'static>(f: impl Fn(&mut Settings, T) + 'static) -> Setter<T> {
    Rc::new(f)
}

fn block(title: &str, note: &str, rows: Vec<Row>) -> Block {
    Block { heading: Some((tx(title), tx(note))), subtitle: None, rows, keep_all: vec![tx(title), tx(note)] }
}

// MARK: Steuerelemente

fn select(id: &str, title: &str, options: Vec<(String, String)>, value: String, set: Setter<String>) -> Row {
    Row::new(id, title, Control::Select { options, value, set })
}

/// Dropdown „an/aus“ wie in der Mac-App.
fn on_off(id: &str, title: &str, value: bool, set: impl Fn(&mut Settings, bool) + 'static) -> Row {
    let options = vec![(t("on"), tx(de!("an"))), (t("off"), tx(de!("aus")))];
    select(id, title, options, t(if value { "on" } else { "off" }), setter(move |s, v: String| set(s, v == "on")))
}

/// Dropdown über die Fälle einer Aufzählung des Kerns.
fn choice<T: RawEnum + 'static>(
    id: &str,
    title: &str,
    all: &'static [T],
    label: fn(T) -> &'static str,
    current: T,
    set: fn(&mut Settings, T),
) -> Row {
    let options = all.iter().map(|c| (t(c.raw()), tx(label(*c)))).collect();
    select(id, title, options, t(current.raw()), setter(move |s, v: String| T::from_raw(&v).into_iter().for_each(|c| set(s, c))))
}

/// Texte als Dropdown, `""` heißt Claude-Standard.
fn texts(id: &str, title: &str, values: &[&str], current: String, set: fn(&mut Settings, &str)) -> Row {
    let options = values.iter().map(|v| (t(v), if v.is_empty() { tx(de!("Standard")) } else { t(v) })).collect();
    select(id, title, options, current, setter(move |s, v: String| set(s, &v)))
}

#[allow(clippy::too_many_arguments)]
fn slider(id: &str, title: &str, value: f64, range: (f64, f64), step: f64, unit: &str, live: bool, set: impl Fn(&mut Settings, f64) + 'static) -> Row {
    let control = Control::Slider { value, min: range.0, max: range.1, step, unit: tx(unit), live, set: setter(set) };
    Row::new(id, title, control)
}

fn text_field(id: &str, title: &str, placeholder: &str, value: String, set: Option<Setter<String>>, buttons: Vec<Btn>) -> Row {
    Row::new(id, title, Control::Text { placeholder: tx(placeholder), value, set, buttons })
}

fn string_field(id: &str, title: &str, placeholder: &str, key: &'static str, default: &str, s: &Settings) -> Row {
    let set = setter(move |s: &mut Settings, v: String| s.set(key, Value::String(v)));
    text_field(id, title, placeholder, s.string_or(key, default), Some(set), vec![])
}

// MARK: Abschnitte

fn sessions(c: &Context) -> Block {
    let s = c.settings;
    let home = super::super::folders::home_dir();
    let folder = super::super::folders::short_path(&s.start_folder(std::path::Path::new(&home)), &home);
    let browse = vec![Btn::icon(Icon::Folder, Msg::Browse { row: t("startFolder"), files: false })];
    let start_folder = setter(|s: &mut Settings, v: String| {
        let v = v.trim();
        if !v.is_empty() {
            s.set_start_folder(v);
        }
    });
    let editor = setter(|s: &mut Settings, v: String| s.set_editor_command(v.trim()));
    let rows = vec![
        text_field("startFolder", de!("Projektordner für ⌘N (nach Git-Repos durchsucht)"), "", folder, Some(start_folder), browse),
        text_field("editorCommand", de!("Externer Editor (Kommando wie im Terminal)"), de!("z. B. code"), s.editor_command(), Some(editor), vec![]),
        slider("autoDetach", de!("Fertige Sessions automatisch trennen (0 = nie)"), s.auto_detach_minutes() as f64, AUTO_DETACH_RANGE, 5., de!("min"), true, |s, v| {
            s.set_auto_detach_minutes(v as i64)
        }),
        select(
            "closeTileOnExit",
            de!("Wenn Claude endet (zweimal ⌃C, /exit)"),
            vec![(t("keep"), tx(de!("Kachel bleibt, Klick setzt fort"))), (t("close"), tx(de!("Kachel schließen")))],
            t(if s.close_tile_on_exit() { "close" } else { "keep" }),
            setter(|s, v: String| s.set_close_tile_on_exit(v == "close")),
        ),
    ];
    block(de!("Sessions"), "", rows)
}

fn claude(c: &Context) -> Block {
    let s = c.settings;
    let rows = vec![
        on_off("claudeFullscreen", de!("Im Fullscreen starten (kein doppelter Text beim Scrollen)"), s.claude_fullscreen(), Settings::set_claude_fullscreen),
        on_off("claudeAllowBypass", de!("Bypass-Modus erlauben (--allow-dangerously-skip-permissions)"), s.claude_allow_bypass(), Settings::set_claude_allow_bypass),
        texts("claudeMode", de!("Startmodus"), &CLAUDE_MODES, s.claude_mode(), Settings::set_claude_mode),
        texts("claudeModel", de!("Modell"), &CLAUDE_MODELS, s.claude_model(), Settings::set_claude_model),
        texts("claudeEffort", de!("Effort"), &CLAUDE_EFFORTS, s.claude_effort(), Settings::set_claude_effort),
        on_off(
            "controlOtherSessions",
            de!("Sessions dürfen andere Sessions steuern (kadrell send, capture, kill)"),
            s.control_other_sessions(),
            Settings::set_control_other_sessions,
        ),
    ];
    block(de!("Claude"), de!("gilt für neu gestartete Claude-Prozesse"), rows)
}

fn account_rows(c: &Context) -> Vec<Row> {
    let active = c.backend.active_account();
    let accounts = c.backend.accounts();
    let mut rows: Vec<Row> = accounts.iter().map(|a| account_row(a, active.as_deref())).collect();
    let current = c.backend.current_email();
    if current.is_none() || !accounts.iter().any(|a| Some(&a.email) == current.as_ref()) {
        let text = current.unwrap_or_default();
        let add = Control::Info { text, tone: Tone::Muted, buttons: vec![Btn::text(de!("Hinzufügen"), Msg::AddAccount)] };
        rows.push(Row::new("addAccount", de!("Angemeldeten Account hinzufügen"), add));
    }
    rows
}

fn account_row(a: &super::backend::AccountInfo, active: Option<&str>) -> Row {
    let title = format!("{}{}", a.email, a.plan.as_ref().map(|p| format!(" · {p}")).unwrap_or_default());
    let mut buttons = Vec::new();
    let is_active = Some(a.id.as_str()) == active;
    if !is_active {
        buttons.push(Btn::text(de!("Wechseln"), Msg::SwitchAccount(a.id.clone())));
    }
    buttons.push(Btn::text(de!("Entfernen"), Msg::RemoveAccount(a.id.clone())));
    let text = if is_active { tx(de!("aktiv")) } else { String::new() };
    Row::new(&format!("account.{}", a.id), &title, Control::Info { text, tone: Tone::Accent, buttons })
}

fn accounts(c: &Context) -> Block {
    let s = c.settings;
    let mut rows = account_rows(c);
    rows.extend([
        on_off("autoswitchEnabled", de!("Automatisch wechseln bei Limit"), s.autoswitch_enabled(), Settings::set_autoswitch_enabled),
        slider("autoswitchThreshold", de!("Umschalten ab Auslastung"), s.autoswitch_threshold() as f64, AUTOSWITCH_RANGE, 1., "%", true, |s, v| {
            s.set_autoswitch_threshold(v as i64)
        }),
        on_off("autoswitchNotice", de!("Wechsel-Meldung anzeigen"), s.autoswitch_notice(), Settings::set_autoswitch_notice),
        on_off("autoswitchPaceWarning", de!("Warnen bei zu schnellem 7-Tage-Verbrauch"), s.autoswitch_pace_warning(), Settings::set_autoswitch_pace_warning),
        on_off("autoswitchOnPace", de!("Nach Wochentempo verteilen"), s.autoswitch_on_pace(), Settings::set_autoswitch_on_pace),
        on_off("barShowSession", de!("5-Stunden-Limit in der Leiste (bei Ultra nie)"), s.bar_show_session(), Settings::set_bar_show_session),
        on_off("barShowWeekly", de!("7-Tage-Limit in der Leiste"), s.bar_show_weekly(), Settings::set_bar_show_weekly),
        on_off("barShowFable", de!("Fable-Kontingent in der Leiste"), s.bar_show_fable(), Settings::set_bar_show_fable),
    ]);
    block(de!("Konten"), de!("mehrere Claude-Accounts, Umschalten ohne neues Login"), rows)
}

fn language_label(l: Language) -> &'static str {
    match l {
        Language::System => de!("🌐  Sprache des Systems"),
        Language::De => "Deutsch",
        Language::En => "English",
    }
}

fn status_bar_label(d: StatusBarDisplay) -> &'static str {
    match d {
        StatusBarDisplay::Full => de!("Symbol und Text"),
        StatusBarDisplay::Icon => de!("nur Symbol"),
        StatusBarDisplay::Hidden => de!("aus"),
    }
}

/// Auswahl der Terminal-Schrift: die mitgelieferte und alle installierten Monospace-Familien (`appearance::fonts`).
/// Eine gespeicherte, die nicht dabei ist, steht vorn (wie in der Mac-App).
fn fonts(current: &str) -> Vec<(String, String)> {
    crate::appearance::fonts::choices(current).into_iter().map(|(v, d)| (t(&v), t(&d))).collect()
}

fn theme_row(c: &Context) -> Row {
    let options = Theme::all().iter().map(|th| (t(th.id), t(th.name))).collect();
    select("colorTheme", de!("Farbschema"), options, t(c.theme_id), setter(|s, v: String| s.set_color_theme(&v)))
}

fn appearance(c: &Context) -> Block {
    let s = c.settings;
    let font_name = s.terminal_font_name();
    let font = select("terminalFontName", de!("Terminal-Schrift"), fonts(&font_name), font_name.clone(), setter(|s, v: String| s.set_terminal_font_name(&v)));
    let mut rows = vec![
        choice("language", de!("Sprache"), Language::ALL, language_label, s.language(), Settings::set_language),
        theme_row(c),
        choice("statusBar", de!("Menüleiste"), StatusBarDisplay::ALL, status_bar_label, s.status_bar(), Settings::set_status_bar),
        slider("uiScale", de!("UI-Größe"), s.ui_scale() * 100., UI_SCALE_PERCENT, 5., "%", false, |s, v| s.set_ui_scale(v / 100.)),
        font,
    ];
    rows.extend(terminal_rows(c));
    block(de!("Darstellung"), "", rows)
}

fn terminal_rows(c: &Context) -> Vec<Row> {
    let s = c.settings;
    let size_title = tx(de!("Terminal-Schriftgröße  ⌘+ ⌘- ⌘0  ⌘ Mausrad"));
    let mut rows = vec![
        slider("terminalFontSize", &size_title, s.terminal_font_size(), FONT_SIZES, 1., "pt", true, Settings::set_terminal_font_size),
        slider("terminalLineSpacing", de!("Zeilenabstand"), s.terminal_line_spacing() * 100., LINE_SPACING_PERCENT, 5., "%", true, |s, v| {
            s.set_terminal_line_spacing(v / 100.)
        }),
        slider("terminalPadding", de!("Innenabstand der Kacheln"), s.terminal_padding(), PIXEL_RANGE, 1., "px", true, Settings::set_terminal_padding),
        slider("terminalScrollback", de!("Verlauf zum Zurückscrollen"), s.terminal_scrollback() as f64, SCROLLBACK_RANGE, 1_000., de!("Zeilen"), true, |s, v| {
            s.set_terminal_scrollback(v as i64)
        }),
        slider("tileGap", de!("Abstand zwischen Kacheln"), s.tile_gap(), PIXEL_RANGE, 1., "px", true, Settings::set_tile_gap),
    ];
    if c.platform == Platform::Mac {
        rows.push(on_off("terminalMetal", de!("Terminal auf der GPU zeichnen (Metal)"), s.terminal_metal(), Settings::set_terminal_metal));
    }
    rows
}

fn customize(c: &Context) -> Block {
    let s = c.settings;
    let buttons = vec![Btn::icon(Icon::Image, Msg::Browse { row: t("backgroundImage"), files: true }), Btn::icon(Icon::Cross, Msg::Clear(t("backgroundImage")))];
    let image = text_field(
        "backgroundImage",
        de!("Hintergrundbild"),
        de!("kein Bild · Datei hineinziehen"),
        s.background_image(),
        Some(setter(|s, v: String| s.set_background_image(&v))),
        buttons,
    );
    let opacity = slider("tileOpacity", de!("Deckkraft von Baum und Kacheln"), s.tile_opacity() * 100., (0., 100.), 1., "%", true, |s, v| s.set_tile_opacity(v / 100.));
    block(de!("Anpassen"), de!("Bild hinter Baum und Kacheln"), vec![image, opacity])
}

fn sidebar_label(s: SidebarStyle) -> &'static str {
    if s == SidebarStyle::Classic { de!("Klassisch") } else { de!("Kompakt") }
}

fn sound_label(l: SoundLevel) -> &'static str {
    match l {
        SoundLevel::Off => de!("aus"),
        SoundLevel::Important => de!("nur wartet und fertig"),
        SoundLevel::All => de!("alle"),
    }
}

fn notification_label(l: NotificationLevel) -> &'static str {
    match l {
        NotificationLevel::Off => de!("aus"),
        NotificationLevel::Waiting => de!("nur wartet"),
        NotificationLevel::All => de!("wartet und fertig"),
    }
}

fn tree(c: &Context) -> Block {
    let s = c.settings;
    let rows = vec![
        choice("sidebarStyle", de!("Design des Baums"), SidebarStyle::ALL, sidebar_label, s.sidebar_style(), Settings::set_sidebar_style),
        on_off("sidebarShowAge", de!("Laufzeit im Baum"), s.sidebar_show_age(), Settings::set_sidebar_show_age),
        on_off("stackShowPath", de!("Pfad in Stack-Zeilen"), s.stack_show_path(), Settings::set_stack_show_path),
        on_off("showLastMessage", de!("Letzte Antwort von Claude im Baum"), s.show_last_message(), Settings::set_show_last_message),
        choice("sounds", de!("Sounds"), SoundLevel::ALL, sound_label, s.sounds(), Settings::set_sounds),
        choice("notifications", de!("Systembenachrichtigungen"), NotificationLevel::ALL, notification_label, s.notifications(), Settings::set_notifications),
    ];
    block(de!("Baum und Kacheln"), "", rows)
}

fn updates(c: &Context) -> Block {
    let row = on_off("checkForUpdates", de!("Nach Updates suchen"), c.settings.check_for_updates(), Settings::set_check_for_updates);
    block(de!("Updates"), "", vec![row])
}

/// Auswahl „Auto-Modus zeigt“: zwei Schalter als ein Dropdown.
fn auto_show_row(s: &Settings) -> Row {
    let options = [("waiting", de!("wartende")), ("running", de!("arbeitende")), ("both", de!("wartende und arbeitende")), ("none", de!("keine"))];
    let value = match (s.auto_waiting(), s.auto_running()) {
        (true, false) => "waiting",
        (false, true) => "running",
        (true, true) => "both",
        (false, false) => "none",
    };
    let set = setter(|s: &mut Settings, v: String| {
        s.set_auto_waiting(v == "waiting" || v == "both");
        s.set_auto_running(v == "running" || v == "both");
    });
    select("autoShow", de!("Zeigt"), options.iter().map(|(a, b)| (t(a), tx(b))).collect(), t(value), set)
}

fn auto_mode(c: &Context) -> Block {
    let s = c.settings;
    let scope = select(
        "autoAllSessions",
        de!("Gilt für"),
        vec![(t("all"), tx(de!("alle Sessions"))), (t("selection"), tx(de!("nur die Auswahl im Baum")))],
        t(if s.auto_all_sessions() { "all" } else { "selection" }),
        setter(|s, v: String| s.set_auto_all_sessions(v == "all")),
    );
    block(de!("Auto-Modus"), de!("AUTO in der Leiste"), vec![scope, auto_show_row(s)])
}

fn asks(c: &Context) -> Block {
    let rows = Ask::ALL
        .iter()
        .map(|&a| {
            let id = format!("ask.{}", a.raw());
            on_off(&id, a.title_key(), c.settings.ask_enabled(a), move |s, v| s.set_ask_enabled(a, v))
        })
        .collect();
    block(de!("Rückfragen"), de!("aus = ohne Nachfrage ausführen"), rows)
}

fn status_row(provider: Provider, c: &Context) -> Row {
    let (text, tone) = match c.backend.status(provider) {
        ProviderStatus::Off => (tx(de!("aus")), Tone::Muted),
        ProviderStatus::Checking => (tx(de!("prüft …")), Tone::Muted),
        ProviderStatus::Ready(v) => (v, Tone::Accent),
        ProviderStatus::Failed(e) => (e, Tone::Error),
    };
    let buttons = vec![Btn::text(de!("Neu laden"), Msg::Reload(provider))];
    Row::new(&format!("status.{provider:?}"), de!("Status"), Control::Info { text, tone, buttons })
}

fn model_rows(provider: Provider, c: &Context, armed: Option<&str>) -> Vec<Row> {
    let models = c.backend.models(provider);
    if models.is_empty() {
        return vec![Row::new(&format!("models.{provider:?}"), "", Control::Note(tx(de!("Keine Modelle installiert")))).unsearchable()];
    }
    models.into_iter().map(|m| model_row(&m, armed)).collect()
}

fn model_row(m: &super::backend::ModelInfo, armed: Option<&str>) -> Row {
    let is_armed = armed == Some(m.id.as_str());
    let mut buttons = Vec::new();
    if m.can_remove {
        let mut b = Btn::text(if is_armed { de!("Wirklich löschen") } else { de!("Löschen") }, Msg::DeleteModel(m.id.clone()));
        b.destructive = is_armed;
        buttons.push(b);
    }
    Row::new(&format!("model.{}", m.id), &m.name, Control::Model { detail: m.detail.clone(), buttons }).unsearchable()
}

fn ollama(c: &Context, armed: Option<&str>) -> Block {
    let s = c.settings;
    let enabled = s.bool_or("local.ollama.enabled", true);
    let mut rows = vec![on_off("ollamaEnabled", de!("Ollama nutzen"), enabled, |s, v| s.set("local.ollama.enabled", Value::Bool(v)))];
    if enabled {
        rows.push(string_field("ollamaUrl", de!("Adresse des Ollama-Dienstes"), "http://127.0.0.1:11434", "local.ollama.url", "http://127.0.0.1:11434", s));
        rows.push(status_row(Provider::Ollama, c));
        rows.extend(model_rows(Provider::Ollama, c, armed));
        let pull = vec![Btn::text(de!("Laden"), Msg::Pull(t("ollamaPull")))];
        rows.push(text_field("ollamaPull", de!("Modell laden (Name aus der Bibliothek)"), "qwen3:0.6b", String::new(), None, pull));
        let search = de!("Auf ollama.com suchen (Webseite, keine offizielle Schnittstelle)");
        rows.push(text_field("ollamaSearch", search, de!("z. B. qwen3"), String::new(), None, vec![]));
    }
    block(de!("Lokale Modelle: Ollama"), de!("gezeigt wird nur, was installiert ist"), rows)
}

fn llama_cpp(c: &Context, armed: Option<&str>) -> Block {
    let s = c.settings;
    let enabled = s.bool_or("local.llama.enabled", false);
    let mut rows = vec![on_off("llamaEnabled", de!("llama.cpp nutzen"), enabled, |s, v| s.set("local.llama.enabled", Value::Bool(v)))];
    if enabled {
        rows.push(string_field("llamaBinary", de!("Pfad zu llama-server (leer = automatisch)"), de!("automatisch"), "local.llama.binary", "", s));
        rows.push(string_field("llamaModelsDir", de!("Modellordner mit GGUF-Dateien (optional)"), de!("nur der Cache von llama.cpp"), "local.llama.modelsDir", "", s));
        rows.push(string_field("llamaUrl", de!("Adresse eines laufenden llama-server (optional)"), de!("leer = Kadrell startet ihn"), "local.llama.url", "", s));
        rows.push(status_row(Provider::LlamaCpp, c));
        rows.extend(model_rows(Provider::LlamaCpp, c, armed));
        let hint = de!("Neue Modelle: einmal im Terminal mit llama-server -hf <Repo> laden (landet im Cache) oder GGUF-Dateien in den Modellordner legen");
        rows.push(Row::new("llamaHint", "", Control::Note(tx(hint))).unsearchable());
    }
    block(de!("Lokale Modelle: llama.cpp"), de!("ohne Adresse startet Kadrell llama-server selbst, er endet mit Kadrell"), rows)
}

/// Funktion mit lokalem Modell: Schlüssel, Titel, Text für „kein Modell“.
struct Feature {
    key: &'static str,
    title: &'static str,
    no_model: &'static str,
}

const FEATURES: [Feature; 3] = [
    Feature { key: "quickStart", title: de!("Schnellstart: Idee rein, Session raus"), no_model: de!("kein Modell (nur Namenssuche)") },
    Feature { key: "piSession", title: de!("Pi-Sessions"), no_model: de!("kein Modell (nur Claude)") },
    Feature { key: "sessionTitle", title: de!("Sessiontitel"), no_model: de!("kein Modell (Titel von Claude Code)") },
];

fn feature_rows(f: &Feature, c: &Context) -> Vec<Row> {
    let s = c.settings;
    let (on_key, model_key) = (format!("local.feature.{}.enabled", f.key), format!("local.feature.{}.model", f.key));
    let enabled_key = on_key.clone();
    let mut rows = vec![on_off(&format!("feature.{}.enabled", f.key), f.title, s.bool_or(&on_key, false), move |s, v| s.set(&enabled_key, Value::Bool(v)))];
    let mut options = vec![(t(""), tx(f.no_model))];
    if f.key == "piSession" {
        options.push((t("pi-default"), tx(de!("Standard von Pi"))));
    }
    options.extend(c.backend.available_models());
    let current = s.string_or(&model_key, "");
    if !current.is_empty() && !options.iter().any(|(v, _)| *v == current) {
        options.push((current.clone(), trd!("%@ (nicht vorhanden)", current.as_str())));
    }
    // Ein gewähltes Modell schaltet die Funktion gleich mit an, „kein Modell“ lässt den Schalter, wie er ist.
    let set = setter(move |s: &mut Settings, v: String| {
        s.set(&model_key, Value::String(v.clone()));
        if !v.is_empty() {
            s.set(&on_key, Value::Bool(true));
        }
    });
    rows.push(select(&format!("feature.{}.model", f.key), de!("Modell"), options, current, set));
    if f.key == "quickStart" {
        let strip = s.bool_or("local.quickStart.stripReference", true);
        let title = de!("Projektverweis aus dem ersten Prompt entfernen, wenn sicher");
        rows.push(on_off("quickStartStrip", title, strip, |s, v| s.set("local.quickStart.stripReference", Value::Bool(v))));
    }
    rows
}

fn features(c: &Context) -> Block {
    let rows = FEATURES.iter().flat_map(|f| feature_rows(f, c)).collect();
    block(de!("Modelle für Funktionen"), de!("je Funktion an/aus und Modell aus den vorhandenen"), rows)
}

// MARK: Kürzel

/// Navigation; „Kacheln verwalten“ nimmt den Rest, damit keine Aktion verschwindet.
pub fn navigation_actions() -> Vec<HotkeyAction> {
    use HotkeyAction::*;
    let mut out = vec![FocusLeft, FocusRight, FocusUp, FocusDown, NextSession, PrevSession, LastSession, PreviewNext, PreviewPrev, NextWaiting];
    out.extend(HotkeyAction::ALL.iter().copied().filter(|a| a.tile_index().is_some()));
    out.extend([FocusSidebar, FocusWorkspace, FocusRightSidebar]);
    out
}

pub fn tile_actions() -> Vec<HotkeyAction> {
    let nav = navigation_actions();
    HotkeyAction::ALL.iter().copied().filter(|a| !nav.contains(a)).collect()
}

/// Titel einer Kürzel-Aktion in der eingestellten Sprache; „Kachel 1 bis 9“ stehen unter `Kachel %lld`.
pub fn hotkey_title(action: HotkeyAction) -> String {
    match action.tile_index() {
        Some(i) => trd!("Kachel %lld", i + 1),
        None => tx(&action.title_key()),
    }
}

fn hotkey_row(action: HotkeyAction, map: &hotkeys::HotkeyMap, platform: Platform) -> Row {
    let mut row = Row::new(action.raw(), &hotkey_title(action), Control::Hotkey(action));
    if let Some(k) = map.get(&action).filter(|k| !k.key.is_empty()) {
        row.search.push(shortcut::format(k, platform));
    }
    row
}

fn hotkey_block(first: bool, actions: Vec<HotkeyAction>, c: &Context) -> Block {
    let map = hotkeys::current(c.settings);
    let subtitle = if first { de!("Navigation") } else { de!("Kacheln verwalten") };
    let heading = first.then(|| (tx(de!("Tastenkürzel")), tx(de!("Kürzel anklicken, Tasten drücken · ⌫ entfernt"))));
    Block {
        heading,
        subtitle: Some(tx(subtitle)),
        rows: actions.into_iter().map(|a| hotkey_row(a, &map, c.platform)).collect(),
        keep_all: vec![tx(subtitle), tx(de!("Tastenkürzel"))],
    }
}

/// Alle Abschnitte in Lese- und Tab-Reihenfolge, die Spalten verteilen sich daraus von selbst.
/// `armed` ist das Modell, dessen „Löschen“ schon einmal gedrückt wurde.
pub fn build(c: &Context, armed: Option<&str>) -> Vec<Block> {
    vec![
        sessions(c),
        claude(c),
        accounts(c),
        appearance(c),
        customize(c),
        tree(c),
        updates(c),
        auto_mode(c),
        asks(c),
        ollama(c, armed),
        llama_cpp(c, armed),
        features(c),
        hotkey_block(true, navigation_actions(), c),
        hotkey_block(false, tile_actions(), c),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::settings::backend::MockBackend;

    fn built() -> Vec<Block> {
        let settings = Settings::default();
        let backend = MockBackend::new();
        build(&Context { settings: &settings, backend: &backend, theme_id: "mocha", platform: Platform::Windows }, None)
    }

    #[test]
    fn hotkey_parts_cover_all_47_actions_without_overlap() {
        assert_eq!(HotkeyAction::ALL.len(), 47);
        let (nav, tiles) = (navigation_actions(), tile_actions());
        assert_eq!(nav.len(), 22);
        assert_eq!(tiles.len(), 25);
        assert!(nav.iter().all(|a| !tiles.contains(a)));
        let mut all: Vec<_> = nav.into_iter().chain(tiles).collect();
        all.sort();
        let mut expected = HotkeyAction::ALL.to_vec();
        expected.sort();
        assert_eq!(all, expected);
    }

    #[test]
    fn fourteen_blocks_in_the_order_of_the_mac_app() {
        let blocks = built();
        assert_eq!(blocks.len(), 14);
        let heads: Vec<_> = blocks.iter().map(|b| b.heading.as_ref().map(|h| h.0.as_str()).unwrap_or("")).collect();
        assert_eq!(&heads[..4], ["Sessions", "Claude", "Konten", "Darstellung"]);
        assert_eq!(heads[12], "Tastenkürzel");
        assert_eq!(heads[13], "");
    }

    #[test]
    fn row_ids_are_unique() {
        let blocks = built();
        let mut ids: Vec<&str> = blocks.iter().flat_map(|b| b.rows.iter().map(|r| r.id.as_str())).collect();
        let n = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), n);
    }

    #[test]
    fn color_scheme_offers_all_twelve() {
        let blocks = built();
        let row = blocks[3].rows.iter().find(|r| r.id == "colorTheme").unwrap();
        let Control::Select { options, value, .. } = &row.control else { panic!("kein Dropdown") };
        assert_eq!(options.len(), 12);
        assert_eq!(value, "mocha");
    }

    #[test]
    fn a_setter_writes_straight_into_the_settings() {
        let blocks = built();
        let row = blocks[1].rows.iter().find(|r| r.id == "claudeModel").unwrap();
        let Control::Select { set, .. } = &row.control else { panic!("kein Dropdown") };
        let mut s = Settings::default();
        set(&mut s, "opus".to_string());
        assert_eq!(s.claude_model(), "opus");
    }

    #[test]
    fn choosing_a_feature_model_switches_the_feature_on() {
        let blocks = built();
        let row = blocks[11].rows.iter().find(|r| r.id == "feature.sessionTitle.model").unwrap();
        let Control::Select { set, .. } = &row.control else { panic!("kein Dropdown") };
        let mut s = Settings::default();
        set(&mut s, "ollama|qwen3:8b".to_string());
        assert!(s.bool_or("local.feature.sessionTitle.enabled", false));
        set(&mut s, String::new());
        assert!(s.bool_or("local.feature.sessionTitle.enabled", false));
    }

    #[test]
    fn auto_show_maps_two_switches_to_one_choice() {
        let mut s = Settings::default();
        s.set_auto_waiting(true);
        s.set_auto_running(true);
        let row = auto_show_row(&s);
        let Control::Select { value, set, .. } = &row.control else { panic!("kein Dropdown") };
        assert_eq!(value, "both");
        set(&mut s, "running".to_string());
        assert!(!s.auto_waiting() && s.auto_running());
    }
}
