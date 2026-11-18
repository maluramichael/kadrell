//! Einstellungen des Profils. Auf dem Mac liegen sie in `UserDefaults`, hier in `settings.json` im Profilordner:
//! ein flaches JSON-Objekt mit denselben Schlüsseln wie `App/Settings.swift` (`sidebar.style`, `claude.mode` ...).
//! Auch Zähler (`stats.*`), Kürzel (`hotkeys`), Layout-Verhältnisse (`layout.*`) und Schlüssel anderer Teile
//! stehen in derselben Datei und bleiben beim Speichern erhalten.
//!
//! Wie bei `UserDefaults` gilt: fehlt ein Schlüssel oder hat er den falschen Typ, gilt der Standardwert; Bereiche
//! werden beim Lesen begrenzt, nicht beim Schreiben.

use crate::group::SidebarSort;
use crate::json_file::write_atomic;
use crate::session::AgentKind;
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::path::Path;

/// Aufzählung mit Text als Rohwert (siehe `raw_enum!`).
pub trait RawEnum: Copy {
    fn from_raw(raw: &str) -> Option<Self>;
    fn raw(self) -> &'static str;
}

impl RawEnum for AgentKind {
    fn from_raw(raw: &str) -> Option<Self> {
        AgentKind::from_raw(raw)
    }
    fn raw(self) -> &'static str {
        AgentKind::raw(self)
    }
}

raw_enum! {
    /// Sprache der Oberfläche. Ohne Wahl gilt die Sprache des Systems.
    Language { System = "system", De = "de", En = "en" }
}

raw_enum! {
    /// Aussehen des Baums. Ein altes `tinted` fällt auf `Classic` zurück.
    SidebarStyle { Classic = "classic", Compact = "compact" }
}

raw_enum! {
    /// Welche Sounds Kadrell spielt.
    SoundLevel { Off = "off", Important = "important", All = "all" }
}

raw_enum! {
    /// Systembenachrichtigungen bei wartenden oder fertigen Sessions.
    NotificationLevel { Off = "off", Waiting = "waiting", All = "all" }
}

raw_enum! {
    /// Wie das Menüleisten-Item erscheint.
    StatusBarDisplay { Full = "full", Icon = "icon", Hidden = "hidden" }
}

raw_enum! {
    /// Rückfragen, die ein Häkchen „Nicht mehr fragen“ abschalten kann. Abgeschaltet heißt: Aktion läuft sofort.
    Ask {
        CloseSession = "closeSession",
        CloseGroup = "closeGroup",
        StopSession = "stopSession",
        Quit = "quit",
        AdoptBackground = "adoptBackground",
    }
}

impl Ask {
    /// Schlüssel in den Einstellungen.
    pub fn key(self) -> String {
        format!("ask.{}", self.raw())
    }

    /// Titel im Einstellungsdialog als deutscher Text; zugleich der Schlüssel in `Localizable.xcstrings`.
    pub fn title_key(self) -> &'static str {
        match self {
            Ask::CloseSession => "Session beenden und entfernen",
            Ask::CloseGroup => "Gruppe schließen",
            Ask::StopSession => "Session stoppen",
            Ask::Quit => "Kadrell beenden, während Claude läuft",
            Ask::AdoptBackground => "Hintergrund-Sessions übernehmen",
        }
    }
}

/// Erlaubte Werte der Start-Flags für Claude (Auswahllisten im Dialog), `""` = Claude-Standard.
pub const CLAUDE_MODES: [&str; 6] = ["", "acceptEdits", "auto", "plan", "dontAsk", "bypassPermissions"];
pub const CLAUDE_MODELS: [&str; 6] = ["", "fable", "opus", "claude-opus-4-8[1m]", "sonnet", "haiku"];
pub const CLAUDE_EFFORTS: [&str; 6] = ["", "low", "medium", "high", "xhigh", "max"];

/// Bereiche der Schieberegler (Prozentwerte werden als Faktor gespeichert).
pub const AUTO_DETACH_RANGE: (f64, f64) = (0.0, 240.0);
pub const AUTOSWITCH_RANGE: (f64, f64) = (50.0, 99.0);
pub const UI_SCALE_PERCENT: (f64, f64) = (50.0, 200.0);
pub const LINE_SPACING_PERCENT: (f64, f64) = (50.0, 300.0);
pub const PIXEL_RANGE: (f64, f64) = (0.0, 64.0);
pub const FONT_SIZES: (f64, f64) = (6.0, 72.0);
pub const DEFAULT_FONT_SIZE: f64 = 12.0;
pub const DEFAULT_FONT_NAME: &str = "JetBrainsMonoNF-Regular";
/// Zeilen Verlauf pro Terminal.
pub const SCROLLBACK_RANGE: (f64, f64) = (1_000.0, 50_000.0);

/// Üblich benannte Projektordner unter `~`, in dieser Reihenfolge geprüft (`Settings.defaultStartFolder`).
pub const START_FOLDER_CANDIDATES: [&str; 10] =
    ["development", "Development", "Projects", "projects", "code", "Code", "src", "workspace", "git", "Documents/GitHub"];

