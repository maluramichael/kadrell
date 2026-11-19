//! Kadrell-Texte in Deutsch und Englisch, umschaltbar zur Laufzeit (kein Neustart: der würde alle
//! Claude-Prozesse mitnehmen).
//!
//! Quelle der Wahrheit sind die xcstrings der Mac-App. `rust/tools/xcstrings2ftl.py` erzeugt daraus
//! `locales/{de,en}/kadrell.ftl` und `src/catalog.rs`; beides ist eingecheckt.
//!
//! ```
//! use kadrell_i18n::{I18n, Language};
//! let i18n = I18n::new(Language::En);
//! assert_eq!(i18n.t_de("Kopieren"), "Copy");
//! i18n.set_language(Language::De);
//! assert_eq!(i18n.t_de("Kopieren"), "Kopieren");
//! ```
//!
//! Die Nachrichten-Ids (Schema im Kopf des Konverters) und die Variablen `arg1`, `arg2`, ... (n-ter
//! Platzhalter `%@`/`%lld` des deutschen Schlüssels) stehen in den `.ftl`-Dateien. Portierter Code kann
//! mit den deutschen Quelltexten starten (`t_de`, `trd!`) und später auf Ids (`t`, `tr!`) wechseln.

mod catalog;
mod extra;
mod system;
mod winwords;

use fluent_bundle::concurrent::FluentBundle;
use fluent_bundle::{FluentArgs, FluentResource, FluentValue};
use std::borrow::Cow;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

pub use system::system_language;
pub use winwords::adapt as windows_words;

const DE_FTL: &str = include_str!("../locales/de/kadrell.ftl");
const EN_FTL: &str = include_str!("../locales/en/kadrell.ftl");
/// Texte, die es nur in der Rust-App gibt (kein Eintrag in den xcstrings). Wird nach `kadrell.ftl` geladen.
const DE_EXTRA_FTL: &str = include_str!("../locales/de/extra.ftl");
const EN_EXTRA_FTL: &str = include_str!("../locales/en/extra.ftl");

/// Eine übersetzte Sprache.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Lang {
    De,
    En,
}

impl Lang {
    pub const ALL: [Lang; 2] = [Lang::De, Lang::En];

    pub fn code(self) -> &'static str {
        match self {
            Lang::De => "de",
            Lang::En => "en",
        }
    }

    /// `de`, `de-DE`, `de_DE.UTF-8`, `EN` ... Nicht unterstützte Sprachen geben `None`.
    pub fn from_tag(tag: &str) -> Option<Lang> {
        let primary = tag.split(['-', '_', '.', '@']).next()?.to_ascii_lowercase();
        match primary.as_str() {
            "de" => Some(Lang::De),
            "en" => Some(Lang::En),
            _ => None,
        }
    }

    fn as_u8(self) -> u8 {
        self as u8
    }

    fn from_u8(v: u8) -> Lang {
        if v == Lang::En as u8 { Lang::En } else { Lang::De }
    }
}

/// Die Einstellung der Nutzerin, wie `Localization.Language` in der Swift-App.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum Language {
    /// Sprache des Systems (erste bevorzugte Sprache aus de/en, sonst Deutsch).
    #[default]
    System,
    De,
    En,
}

impl Language {
    pub const ALL: [Language; 3] = [Language::System, Language::De, Language::En];

    /// Wert für die Einstellungsdatei: `system`, `de`, `en` (wie in der Swift-App).
    pub fn as_str(self) -> &'static str {
        match self {
            Language::System => "system",
            Language::De => "de",
            Language::En => "en",
        }
    }

    pub fn parse(s: &str) -> Option<Language> {
        match s {
            "system" => Some(Language::System),
            "de" => Some(Language::De),
            "en" => Some(Language::En),
            _ => None,
        }
    }

    /// Die tatsächlich verwendete Sprache.
    pub fn resolve(self) -> Lang {
        match self {
            Language::System => system_language(),
            Language::De => Lang::De,
            Language::En => Lang::En,
        }
    }

    fn as_u8(self) -> u8 {
        self as u8
    }

    fn from_u8(v: u8) -> Language {
        match v {
            1 => Language::De,
            2 => Language::En,
            _ => Language::System,
        }
    }
}

