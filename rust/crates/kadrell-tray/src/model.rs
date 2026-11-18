//! Reine Aufbereitung ohne Plattform: Kurzstatus, Menümodell, Benachrichtigungstexte, Sound-Regeln.
//! Port von `StatusMenu.swift`, `Notifications.swift`, `Feedback.swift` und dem Badge aus `AttentionTracker.swift`.

use std::collections::{BTreeMap, BTreeSet};

/// Sprache der Texte (die Swift-App schaltet zur Laufzeit um).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Lang {
    #[default]
    De,
    En,
}

impl Lang {
    /// Die eingestellte Sprache der App (`kadrell-i18n`).
    pub fn current() -> Lang {
        Lang::from(kadrell_i18n::global().lang())
    }

    /// Text zum deutschen Quelltext (Schlüssel der xcstrings) in dieser Sprache, Werte der Reihe nach.
    fn tr(self, source: &str, args: &[kadrell_i18n::Arg]) -> String {
        kadrell_i18n::global().t_de_in(kadrell_i18n::Lang::from(self), source, args)
    }
}

impl From<kadrell_i18n::Lang> for Lang {
    fn from(lang: kadrell_i18n::Lang) -> Lang {
        match lang {
            kadrell_i18n::Lang::De => Lang::De,
            kadrell_i18n::Lang::En => Lang::En,
        }
    }
}

impl From<Lang> for kadrell_i18n::Lang {
    fn from(lang: Lang) -> kadrell_i18n::Lang {
        match lang {
            Lang::De => kadrell_i18n::Lang::De,
            Lang::En => kadrell_i18n::Lang::En,
        }
    }
}

/// Wie das Icon erscheint (Einstellung "Menüleiste"): Symbol und Text, nur Symbol, aus.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Display {
    #[default]
    Full,
    Icon,
    Hidden,
}

/// Was die Anzeige braucht: wartende und ungesehen fertige Sessions (zählen als "neu"), dazu die arbeitenden
/// (nur fürs Icon-Pünktchen und den Tooltip).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StatusSummary {
    pub waiting: usize,
    pub working: usize,
    /// Ungesehen fertige Sessions ("neu"), ohne wartende.
    pub done: usize,
    pub display: Display,
    pub lang: Lang,
}

/// "  3 warten · 1 neu", leer ohne beides (zwei führende Leerzeichen wie in Swift, der Text hängt am Symbol).
pub fn title(waiting: usize, done: usize, lang: Lang) -> String {
    if waiting == 0 && done == 0 {
        return String::new();
    }
    lang.tr("  %lld warten · %lld neu", &[waiting.into(), done.into()])
}

/// Text je nach Modus: nur `Full` zeigt den Kurztext, `Icon` und `Hidden` bleiben leer.
pub fn title_for(s: &StatusSummary) -> String {
    if s.display == Display::Full { title(s.waiting, s.done, s.lang) } else { String::new() }
}

/// Tooltip am Windows-Icon (Windows hat keinen Text neben dem Icon): Kurztext ohne Einrückung, sonst "Kadrell".
pub fn tooltip(s: &StatusSummary) -> String {
    let t = title(s.waiting, s.done, s.lang);
    let t = t.trim();
    let mut out = String::from("Kadrell");
    if !t.is_empty() {
        out.push_str(" · ");
        out.push_str(t);
    }
    if s.working > 0 {
        out.push_str(&format!(" · {} {}", s.working, s.lang.tr("arbeiten", &[])));
    }
    out
}

/// Farbe des Statuspunkts im Icon.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dot {
    None,
    Working,
    Waiting,
}

/// Wartend schlägt arbeitend (Gelb heißt: Claude wartet auf dich).
pub fn dot(s: &StatusSummary) -> Dot {
    if s.waiting > 0 {
        Dot::Waiting
    } else if s.working > 0 {
        Dot::Working
    } else {
        Dot::None
    }
}