/// Schlüssel eines Wegwerfprofils, die nicht aus dem Standardprofil übernommen werden: Auswahl und Lesestand gehören
/// zu Sessions, die es dort nicht gibt.
const NOT_COPIED_TO_TEMP: [&str; 3] = ["session.lastSeenSize", "folders.uses", "windows.open"];

fn clamp((lo, hi): (f64, f64), v: f64) -> f64 {
    v.max(lo).min(hi)
}

/// Der Inhalt von `settings.json`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Settings {
    values: Map<String, Value>,
}

impl Settings {
    /// Aus dem Text einer `settings.json`. Kaputtes oder etwas anderes als ein Objekt ergibt leere Einstellungen,
    /// wie ein frisches Profil. Das ältere verschachtelte Format der Windows-Version wird übernommen.
    pub fn from_bytes(data: &[u8]) -> Settings {
        let mut values = match serde_json::from_slice::<Value>(data) {
            Ok(Value::Object(map)) => map,
            _ => Map::new(),
        };
        migrate_legacy_windows(&mut values);
        Settings { values }
    }

    /// Lädt die Datei, fehlt sie oder ist sie kaputt, sind die Einstellungen leer.
    pub fn load(path: &Path) -> Settings {
        std::fs::read(path).map(|d| Settings::from_bytes(&d)).unwrap_or_default()
    }