/// Wert eines Formatplatzhalters (`%@` ist Text, `%lld` eine Zahl).
#[derive(Clone, Debug, PartialEq)]
pub enum Arg<'a> {
    Str(Cow<'a, str>),
    Int(i64),
    Float(f64),
}

macro_rules! arg_from_int {
    ($($t:ty),*) => {$(impl From<$t> for Arg<'_> { fn from(v: $t) -> Self { Arg::Int(v as i64) } })*};
}
arg_from_int!(i8, i16, i32, i64, isize, u8, u16, u32, u64, usize);

impl From<f64> for Arg<'_> {
    fn from(v: f64) -> Self {
        Arg::Float(v)
    }
}
impl<'a> From<&'a str> for Arg<'a> {
    fn from(v: &'a str) -> Self {
        Arg::Str(Cow::Borrowed(v))
    }
}
impl<'a> From<&'a String> for Arg<'a> {
    fn from(v: &'a String) -> Self {
        Arg::Str(Cow::Borrowed(v.as_str()))
    }
}
impl From<String> for Arg<'_> {
    fn from(v: String) -> Self {
        Arg::Str(Cow::Owned(v))
    }
}

impl<'a> Arg<'a> {
    fn to_fluent(&self) -> FluentValue<'_> {
        match self {
            Arg::Str(s) => FluentValue::String(Cow::Borrowed(s.as_ref())),
            Arg::Int(n) => FluentValue::from(*n),
            Arg::Float(f) => FluentValue::from(*f),
        }
    }
}

type Bundle = FluentBundle<FluentResource>;

fn build_bundle(lang: Lang, source: &'static str) -> Bundle {
    let id = lang.code().parse().expect("gültiger Sprachcode");
    let mut bundle = FluentBundle::new_concurrent(vec![id]);
    bundle.set_use_isolating(false); // keine Unicode-Isolierzeichen um Variablen
    for text in [source, extra_source(lang)] {
        let resource = match FluentResource::try_new(text.to_string()) {
            Ok(r) => r,
            Err((r, _)) => r, // fehlerhafte Einträge fehlen, der Rest bleibt nutzbar (Tests melden den Fehler)
        };
        let _ = bundle.add_resource(resource); // doppelte Ids: der erste Eintrag gilt, Tests melden es
    }
    bundle
}

fn extra_source(lang: Lang) -> &'static str {
    match lang {
        Lang::De => DE_EXTRA_FTL,
        Lang::En => EN_EXTRA_FTL,
    }
}

thread_local! {
    /// Sprache nur für diesen Thread, siehe `set_thread_language`.
    static THREAD_LANG: std::cell::Cell<Option<Lang>> = const { std::cell::Cell::new(None) };
}

/// Legt die Sprache fest, in der Aufrufe dieses Threads übersetzen, ohne die globale Einstellung anzufassen. Gedacht
/// für Tests: Die Testfäden eines Prozesses laufen parallel, und ein globales `set_language` des einen Tests kippt
/// sonst die Texte des anderen (zufällige Fehler je nach Reihenfolge).
pub fn set_thread_language(language: Language) {
    THREAD_LANG.with(|l| l.set(Some(language.resolve())));
}

/// Übersetzer. Thread-sicher; `set_language` wirkt sofort für alle folgenden Aufrufe.
pub struct I18n {
    de: Bundle,
    en: Bundle,
    setting: AtomicU8,
    resolved: AtomicU8,
    generation: AtomicU64,
    /// Windows-Begriffe statt Mac-Begriffe (Explorer statt Finder), Standard: nicht auf dem Mac.
    windows_words: AtomicBool,
    reported: Mutex<HashSet<String>>,
}

impl I18n {
    pub fn new(language: Language) -> I18n {
        let i18n = I18n {
            de: build_bundle(Lang::De, DE_FTL),
            en: build_bundle(Lang::En, EN_FTL),
            setting: AtomicU8::new(0),
            resolved: AtomicU8::new(0),
            generation: AtomicU64::new(0),
            windows_words: AtomicBool::new(!cfg!(target_os = "macos")),
            reported: Mutex::new(HashSet::new()),
        };
        i18n.apply(language);
        i18n
    }