/// Wartende zuerst, danach ungesehen fertige (nicht wartende), beide in Baumreihenfolge, keine Dopplungen.
pub fn rows(order: &[String], waiting: &BTreeSet<String>, unseen: &BTreeSet<String>) -> (Vec<String>, Vec<String>) {
    let w = order.iter().filter(|k| waiting.contains(*k)).cloned().collect();
    let d = order.iter().filter(|k| unseen.contains(*k) && !waiting.contains(*k)).cloned().collect();
    (w, d)
}

/// Symbol vor einem Menüeintrag (macOS: SF Symbols "clock" / "checkmark.circle").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemIcon {
    None,
    Waiting,
    Done,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TrayItem {
    Entry { id: String, label: String, icon: ItemIcon, enabled: bool },
    Separator,
}

/// Eine Session fürs Menü: Anzeige "Gruppe › Titel · 3 min".
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuSession {
    pub key: String,
    pub group: String,
    pub title: String,
    pub elapsed: String,
}

pub const ID_NEXT_WAITING: &str = "next-waiting";
pub const ID_OPEN: &str = "open";
pub const SESSION_PREFIX: &str = "session:";

/// Menü-Id einer Session-Zeile; `session_key_of` kehrt es um.
pub fn session_id(key: &str) -> String {
    format!("{SESSION_PREFIX}{key}")
}

pub fn session_key_of(id: &str) -> Option<&str> {
    id.strip_prefix(SESSION_PREFIX)
}

/// Das Menü wie in Swift: wartende, dann fertige Zeilen, Trenner, "Nächste wartende Session"; ohne beides
/// "Keine wartenden Sessions" (deaktiviert); am Ende "Kadrell öffnen".
pub fn build_menu(waiting: &[MenuSession], done: &[MenuSession], lang: Lang) -> Vec<TrayItem> {
    let mut items = Vec::new();
    let entry = |id: String, label: String, icon, enabled| TrayItem::Entry { id, label, icon, enabled };
    if waiting.is_empty() && done.is_empty() {
        let label = lang.tr("Keine wartenden Sessions", &[]);
        items.push(entry(String::new(), label, ItemIcon::None, false));
    } else {
        let row = |s: &MenuSession, icon| {
            entry(session_id(&s.key), format!("{} › {} · {}", s.group, s.title, s.elapsed), icon, true)
        };
        items.extend(waiting.iter().map(|s| row(s, ItemIcon::Waiting)));
        items.extend(done.iter().map(|s| row(s, ItemIcon::Done)));
        items.push(TrayItem::Separator);
        let next = lang.tr("Nächste wartende Session", &[]);
        items.push(entry(ID_NEXT_WAITING.into(), next, ItemIcon::None, !waiting.is_empty()));
    }
    items.push(TrayItem::Separator);
    items.push(entry(ID_OPEN.into(), lang.tr("Kadrell öffnen", &[]), ItemIcon::None, true));
    items
}

// ---------------------------------------------------------------------------------------------------------------
// Benachrichtigungen

/// Einstellung "Systembenachrichtigungen".
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum NotifyLevel {
    Off,
    #[default]
    Waiting,
    All,
}

/// Bei welchem Level welches Ereignis eine Benachrichtigung auslöst.
pub fn should_notify(level: NotifyLevel, waiting: bool) -> bool {
    level != NotifyLevel::Off && (waiting || level == NotifyLevel::All)
}

/// Eine Systembenachrichtigung. Gleicher `session_key` ersetzt die vorige statt sich zu stapeln, `group` gruppiert.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notification {
    pub title: String,
    pub body: String,
    pub session_key: String,
    pub group: String,
}

/// Titel "Gruppe › Titel", Text die Meldung der Session oder "wartet auf dich" / "ist fertig".
pub fn notification(
    session_key: &str,
    group: &str,
    title: &str,
    message: Option<&str>,
    waiting: bool,
    lang: Lang,
) -> Notification {
    let fallback = if waiting { lang.tr("wartet auf dich", &[]) } else { lang.tr("ist fertig", &[]) };
    Notification {
        title: format!("{group} › {title}"),
        body: message.filter(|m| !m.is_empty()).map(str::to_string).unwrap_or(fallback),
        session_key: session_key.to_string(),
        group: group.to_string(),
    }
}

