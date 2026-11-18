//! Die Regeln hinter Tray, Benachrichtigung, Klang und Badge, rein und ohne Fenster testbar. Was wann ausgelöst
//! wird, entscheidet der Aufmerksamkeits-Tracker (`kadrell_claude::attention`, Port von `AttentionTracker.swift`):
//! er kennt „Fenster vorn“, „sichtbar“ und die Marke „neu“. Hier kommen nur noch die Einstellungen dazu
//! („Sounds“, „Systembenachrichtigungen“, „Menüleiste“) und die Abbildung auf die Tray-Typen.

use kadrell_claude::attention::{Effect, Sound as TrackerSound};
use kadrell_core::settings::{NotificationLevel, SoundLevel, StatusBarDisplay};
use kadrell_tray::model::{self, Display, Lang, NotifyLevel, Sound};

/// Was die Plattformschicht tun soll.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// Klang spielen (die Stufe der Einstellung ist schon geprüft).
    Play(Sound),
    /// Systembenachrichtigung für diese Session; `waiting` falsch heißt fertig.
    Notify { key: String, waiting: bool },
    /// Fenster in der Taskleiste bzw. im Dock auf sich aufmerksam machen lassen.
    RequestAttention,
}

pub fn sound_level(level: SoundLevel) -> model::SoundLevel {
    match level {
        SoundLevel::Off => model::SoundLevel::Off,
        SoundLevel::Important => model::SoundLevel::Important,
        SoundLevel::All => model::SoundLevel::All,
    }
}

pub fn notify_level(level: NotificationLevel) -> NotifyLevel {
    match level {
        NotificationLevel::Off => NotifyLevel::Off,
        NotificationLevel::Waiting => NotifyLevel::Waiting,
        NotificationLevel::All => NotifyLevel::All,
    }
}

pub fn display(display: StatusBarDisplay) -> Display {
    match display {
        StatusBarDisplay::Full => Display::Full,
        StatusBarDisplay::Icon => Display::Icon,
        StatusBarDisplay::Hidden => Display::Hidden,
    }
}

/// Sprache der Tray-Texte aus der der Oberfläche.
pub fn lang(lang: kadrell_i18n::Lang) -> Lang {
    match lang {
        kadrell_i18n::Lang::De => Lang::De,
        kadrell_i18n::Lang::En => Lang::En,
    }
}

/// Die Wirkungen des Trackers, durch die Einstellungen gefiltert. Badge, Ansage, Tipp und Fokus-Hook gehören nicht
/// hierher: das Badge folgt dem Zustand (`view`), die anderen führt die Oberfläche bzw. der Zustand selbst aus.
pub fn plan(effects: &[Effect], sounds: SoundLevel, notifications: NotificationLevel) -> Vec<Action> {
    let (sounds, notifications) = (sound_level(sounds), notify_level(notifications));
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Play(sound) => {
                let sound = sound_of(*sound);
                model::should_play(sounds, sound).then_some(Action::Play(sound))
            }
            Effect::Notify { key, waiting } => {
                model::should_notify(notifications, *waiting).then(|| Action::Notify { key: key.clone(), waiting: *waiting })
            }
            Effect::RequestAttention => Some(Action::RequestAttention),
            _ => None,
        })
        .collect()
}

fn sound_of(sound: TrackerSound) -> Sound {
    match sound {
        TrackerSound::Waiting => Sound::Waiting,
        TrackerSound::Done => Sound::Done,
    }
}

/// Die Session, zu der „Nächste wartende Session“ springt: die nach der fokussierten in Baumreihenfolge
/// (zyklisch), ohne Fokus oder bei nicht wartendem Fokus die erste wartende (`focusNextWaiting`).
pub fn next_waiting<'a>(waiting: &'a [String], focused: Option<&str>) -> Option<&'a String> {
    let at = focused.and_then(|f| waiting.iter().position(|k| k == f));
    match at {
        Some(i) => waiting.get((i + 1) % waiting.len()),
        None => waiting.first(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn notify(key: &str, waiting: bool) -> Effect {
        Effect::Notify { key: key.into(), waiting }
    }

    #[test]
    fn notifications_follow_the_setting() {
        let effects = [notify("a", true), notify("b", false)];
        let all = plan(&effects, SoundLevel::All, NotificationLevel::All);
        assert_eq!(all, [Action::Notify { key: "a".into(), waiting: true }, Action::Notify { key: "b".into(), waiting: false }]);
        let waiting = plan(&effects, SoundLevel::All, NotificationLevel::Waiting);
        assert_eq!(waiting, [Action::Notify { key: "a".into(), waiting: true }], "fertig nur bei „wartet und fertig“");
        assert!(plan(&effects, SoundLevel::All, NotificationLevel::Off).is_empty());
    }

    #[test]
    fn sounds_follow_the_setting() {
        let effects = [Effect::Play(TrackerSound::Waiting), Effect::Play(TrackerSound::Done)];
        assert_eq!(plan(&effects, SoundLevel::Off, NotificationLevel::Off), []);
        assert_eq!(plan(&effects, SoundLevel::Important, NotificationLevel::Off), [Action::Play(Sound::Waiting), Action::Play(Sound::Done)]);
        assert_eq!(plan(&effects[1..], SoundLevel::All, NotificationLevel::Off), [Action::Play(Sound::Done)]);
    }

    #[test]
    fn only_platform_effects_are_planned() {
        let effects = [
            Effect::SetBadge(3),
            Effect::Announce { key: "a".into(), waiting: true },
            Effect::Tip(None),
            Effect::FireFocusHook("a".into()),
            Effect::RequestAttention,
        ];
        assert_eq!(plan(&effects, SoundLevel::All, NotificationLevel::All), [Action::RequestAttention]);
    }

    #[test]
    fn settings_map_onto_tray_types() {
        assert_eq!(display(StatusBarDisplay::Hidden), Display::Hidden);
        assert_eq!(display(StatusBarDisplay::Icon), Display::Icon);
        assert_eq!(notify_level(NotificationLevel::Waiting), NotifyLevel::Waiting);
        assert_eq!(sound_level(SoundLevel::Important), model::SoundLevel::Important);
        assert_eq!(lang(kadrell_i18n::Lang::En), Lang::En);
    }

    #[test]
    fn next_waiting_cycles_through_the_waiting_sessions() {
        let waiting: Vec<String> = ["a", "b", "c"].iter().map(|s| s.to_string()).collect();
        assert_eq!(next_waiting(&waiting, Some("a")).map(String::as_str), Some("b"));
        assert_eq!(next_waiting(&waiting, Some("c")).map(String::as_str), Some("a"));
        assert_eq!(next_waiting(&waiting, Some("x")).map(String::as_str), Some("a"));
        assert_eq!(next_waiting(&waiting, None).map(String::as_str), Some("a"));
        assert_eq!(next_waiting(&[], Some("a")), None);
    }
}
