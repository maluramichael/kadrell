//! Zeitangabe rechts in der Zeile des Schnellstarts wie `Text(date, format: .relative(presentation: .named))` in
//! `QuickStartView.swift`: „gestern“, „vor 3 Tagen“, „vor 1 Stunde“ (ausgeschrieben, nicht „vor 3 Tg.“ wie im Verlauf).
//! Nicht verifiziert, Annahme: Foundation rechnet „gestern“ nach Kalendertagen, hier gelten 24-Stunden-Schritte; die
//! Wörter für Woche, Monat und Jahr sind die Grundformen von `.named` ohne Sonderfälle wie „letzte Woche“.

use kadrell_i18n::trd;

use crate::lang::de;

struct Unit {
    size: u64,
    one: &'static str,
    many: &'static str,
}

const DAY: u64 = 86_400;

const UNITS: [Unit; 6] = [
    Unit { size: 31_536_000, one: de!("vor 1 Jahr"), many: de!("vor %lld Jahren") },
    Unit { size: 2_592_000, one: de!("vor 1 Monat"), many: de!("vor %lld Monaten") },
    Unit { size: 604_800, one: de!("vor 1 Woche"), many: de!("vor %lld Wochen") },
    Unit { size: DAY, one: de!("gestern"), many: de!("vor %lld Tagen") },
    Unit { size: 3_600, one: de!("vor 1 Stunde"), many: de!("vor %lld Stunden") },
    Unit { size: 60, one: de!("vor 1 Minute"), many: de!("vor %lld Minuten") },
];

/// Wie lange her, in der eingestellten Sprache.
pub fn named_relative(secs_ago: u64) -> String {
    match UNITS.iter().find(|u| secs_ago >= u.size) {
        Some(u) => phrase(u, secs_ago / u.size),
        None => trd!("jetzt"),
    }
}

fn phrase(unit: &Unit, n: u64) -> String {
    match (unit.size, n) {
        (DAY, 2) => trd!("vorgestern"),
        (_, 1) => trd!(unit.one),
        _ => trd!(unit.many, n),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_phrases_in_german() {
        kadrell_i18n::set_thread_language(kadrell_i18n::Language::De);
        assert_eq!(named_relative(5), "jetzt");
        assert_eq!(named_relative(60), "vor 1 Minute");
        assert_eq!(named_relative(5 * 60 + 3), "vor 5 Minuten");
        assert_eq!(named_relative(3_600), "vor 1 Stunde");
        assert_eq!(named_relative(7 * 3_600), "vor 7 Stunden");
        assert_eq!(named_relative(DAY), "gestern");
        assert_eq!(named_relative(2 * DAY + 5), "vorgestern");
        assert_eq!(named_relative(3 * DAY), "vor 3 Tagen");
        assert_eq!(named_relative(7 * DAY), "vor 1 Woche");
        assert_eq!(named_relative(21 * DAY), "vor 3 Wochen");
        assert_eq!(named_relative(40 * DAY), "vor 1 Monat");
        assert_eq!(named_relative(400 * DAY), "vor 1 Jahr");
        assert_eq!(named_relative(800 * DAY), "vor 2 Jahren");
    }

    #[test]
    fn named_phrases_in_english() {
        kadrell_i18n::set_thread_language(kadrell_i18n::Language::En);
        assert_eq!(named_relative(5), "now");
        assert_eq!(named_relative(DAY), "yesterday");
        assert_eq!(named_relative(2 * DAY), "2 days ago");
        assert_eq!(named_relative(3 * DAY), "3 days ago");
        assert_eq!(named_relative(60), "1 minute ago");
        assert_eq!(named_relative(2 * 3_600), "2 hours ago");
        kadrell_i18n::set_thread_language(kadrell_i18n::Language::De);
    }
}