/// Dock-Badge / Taskleisten-Overlay: wartende plus ungesehene (fertige zählen mit, sonst verschwänden sie).
pub fn badge_count(waiting: &BTreeSet<String>, unseen: &BTreeSet<String>) -> usize {
    waiting.union(unseen).count()
}

// ---------------------------------------------------------------------------------------------------------------
// Sounds

/// Einstellung "Sounds".
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SoundLevel {
    Off,
    #[default]
    Important,
    All,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sound {
    Waiting,
    Done,
    Open,
    Close,
    Toggle,
}

impl Sound {
    /// macOS-Systemklang wie in Swift.
    pub fn mac_name(self) -> &'static str {
        match self {
            Sound::Waiting => "Tink",
            Sound::Done => "Glass",
            Sound::Open => "Pop",
            Sound::Close => "Bottle",
            Sound::Toggle => "Morse",
        }
    }

    /// Zählen auch bei "nur wichtig".
    pub fn important(self) -> bool {
        matches!(self, Sound::Waiting | Sound::Done)
    }
}

pub fn should_play(level: SoundLevel, sound: Sound) -> bool {
    match level {
        SoundLevel::Off => false,
        SoundLevel::Important => sound.important(),
        SoundLevel::All => true,
    }
}

/// Sessionstatus für `transitions`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Running,
    Waiting,
    Idle,
}

/// Sessions, die seit dem letzten Stand zu warten begonnen haben bzw. vom Arbeiten in den Leerlauf gewechselt sind.
pub fn transitions(old: &BTreeMap<String, Status>, new: &BTreeMap<String, Status>) -> (BTreeSet<String>, BTreeSet<String>) {
    let (mut waiting, mut done) = (BTreeSet::new(), BTreeSet::new());
    for (id, s) in new {
        let Some(o) = old.get(id) else { continue };
        if o == s {
            continue;
        }
        if *s == Status::Waiting {
            waiting.insert(id.clone());
        }
        if *o == Status::Running && *s == Status::Idle {
            done.insert(id.clone());
        }
    }
    (waiting, done)
}