    fn apply(&self, language: Language) {
        self.setting.store(language.as_u8(), Ordering::SeqCst);
        self.resolved.store(language.resolve().as_u8(), Ordering::SeqCst);
        self.generation.fetch_add(1, Ordering::SeqCst);
    }

    /// Schaltet live um. Danach liefern alle Aufrufe die neue Sprache; die Oberfläche muss sich
    /// neu zeichnen und alles neu bauen, was Texte beim Erzeugen zwischenspeichert.
    pub fn set_language(&self, language: Language) {
        self.apply(language);
    }

    /// Schaltet die Windows-Begriffe ein oder aus (für Tests, die den Mac-Text prüfen).
    pub fn set_windows_words(&self, on: bool) {
        self.windows_words.store(on, Ordering::SeqCst);
    }

    /// Liest die Systemsprache neu ein (nur bei `Language::System` nötig, z. B. beim Zurückkehren in die App).
    pub fn refresh_system_language(&self) {
        self.apply(self.language());
    }

    /// Die Einstellung (`System`, `De`, `En`).
    pub fn language(&self) -> Language {
        Language::from_u8(self.setting.load(Ordering::SeqCst))
    }

    /// Die tatsächlich verwendete Sprache.
    pub fn lang(&self) -> Lang {
        if let Some(lang) = THREAD_LANG.with(std::cell::Cell::get) {
            return lang;
        }
        Lang::from_u8(self.resolved.load(Ordering::SeqCst))
    }

    /// Zähler, der bei jedem Umschalten steigt. Die Oberfläche vergleicht ihn, um zu sehen, ob sie neu bauen muss.
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::SeqCst)
    }

    fn bundle(&self, lang: Lang) -> &Bundle {
        match lang {
            Lang::De => &self.de,
            Lang::En => &self.en,
        }
    }

    /// Text zu einer Nachrichten-Id (ohne Variablen).
    pub fn t(&self, id: &str) -> String {
        self.t_args(id, &[])
    }

    /// Text mit benannten Variablen (`arg1`, `arg2`, ...).
    pub fn t_args(&self, id: &str, args: &[(&str, Arg)]) -> String {
        self.t_args_in(self.lang(), id, args)
    }

    fn t_args_in(&self, lang: Lang, id: &str, args: &[(&str, Arg)]) -> String {
        let fluent_args = build_args(args);
        if let Some(text) = self.format(lang, id, &fluent_args) {
            return text;
        }
        self.report_missing(lang, id);
        if lang != Lang::De
            && let Some(text) = self.format(Lang::De, id, &fluent_args)
        {
            return text;
        }
        id.to_string()
    }

    /// Text zum deutschen Quelltext (Schlüssel der xcstrings). Unbekannte Texte kommen unverändert zurück.
    pub fn t_de(&self, source: &str) -> String {
        self.t_de_args(source, &[])
    }

    /// Wie `t_de`, die Werte belegen der Reihe nach `arg1`, `arg2`, ...
    pub fn t_de_args(&self, source: &str, args: &[Arg]) -> String {
        self.t_de_in(self.lang(), source, args)
    }

    /// Wie `t_de_args`, aber in einer bestimmten Sprache statt der eingestellten (Crates, die ihre Sprache
    /// als Parameter bekommen, und deterministische Tests).
    pub fn t_de_in(&self, lang: Lang, source: &str, args: &[Arg]) -> String {
        let Some(id) = id_for_de(source) else {
            self.report_missing(Lang::De, source);
            return source.to_string();
        };
        let named: Vec<(String, &Arg)> = args.iter().enumerate().map(|(i, a)| (format!("arg{}", i + 1), a)).collect();
        let args: Vec<(&str, Arg)> = named.iter().map(|(n, a)| (n.as_str(), (*a).clone())).collect();
        self.t_args_in(lang, id, &args)
    }

    /// Ob die Id in beiden Sprachen vorhanden ist (für Tests und Werkzeuge).
    pub fn has(&self, lang: Lang, id: &str) -> bool {
        self.bundle(lang).has_message(id)
    }

    fn format(&self, lang: Lang, id: &str, args: &FluentArgs) -> Option<String> {
        let bundle = self.bundle(lang);
        let pattern = bundle.get_message(id)?.value()?;
        let mut errors = Vec::new();
        let text = bundle.format_pattern(pattern, Some(args), &mut errors).into_owned();
        Some(if self.windows_words.load(Ordering::SeqCst) { winwords::adapt(&text, lang) } else { text })
    }

    /// Fehlende Übersetzung: in Debug-Builds einmal je Id auf stderr, nie ein Panic.
    fn report_missing(&self, lang: Lang, id: &str) {
        let key = format!("{}:{id}", lang.code());
        let first = self.reported.lock().map(|mut set| set.insert(key)).unwrap_or(false);
        if first && cfg!(debug_assertions) {
            eprintln!("kadrell-i18n: fehlende Übersetzung ({}): {id}", lang.code());
        }
    }
}