    /// Schreibt die Datei atomar.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        write_atomic(path, &serde_json::to_vec_pretty(&self.values).unwrap_or_default())
    }

    /// Einstellungen für ein neues Wegwerfprofil: die des Standardprofils ohne Auswahl und Lesestand.
    pub fn for_temp_profile(&self) -> Settings {
        let mut copy = self.clone();
        copy.values.retain(|k, _| !k.starts_with("workspace.selected") && !NOT_COPIED_TO_TEMP.contains(&k.as_str()));
        copy
    }

    // MARK: Rohzugriff (wie UserDefaults)

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.values.get(key)
    }

    pub fn contains(&self, key: &str) -> bool {
        self.values.contains_key(key)
    }

    pub fn set(&mut self, key: &str, value: Value) {
        self.values.insert(key.to_string(), value);
    }

    pub fn remove(&mut self, key: &str) {
        self.values.remove(key);
    }

    /// Alle Schlüssel, sortiert.
    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.values.keys()
    }

    pub fn bool_or(&self, key: &str, default: bool) -> bool {
        self.get(key).and_then(Value::as_bool).unwrap_or(default)
    }

    pub fn f64_or(&self, key: &str, default: f64) -> f64 {
        self.get(key).and_then(Value::as_f64).unwrap_or(default)
    }

    /// Ganze Zahl; eine Kommazahl ohne Rest (`2.0`) zählt auch.
    pub fn i64_or(&self, key: &str, default: i64) -> i64 {
        let v = self.get(key);
        v.and_then(Value::as_i64).or_else(|| v.and_then(Value::as_f64).filter(|f| f.fract() == 0.0).map(|f| f as i64)).unwrap_or(default)
    }

    pub fn string_or(&self, key: &str, default: &str) -> String {
        self.opt_string(key).unwrap_or_else(|| default.to_string())
    }

    pub fn opt_string(&self, key: &str) -> Option<String> {
        self.get(key).and_then(Value::as_str).map(str::to_string)
    }

    /// Liste von Text; sind es nicht nur Texte, gilt sie als leer.
    pub fn strings_or(&self, key: &str) -> Vec<String> {
        let Some(Value::Array(items)) = self.get(key) else { return Vec::new() };
        items.iter().map(|v| v.as_str().map(str::to_string)).collect::<Option<Vec<_>>>().unwrap_or_default()
    }

    /// Zuordnung Text zu Text; sind nicht alle Werte Texte, gilt sie als leer.
    pub fn string_map(&self, key: &str) -> BTreeMap<String, String> {
        let Some(Value::Object(map)) = self.get(key) else { return BTreeMap::new() };
        map.iter().map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string()))).collect::<Option<_>>().unwrap_or_default()
    }

    pub fn enum_or<T: RawEnum>(&self, key: &str, default: T) -> T {
        self.get(key).and_then(Value::as_str).and_then(T::from_raw).unwrap_or(default)
    }

    /// Schreibt eine Kommazahl (ganzzahlige als `10.0`, wie `UserDefaults` einen Double ablegt).
    pub fn set_f64(&mut self, key: &str, value: f64) {
        self.set(key, serde_json::Number::from_f64(value).map_or(Value::Null, Value::Number));
    }

    // MARK: Sprache, Ordner, Agent

    enum_setting! {
        /// Sprache der Oberfläche, Standard: die des Systems.
        language, set_language, "language", Language, Language::System
    }

    /// Hat der Nutzer eine Sprache gewählt? Sonst bietet der Leerzustand die Flaggen an.
    pub fn language_chosen(&self) -> bool {
        self.opt_string("language").is_some()
    }

    /// Startordner für neue Sessions (⌘N), immer mit abschließendem Schrägstrich, `~` ist aufgelöst. Ohne Wert der
    /// erste vorhandene Projektordner unter `home`, sonst `home`.
    pub fn start_folder(&self, home: &Path) -> String {
        let raw = self.opt_string("startFolder").unwrap_or_else(|| default_start_folder(home));
        let expanded = match raw.strip_prefix('~') {
            Some(rest) => format!("{}{rest}", home.to_string_lossy().trim_end_matches(['/', '\\'])),
            None => raw,
        };
        if expanded.ends_with('/') || expanded.ends_with('\\') { expanded } else { expanded + "/" }
    }

    pub fn set_start_folder(&mut self, folder: &str) {
        self.set("startFolder", Value::String(folder.to_string()));
    }

    /// Zuletzt gewählter Agent. Ohne Wahl gilt die frühere Schnellstart-Einstellung (Pi).
    pub fn last_agent(&self) -> AgentKind {
        let fallback = if self.bool_or("local.quickStart.usePi", false) { AgentKind::Pi } else { AgentKind::Claude };
        self.enum_or("lastAgent", fallback)
    }

    pub fn set_last_agent(&mut self, kind: AgentKind) {
        self.set("lastAgent", Value::String(kind.raw().to_string()));
    }

    string_setting! {
        /// Kommando des externen Editors (`code`, `subl`, `zed`), `""` = Kürzel tut nichts.
        editor_command, set_editor_command, "editorCommand", ""
    }

    // MARK: Aussehen

    bool_setting! {
        /// Letzte Antwort von Claude als zweite Zeile unter jeder Session im Baum.
        show_last_message, set_show_last_message, "showLastMessage", false
    }

    string_setting! {
        /// Farbschema der Oberfläche und Terminals, Standard Instrument Dark.
        color_theme, set_color_theme, "colorTheme", "instrument-dark"
    }

    bool_setting! {
        /// Ob schon einmal die Hilfe gezeigt wurde (Erststart-Zweig).
        help_shown, set_help_shown, "helpShown", false
    }

    /// Bestehende Profile (schon einmal gestartet, kein Schema gewählt) behalten Catppuccin Mocha, das bis zum
    /// Wechsel des Standards auf Instrument galt. Nur neue Installationen starten mit Instrument. Läuft pro Profil
    /// genau einmal (`themePinChecked`), sonst würde eine neue Installation beim zweiten Start auf Mocha
    /// festgenagelt, weil der erste `helpShown` gesetzt hat. Muss vor dem ersten Lesen des Themes laufen.
    pub fn pin_theme_for_existing_profile(&mut self) {
        if self.bool_or("themePinChecked", false) {
            return;
        }
        if self.help_shown() && !self.contains("colorTheme") {
            self.set_color_theme("mocha");
        }
        self.set("themePinChecked", Value::Bool(true));
    }

    enum_setting! {
        /// Aussehen des Baums, Standard Klassisch.
        sidebar_style, set_sidebar_style, "sidebar.style", SidebarStyle, SidebarStyle::Classic
    }

    enum_setting! {
        /// Sortierung des Baums, Standard aus (Handreihenfolge).
        sidebar_sort, set_sidebar_sort, "sidebar.sort", SidebarSort, SidebarSort::Off
    }

    bool_setting! {
        /// Gruppierung nach Projekt im Baum. Aus = eine flache Liste aller Sessions.
        sidebar_grouped, set_sidebar_grouped, "sidebar.grouped", true
    }

    strings_setting! {
        /// Von Hand gezogene Reihenfolge der flachen Liste, neben der aus `groups.json`.
        sidebar_flat_order, set_sidebar_flat_order, "sidebar.flatOrder"
    }

    bool_setting! {
        /// Laufzeit („12m“) in jeder Session-Zeile des Baums.
        sidebar_show_age, set_sidebar_show_age, "sidebar.showAge", true
    }

    enum_setting! {
        sounds, set_sounds, "sounds", SoundLevel, SoundLevel::All
    }

    enum_setting! {
        /// Systembenachrichtigungen, Standard nur wartet.
        notifications, set_notifications, "notifications", NotificationLevel, NotificationLevel::Waiting
    }

    enum_setting! {
        status_bar, set_status_bar, "statusBar.display", StatusBarDisplay, StatusBarDisplay::Full
    }

    bool_setting! {
        /// Beim Start und danach alle 24 h auf eine neuere Version prüfen.
        check_for_updates, set_check_for_updates, "checkForUpdates", true
    }

    bool_setting! {
        /// Stack-Zeilen zeigen zusätzlich den Pfad der Session.
        stack_show_path, set_stack_show_path, "stackShowPath", true
    }

    bool_setting! {
        /// Beendet sich Claude selbst, verschwindet die Kachel. Standard aus: sie bleibt, Klick setzt fort.
        close_tile_on_exit, set_close_tile_on_exit, "closeTileOnExit", false
    }

    /// Minuten, die eine fertige Session ohne Arbeit bleiben darf, bevor ihr Claude-Prozess getrennt wird. 0 = nie.
    pub fn auto_detach_minutes(&self) -> i64 {
        clamp(AUTO_DETACH_RANGE, self.f64_or("autoDetachMinutes", 0.0)) as i64
    }

    pub fn set_auto_detach_minutes(&mut self, minutes: i64) {
        self.set_f64("autoDetachMinutes", minutes as f64);
    }

    // MARK: Auto-Modus

    bool_setting! {
        /// Auto-Modus zeigt aus der Auswahl nur Sessions in diesen Zuständen. Standard: nur wartende.
        auto_waiting, set_auto_waiting, "auto.waiting", true
    }
    bool_setting! {
        auto_running, set_auto_running, "auto.running", false
    }
    bool_setting! {
        /// Auto-Modus filtert alle Sessions des Baums statt nur der Auswahl.
        auto_all_sessions, set_auto_all_sessions, "auto.allSessions", true
    }

    // MARK: Claude

    bool_setting! {
        claude_allow_bypass, set_claude_allow_bypass, "claude.allowBypass", false
    }
    string_setting! {
        /// Berechtigungsmodus (`--permission-mode`), siehe [`CLAUDE_MODES`].
        claude_mode, set_claude_mode, "claude.mode", ""
    }
    string_setting! {
        /// Modell, siehe [`CLAUDE_MODELS`].
        claude_model, set_claude_model, "claude.model", ""
    }
    string_setting! {
        /// Effort, siehe [`CLAUDE_EFFORTS`].
        claude_effort, set_claude_effort, "claude.effort", ""
    }
    bool_setting! {
        /// Claude im Fullscreen-Renderer starten (`CLAUDE_CODE_NO_FLICKER=1`).
        claude_fullscreen, set_claude_fullscreen, "claude.fullscreen", true
    }
    string_setting! {
        /// Pfad zu `claude`, leer = über PATH suchen. Nur auf Windows und im Rust-Kern gebräuchlich.
        claude_binary, set_claude_binary, "claudeBinary", ""
    }
    bool_setting! {
        /// `kadrell` aus einer Session heraus darf Sessions anderer Gruppen lesen und steuern. Aus: nur die eigene.
        control_other_sessions, set_control_other_sessions, "control.otherSessions", true
    }

    // MARK: Extensions und Konten

    strings_setting! {
        /// Eingeschaltete Extensions (Ordnernamen). Bleibt auch für verschwundene stehen.
        enabled_extensions, set_enabled_extensions, "extensions.enabled"
    }
    strings_setting! {
        /// Extensions, deren ungesandboxten Codelauf der Nutzer einmal bestätigt hat.
        acknowledged_extensions, set_acknowledged_extensions, "extensions.acknowledged"
    }

    /// Herkunft je aus dem Marktplatz installierter Extension (`owner/repo`).
    pub fn extension_origins(&self) -> BTreeMap<String, String> {
        self.string_map("extensions.origins")
    }

    pub fn set_extension_origins(&mut self, origins: &BTreeMap<String, String>) {
        self.set("extensions.origins", serde_json::json!(origins));
    }

    /// Hinterlegte Claude-Konten (nur Metadaten), als JSON-Text. Auf dem Mac ein `Data`-Blob.
    pub fn accounts_index(&self) -> Option<String> {
        self.opt_string("accounts.index")
    }

    pub fn set_accounts_index(&mut self, json: Option<&str>) {
        match json {
            Some(j) => self.set("accounts.index", Value::String(j.to_string())),
            None => self.remove("accounts.index"),
        }
    }

    /// Id des aktiven Kontos (welches gerade im Schlüsselbund liegt).
    pub fn active_account_id(&self) -> Option<String> {
        self.opt_string("accounts.active")
    }

    pub fn set_active_account_id(&mut self, id: Option<&str>) {
        match id {
            Some(i) => self.set("accounts.active", Value::String(i.to_string())),
            None => self.remove("accounts.active"),
        }
    }

    bool_setting! {
        /// Bei erreichtem Limit automatisch auf das nächste Konto wechseln.
        autoswitch_enabled, set_autoswitch_enabled, "autoswitch.enabled", false
    }
    bool_setting! {
        /// Nach einem automatischen Wechsel einmal melden, welches Konto jetzt aktiv ist.
        autoswitch_notice, set_autoswitch_notice, "autoswitch.notice", true
    }
    bool_setting! {
        /// Warnen, sobald das 7-Tage-Limit schneller vollläuft als der gleichmäßige Wochenplan erlaubt.
        autoswitch_pace_warning, set_autoswitch_pace_warning, "autoswitch.paceWarning", true
    }
    bool_setting! {
        /// Auch wechseln, wenn das 7-Tage-Limit zu schnell vollläuft und ein Konto mit mehr Runway frei ist.
        autoswitch_on_pace, set_autoswitch_on_pace, "autoswitch.onPace", false
    }

    /// Ab welcher Auslastung (5 h oder 7 Tage) der Auto-Wechsel greift, 50 bis 99.
    pub fn autoswitch_threshold(&self) -> i64 {
        clamp(AUTOSWITCH_RANGE, self.f64_or("autoswitch.threshold", 90.0)) as i64
    }

    pub fn set_autoswitch_threshold(&mut self, percent: i64) {
        self.set_f64("autoswitch.threshold", percent as f64);
    }

    bool_setting! {
        /// Welche Nutzungswerte die Leiste zeigt.
        bar_show_session, set_bar_show_session, "bar.usage.session", true
    }
    bool_setting! {
        bar_show_weekly, set_bar_show_weekly, "bar.usage.weekly", true
    }
    bool_setting! {
        bar_show_fable, set_bar_show_fable, "bar.usage.fable", true
    }

    // MARK: Terminal und Kacheln

    f64_setting! {
        /// UI-Größe als Faktor (0,5 bis 2), ohne Begrenzung beim Lesen.
        ui_scale, set_ui_scale, "uiScale", 1.0
    }

    /// Schriftgröße im Terminal, 6 bis 72.
    pub fn terminal_font_size(&self) -> f64 {
        clamp(FONT_SIZES, self.f64_or("terminal.fontSize", DEFAULT_FONT_SIZE))
    }

    pub fn set_terminal_font_size(&mut self, size: f64) {
        self.set_f64("terminal.fontSize", clamp(FONT_SIZES, size));
    }

    string_setting! {
        terminal_font_name, set_terminal_font_name, "terminal.fontName", DEFAULT_FONT_NAME
    }
    f64_setting! {
        /// Zeilenabstand als Faktor (0,5 bis 3).
        terminal_line_spacing, set_terminal_line_spacing, "terminal.lineSpacing", 1.0
    }

    /// Zeilen Verlauf pro Terminal, 1000 bis 50 000.
    pub fn terminal_scrollback(&self) -> i64 {
        clamp(SCROLLBACK_RANGE, self.f64_or("terminal.scrollback", 10_000.0)) as i64
    }

    pub fn set_terminal_scrollback(&mut self, lines: i64) {
        self.set_f64("terminal.scrollback", lines as f64);
    }

    f64_setting! {
        /// Abstand zwischen Kachelrahmen und Terminaltext, zusätzlich zu den 2 px Rahmenschutz.
        terminal_padding, set_terminal_padding, "terminal.padding", 0.0
    }
    bool_setting! {
        /// Terminal auf der GPU zeichnen (nur Mac-Swift-App, hier der Vollständigkeit halber).
        terminal_metal, set_terminal_metal, "terminal.metal", true
    }
    string_setting! {
        /// Bild hinter den Kacheln, `""` = keins.
        background_image, set_background_image, "workspace.backgroundImage", ""
    }
    f64_setting! {
        /// Deckkraft des Kachelkörpers samt Terminal-Hintergrund, 1 = undurchsichtig.
        tile_opacity, set_tile_opacity, "tiles.opacity", 1.0
    }
    f64_setting! {
        /// Abstand zwischen den Kacheln und zum Rand der Arbeitsfläche.
        tile_gap, set_tile_gap, "tiles.gap", 6.0
    }

    // MARK: Layout

    /// Feste Spaltenzahl im Grid, 0 = automatisch.
    pub fn grid_columns(&self) -> i64 {
        self.i64_or("layout.grid.columns", 0)
    }

    pub fn set_grid_columns(&mut self, columns: i64) {
        self.set("layout.grid.columns", Value::from(columns));
    }

    /// Sichtbare Spalten beim Scrollen, mindestens 1.
    pub fn scroll_columns(&self) -> i64 {
        self.i64_or("layout.scroll.columns", 2).max(1)
    }

    pub fn set_scroll_columns(&mut self, columns: i64) {
        self.set("layout.scroll.columns", Value::from(columns.max(1)));
    }

    string_setting! {
        /// Layout Frei: Teilungsrichtung je Position, `r` rechts, `d` unten, `a` längere Seite.
        custom_splits, set_custom_splits, "workspace.custom.splits", ""
    }

    /// Gezogene Verhältnisse einer Layout-Vorlage, `None` = gleich verteilt.
    pub fn layout_ratios(&self, key: &str) -> Option<Vec<f64>> {
        let Some(Value::Array(items)) = self.get(&format!("layout.{key}")) else { return None };
        items.iter().map(Value::as_f64).collect()
    }

    pub fn set_layout_ratios(&mut self, key: &str, ratios: Option<&[f64]>) {
        match ratios {
            Some(r) => self.set(&format!("layout.{key}"), serde_json::json!(r)),
            None => self.remove(&format!("layout.{key}")),
        }
    }

    /// Alle gezogenen Verhältnisse weg, die Spaltenzahlen bleiben.
    pub fn reset_layout_ratios(&mut self) {
        self.values.retain(|k, _| !k.starts_with("layout.") || k == "layout.grid.columns" || k == "layout.scroll.columns");
    }

    // MARK: Rückfragen

    /// Soll diese Rückfrage kommen? Standard ja.
    pub fn ask_enabled(&self, ask: Ask) -> bool {
        self.bool_or(&ask.key(), true)
    }

    pub fn set_ask_enabled(&mut self, ask: Ask, enabled: bool) {
        self.set(&ask.key(), Value::Bool(enabled));
    }
}