/// Fortschritt einer Animation 0..1, zum Ende abbremsend; None = vorbei (sofort bei "Bewegung reduzieren").
pub fn progress(elapsed_secs: f64, duration_secs: f64, reduce_motion: bool) -> Option<f64> {
    if reduce_motion {
        return None;
    }
    let p = elapsed_secs / duration_secs;
    (0.0..1.0).contains(&p).then(|| 1.0 - (1.0 - p).powi(3))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(v: &[&str]) -> BTreeSet<String> {
        v.iter().map(|s| s.to_string()).collect()
    }
    fn strs(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }
    fn ms(key: &str) -> MenuSession {
        MenuSession { key: key.into(), group: "G".into(), title: "T".into(), elapsed: "3 min".into() }
    }

    #[test]
    fn title_formats_like_swift() {
        assert_eq!(title(0, 0, Lang::De), "");
        assert_eq!(title(3, 1, Lang::De), "  3 warten · 1 neu");
        assert_eq!(title(0, 2, Lang::En), "  0 waiting · 2 new");
    }

    #[test]
    fn title_depends_on_display() {
        let mut s = StatusSummary { waiting: 3, done: 1, ..Default::default() };
        assert_eq!(title_for(&s), "  3 warten · 1 neu");
        s.display = Display::Icon;
        assert_eq!(title_for(&s), "");
        s.display = Display::Hidden;
        assert_eq!(title_for(&s), "");
    }

    #[test]
    fn tooltip_and_dot() {
        let s = StatusSummary { waiting: 2, working: 1, done: 0, ..Default::default() };
        assert_eq!(tooltip(&s), "Kadrell · 2 warten · 0 neu · 1 arbeiten");
        assert_eq!(dot(&s), Dot::Waiting);
        assert_eq!(dot(&StatusSummary { working: 1, ..Default::default() }), Dot::Working);
        assert_eq!(dot(&StatusSummary::default()), Dot::None);
        assert_eq!(tooltip(&StatusSummary::default()), "Kadrell");
    }

    #[test]
    fn rows_waiting_first_no_duplicates() {
        let order = strs(&["a", "b", "c", "d"]);
        let (w, d) = rows(&order, &set(&["c", "a"]), &set(&["b", "c", "x"]));
        assert_eq!(w, ["a", "c"]);
        assert_eq!(d, ["b"]);
    }

    #[test]
    fn menu_empty_has_disabled_placeholder_and_open() {
        let m = build_menu(&[], &[], Lang::En);
        assert_eq!(m.len(), 3);
        assert!(matches!(&m[0], TrayItem::Entry { label, enabled: false, .. } if label == "No waiting sessions"));
        assert_eq!(m[1], TrayItem::Separator);
        assert!(matches!(&m[2], TrayItem::Entry { id, .. } if id == ID_OPEN));
    }

    #[test]
    fn menu_lists_rows_then_next_waiting() {
        let m = build_menu(&[ms("a")], &[ms("b")], Lang::De);
        assert!(matches!(&m[0], TrayItem::Entry { id, label, icon: ItemIcon::Waiting, .. }
            if id == "session:a" && label == "G › T · 3 min"));
        assert!(matches!(&m[1], TrayItem::Entry { icon: ItemIcon::Done, .. }));
        assert!(matches!(&m[3], TrayItem::Entry { id, enabled: true, .. } if id == ID_NEXT_WAITING));
        let only_done = build_menu(&[], &[ms("b")], Lang::De);
        assert!(matches!(&only_done[2], TrayItem::Entry { id, enabled: false, .. } if id == ID_NEXT_WAITING));
        assert_eq!(session_key_of("session:a"), Some("a"));
        assert_eq!(session_key_of("open"), None);
    }

    #[test]
    fn notify_levels() {
        assert!(!should_notify(NotifyLevel::Off, true));
        assert!(should_notify(NotifyLevel::Waiting, true));
        assert!(!should_notify(NotifyLevel::Waiting, false));
        assert!(should_notify(NotifyLevel::All, false));
    }

    #[test]
    fn notification_texts() {
        let n = notification("k", "Proj", "Fix", None, true, Lang::De);
        assert_eq!((n.title.as_str(), n.body.as_str()), ("Proj › Fix", "wartet auf dich"));
        assert_eq!(notification("k", "P", "F", Some(""), false, Lang::En).body, "is done");
        assert_eq!(notification("k", "P", "F", Some("Fertig mit X"), false, Lang::De).body, "Fertig mit X");
    }

    #[test]
    fn badge_counts_union() {
        assert_eq!(badge_count(&set(&["a", "b"]), &set(&["b", "c"])), 3);
        assert_eq!(badge_count(&set(&[]), &set(&[])), 0);
    }

    #[test]
    fn sound_rules() {
        assert!(!should_play(SoundLevel::Off, Sound::Waiting));
        assert!(should_play(SoundLevel::Important, Sound::Done));
        assert!(!should_play(SoundLevel::Important, Sound::Open));
        assert!(should_play(SoundLevel::All, Sound::Toggle));
        assert_eq!(Sound::Waiting.mac_name(), "Tink");
    }

    #[test]
    fn transitions_like_swift() {
        let old = BTreeMap::from([
            ("a".to_string(), Status::Running),
            ("b".into(), Status::Running),
            ("c".into(), Status::Idle),
        ]);
        let new = BTreeMap::from([
            ("a".to_string(), Status::Waiting),
            ("b".into(), Status::Idle),
            ("c".into(), Status::Idle),
            ("n".into(), Status::Waiting),
        ]);
        let (w, d) = transitions(&old, &new);
        assert_eq!(w, set(&["a"]));
        assert_eq!(d, set(&["b"]));
    }

    #[test]
    fn progress_eases_and_respects_reduce_motion() {
        assert_eq!(progress(0.0, 1.0, true), None);
        assert_eq!(progress(2.0, 1.0, false), None);
        assert_eq!(progress(0.0, 1.0, false), Some(0.0));
        assert!((progress(0.5, 1.0, false).unwrap() - 0.875).abs() < 1e-9);
    }
}