fn build_args<'a>(args: &'a [(&'a str, Arg<'a>)]) -> FluentArgs<'a> {
    let mut out = FluentArgs::with_capacity(args.len());
    for (name, value) in args {
        out.set(*name, value.to_fluent());
    }
    out
}

/// Nachrichten-Id zum deutschen Quelltext (erst die xcstrings, dann die Zusatztexte der Rust-App).
pub fn id_for_de(source: &str) -> Option<&'static str> {
    let find = |keys: &'static [(&'static str, &'static str)]| {
        keys.binary_search_by(|(k, _)| (*k).cmp(source)).ok().map(|i| keys[i].1)
    };
    find(catalog::KEYS).or_else(|| find(extra::KEYS))
}

/// Zusatztexte der Rust-App (deutscher Quelltext, Id), sortiert nach Quelltext. Nicht in den xcstrings.
pub fn extra_catalog() -> &'static [(&'static str, &'static str)] {
    extra::KEYS
}

/// Die `.ftl`-Quelle der Zusatztexte einer Sprache (für Tests und Werkzeuge).
pub fn extra_ftl_source(lang: Lang) -> &'static str {
    extra_source(lang)
}

/// Alle (deutscher Quelltext, Id) in der Reihenfolge der Quelle.
pub fn catalog() -> &'static [(&'static str, &'static str)] {
    catalog::KEYS
}

/// Die `.ftl`-Quelle einer Sprache (für Tests und Werkzeuge).
pub fn ftl_source(lang: Lang) -> &'static str {
    match lang {
        Lang::De => DE_FTL,
        Lang::En => EN_FTL,
    }
}

/// Gemeinsame Instanz der App, beim ersten Zugriff mit der Systemsprache.
///
/// Mit dem Feature `pin-german` (nur für Tests anderer Crates) startet sie auf Deutsch, damit Tests, die deutsche
/// Texte prüfen, nicht von der Sprache des Rechners abhängen.
pub fn global() -> &'static I18n {
    static GLOBAL: OnceLock<I18n> = OnceLock::new();
    GLOBAL.get_or_init(|| I18n::new(if cfg!(feature = "pin-german") { Language::De } else { Language::System }))
}

/// `tr!("id")` oder `tr!("id", arg1 = x, arg2 = "text")` über die gemeinsame Instanz.
#[macro_export]
macro_rules! tr {
    ($id:expr) => { $crate::global().t($id) };
    ($id:expr, $($name:ident = $value:expr),+ $(,)?) => {
        $crate::global().t_args($id, &[$((stringify!($name), $crate::Arg::from($value))),+])
    };
}

/// `trd!("Neue Session")` oder `trd!("%lld Sessions", 3)`: deutscher Quelltext, Werte der Reihe nach.
#[macro_export]
macro_rules! trd {
    ($src:expr) => { $crate::global().t_de($src) };
    ($src:expr, $($value:expr),+ $(,)?) => {
        $crate::global().t_de_args($src, &[$($crate::Arg::from($value)),+])
    };
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod extra_tests;