/// Erster vorhandener Kandidat aus [`START_FOLDER_CANDIDATES`], sonst `home`. `~` selbst als Startordner würde beim
/// ersten Öffnen Schreibtisch, Dokumente und Downloads scannen (TCC-Abfragen auf dem Mac).
fn default_start_folder(home: &Path) -> String {
    let dir = START_FOLDER_CANDIDATES.iter().map(|c| home.join(c)).find(|p| p.is_dir()).unwrap_or_else(|| home.to_path_buf());
    dir.to_string_lossy().replace('\\', "/")
}

/// Start-Flags für den Claude-Prozess, wie `ClaudeCLI.launchArgs`. Ein leerer Text heißt Claude-Standard, das Flag
/// entfällt.
pub fn claude_launch_args(allow_bypass: bool, mode: &str, model: &str, effort: &str) -> Vec<String> {
    let mut args = Vec::new();
    if allow_bypass {
        args.push("--allow-dangerously-skip-permissions".to_string());
    }
    for (flag, value) in [("--permission-mode", mode), ("--model", model), ("--effort", effort)] {
        if !value.is_empty() {
            args.extend([flag.to_string(), value.to_string()]);
        }
    }
    args
}

/// Hebt das verschachtelte Format der ersten Windows-Version (`{"claude":{"mode":...},"controlOtherSessions":...}`)
/// auf die flachen Schlüssel. Schon vorhandene flache Schlüssel gewinnen.
fn migrate_legacy_windows(values: &mut Map<String, Value>) {
    if let Some(Value::Object(claude)) = values.get("claude").cloned() {
        values.remove("claude");
        for (field, key) in
            [("allowBypass", "claude.allowBypass"), ("mode", "claude.mode"), ("model", "claude.model"), ("effort", "claude.effort"), ("fullscreen", "claude.fullscreen")]
        {
            if let (Some(v), false) = (claude.get(field), values.contains_key(key)) {
                values.insert(key.to_string(), v.clone());
            }
        }
    }
    if let Some(v) = values.remove("controlOtherSessions") {
        values.entry("control.otherSessions".to_string()).or_insert(v);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::testutil::TempDir;

    #[test]
    fn defaults_match_swift() {
        let s = Settings::default();
        assert_eq!(s.language(), Language::System);
        assert!(!s.language_chosen());
        assert_eq!(s.color_theme(), "instrument-dark");
        assert_eq!(s.sidebar_style(), SidebarStyle::Classic);
        assert_eq!(s.sidebar_sort(), SidebarSort::Off);
        assert!(s.sidebar_grouped() && s.sidebar_show_age() && s.stack_show_path() && s.check_for_updates());
        assert!(!s.show_last_message() && !s.close_tile_on_exit());
        assert_eq!((s.sounds(), s.notifications(), s.status_bar()), (SoundLevel::All, NotificationLevel::Waiting, StatusBarDisplay::Full));
        assert_eq!(s.auto_detach_minutes(), 0);
        assert!(s.auto_waiting() && !s.auto_running() && s.auto_all_sessions());
        assert!(!s.claude_allow_bypass() && s.claude_fullscreen() && s.control_other_sessions());
        assert_eq!((s.claude_mode(), s.claude_model(), s.claude_effort(), s.claude_binary()), (String::new(), String::new(), String::new(), String::new()));
        assert_eq!(s.autoswitch_threshold(), 90);
        assert!(!s.autoswitch_enabled() && s.autoswitch_notice() && s.autoswitch_pace_warning() && !s.autoswitch_on_pace());
        assert!(s.bar_show_session() && s.bar_show_weekly() && s.bar_show_fable());
        assert_eq!((s.ui_scale(), s.terminal_font_size(), s.terminal_line_spacing(), s.terminal_padding()), (1.0, 12.0, 1.0, 0.0));
        assert_eq!(s.terminal_font_name(), "JetBrainsMonoNF-Regular");
        assert_eq!(s.terminal_scrollback(), 10_000);
        assert!(s.terminal_metal());
        assert_eq!((s.tile_opacity(), s.tile_gap()), (1.0, 6.0));
        assert_eq!((s.grid_columns(), s.scroll_columns()), (0, 2));
        assert_eq!((s.background_image(), s.custom_splits(), s.editor_command()), (String::new(), String::new(), String::new()));
        assert!(s.sidebar_flat_order().is_empty() && s.enabled_extensions().is_empty() && s.extension_origins().is_empty());
        assert_eq!((s.accounts_index(), s.active_account_id()), (None, None));
        assert_eq!(s.last_agent(), AgentKind::Claude);
        assert!(Ask::ALL.iter().all(|a| s.ask_enabled(*a)));
    }

    #[test]
    fn ranges_are_clamped_on_read() {
        let mut s = Settings::default();
        s.set_f64("autoDetachMinutes", 9999.0);
        assert_eq!(s.auto_detach_minutes(), 240);
        s.set_f64("autoDetachMinutes", -5.0);
        assert_eq!(s.auto_detach_minutes(), 0);
        s.set_autoswitch_threshold(10);
        assert_eq!(s.autoswitch_threshold(), 50);
        s.set_autoswitch_threshold(120);
        assert_eq!(s.autoswitch_threshold(), 99);
        s.set_terminal_font_size(500.0);
        assert_eq!(s.terminal_font_size(), 72.0);
        s.set("terminal.fontSize", Value::from(1));
        assert_eq!(s.terminal_font_size(), 6.0);
        s.set_terminal_scrollback(10);
        assert_eq!(s.terminal_scrollback(), 1_000);
        s.set_terminal_scrollback(1_000_000);
        assert_eq!(s.terminal_scrollback(), 50_000);
        s.set_scroll_columns(-3);
        assert_eq!(s.scroll_columns(), 1);
        s.set_ui_scale(7.0);
        assert_eq!(s.ui_scale(), 7.0, "uiScale wird beim Lesen nicht begrenzt");
    }

    #[test]
    fn wrong_types_fall_back_to_defaults() {
        let s = Settings::from_bytes(r#"{"sidebar.grouped":"ja","uiScale":"groß","sidebar.style":"tinted","extensions.enabled":["a",1],"layout.grid.columns":2.5}"#.as_bytes());
        assert!(s.sidebar_grouped());
        assert_eq!(s.ui_scale(), 1.0);
        assert_eq!(s.sidebar_style(), SidebarStyle::Classic, "ein altes tinted startet mit Klassisch");
        assert!(s.enabled_extensions().is_empty());
        assert_eq!(s.grid_columns(), 0);
        assert_eq!(SidebarStyle::ALL, [SidebarStyle::Classic, SidebarStyle::Compact]);
        assert_eq!(Settings::from_bytes(br#"{"layout.grid.columns":3.0}"#).grid_columns(), 3);
    }

    #[test]
    fn typed_values_roundtrip_through_the_file() {
        let d = TempDir::new();
        let path = d.path().join("settings.json");
        let mut s = Settings::default();
        s.set_language(Language::De);
        s.set_sidebar_sort(SidebarSort::Status);
        s.set_claude_model("sonnet");
        s.set_auto_detach_minutes(30);
        s.set_enabled_extensions(&["git".to_string()]);
        s.set_last_agent(AgentKind::Pi);
        s.set_layout_ratios("grid", Some(&[0.3, 0.7]));
        s.set("stats.messages", Value::from(4));
        s.save(&path).unwrap();
        let back = Settings::load(&path);
        assert_eq!(back, s);
        assert_eq!((back.language(), back.sidebar_sort(), back.claude_model()), (Language::De, SidebarSort::Status, "sonnet".to_string()));
        assert!(back.language_chosen());
        assert_eq!(back.auto_detach_minutes(), 30);
        assert_eq!(back.last_agent(), AgentKind::Pi);
        assert_eq!(back.layout_ratios("grid"), Some(vec![0.3, 0.7]));
        assert_eq!(back.get("stats.messages"), Some(&Value::from(4)), "fremde Schlüssel bleiben erhalten");
    }

    #[test]
    fn broken_or_missing_file_means_empty_settings() {
        let d = TempDir::new();
        assert_eq!(Settings::load(&d.path().join("nope.json")), Settings::default());
        assert_eq!(Settings::from_bytes(b"{kaputt"), Settings::default());
        assert_eq!(Settings::from_bytes(b"[1,2]"), Settings::default());
    }

    #[test]
    fn legacy_agent_setting_decides_the_fallback() {
        let mut s = Settings::default();
        s.set("local.quickStart.usePi", Value::Bool(true));
        assert_eq!(s.last_agent(), AgentKind::Pi);
        s.set_last_agent(AgentKind::Claude);
        assert_eq!(s.last_agent(), AgentKind::Claude);
    }

    #[test]
    fn start_folder_expands_tilde_and_ends_with_slash() {
        let d = TempDir::new();
        std::fs::create_dir_all(d.path().join("git")).unwrap();
        std::fs::create_dir_all(d.path().join("src")).unwrap();
        let mut s = Settings::default();
        let home = d.path();
        let default = s.start_folder(home);
        assert!(default.ends_with("/src/"), "erster vorhandener Kandidat: {default}");
        s.set_start_folder("~/code");
        assert!(s.start_folder(home).ends_with("/code/") && !s.start_folder(home).starts_with('~'));
        s.set_start_folder("/x/y/");
        assert_eq!(s.start_folder(home), "/x/y/");
        let empty = TempDir::new();
        let none = Settings::default().start_folder(empty.path());
        assert!(none.ends_with('/') && none.contains("kadrell-core-test"), "ohne Kandidat gilt home: {none}");
    }

    #[test]
    fn existing_profile_keeps_mocha() {
        let mut s = Settings::default();
        s.set_help_shown(true);
        s.pin_theme_for_existing_profile();
        assert_eq!(s.get("colorTheme"), Some(&Value::String("mocha".into())));
    }

    #[test]
    fn fresh_profile_gets_instrument_and_keeps_it_on_second_launch() {
        let mut s = Settings::default();
        s.pin_theme_for_existing_profile();
        assert!(!s.contains("colorTheme"));
        s.set_help_shown(true);
        s.pin_theme_for_existing_profile();
        assert!(!s.contains("colorTheme"), "zweiter Start nagelt nicht auf Mocha fest");
        assert_eq!(s.color_theme(), "instrument-dark");
    }

    #[test]
    fn chosen_theme_is_untouched() {
        let mut s = Settings::default();
        s.set_help_shown(true);
        s.set_color_theme("nord");
        s.pin_theme_for_existing_profile();
        assert_eq!(s.color_theme(), "nord");
    }

    #[test]
    fn temp_profile_copy_drops_selection_and_read_state() {
        let mut s = Settings::default();
        for k in ["workspace.selected", "workspace.selected.x", "session.lastSeenSize", "folders.uses", "windows.open"] {
            s.set(k, Value::from(1));
        }
        s.set("workspace.custom.splits", Value::from("r"));
        s.set_claude_model("opus");
        let t = s.for_temp_profile();
        // Sortiert vergleichen: im Workspace schaltet eine Abhängigkeit serde_json/preserve_order ein.
        let mut keys = t.keys().cloned().collect::<Vec<_>>();
        keys.sort();
        assert_eq!(keys, ["claude.model", "workspace.custom.splits"]);
    }

    #[test]
    fn legacy_windows_format_is_lifted() {
        let s = Settings::from_bytes(br#"{"claude":{"mode":"auto","fullscreen":false},"controlOtherSessions":false,"claudeBinary":"C:/c.exe"}"#);
        assert_eq!(s.claude_mode(), "auto");
        assert!(!s.claude_fullscreen() && !s.control_other_sessions());
        assert_eq!(s.claude_binary(), "C:/c.exe");
        assert!(!s.contains("claude") && !s.contains("controlOtherSessions"));
        let both = Settings::from_bytes(br#"{"claude":{"mode":"auto"},"claude.mode":"plan"}"#);
        assert_eq!(both.claude_mode(), "plan", "flache Schlüssel gewinnen");
        let partial = Settings::from_bytes(br#"{"claude":{"mode":"auto"}}"#);
        assert!(partial.claude_fullscreen() && partial.control_other_sessions());
    }

    #[test]
    fn layout_ratios_reset_keeps_column_counts() {
        let mut s = Settings::default();
        s.set_grid_columns(3);
        s.set_scroll_columns(4);
        s.set_layout_ratios("spiral", Some(&[0.5]));
        s.set_layout_ratios("main", Some(&[0.6, 0.4]));
        s.reset_layout_ratios();
        assert_eq!((s.layout_ratios("spiral"), s.layout_ratios("main")), (None, None));
        assert_eq!((s.grid_columns(), s.scroll_columns()), (3, 4));
    }

    #[test]
    fn asks_and_extension_origins() {
        let mut s = Settings::default();
        s.set_ask_enabled(Ask::Quit, false);
        assert!(!s.ask_enabled(Ask::Quit) && s.ask_enabled(Ask::CloseGroup));
        assert_eq!(Ask::Quit.key(), "ask.quit");
        let mut origins = BTreeMap::new();
        origins.insert("git".to_string(), "owner/repo".to_string());
        s.set_extension_origins(&origins);
        assert_eq!(s.extension_origins(), origins);
        s.set_accounts_index(Some("[]"));
        s.set_active_account_id(Some("a1"));
        assert_eq!((s.accounts_index().as_deref(), s.active_account_id().as_deref()), (Some("[]"), Some("a1")));
        s.set_active_account_id(None);
        assert_eq!(s.active_account_id(), None);
    }

    #[test]
    fn launch_args_skip_empty_values() {
        assert!(claude_launch_args(false, "", "", "").is_empty());
        assert_eq!(
            claude_launch_args(true, "bypassPermissions", "opus", "high"),
            ["--allow-dangerously-skip-permissions", "--permission-mode", "bypassPermissions", "--model", "opus", "--effort", "high"]
        );
        assert_eq!(claude_launch_args(false, "", "", "low"), ["--effort", "low"]);
    }
}
