//! Die wenigen Texte, die dieses Crate selbst baut (Anzeigename, Nutzungszeile, Wochentag). Sie kommen aus
//! `kadrell-i18n` (deutscher Quelltext = Schlüssel der xcstrings bzw. der Zusatztexte). Die Sprache ist ein Parameter,
//! damit Aufrufer und Tests unabhängig von der eingestellten Sprache der App bleiben.

use kadrell_i18n::Arg;

/// Oberflächensprache (Einstellung `language`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Lang {
    #[default]
    De,
    En,
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

impl Lang {
    /// Die eingestellte Sprache der App.
    pub fn current() -> Lang {
        Lang::from(kadrell_i18n::global().lang())
    }

    fn tr(self, source: &str, args: &[Arg]) -> String {
        kadrell_i18n::global().t_de_in(self.into(), source, args)
    }

    pub(crate) fn account(self) -> String {
        self.tr("Konto", &[])
    }

    pub(crate) fn current_account(self) -> String {
        self.tr("Aktueller Account", &[])
    }

    pub(crate) fn free(self) -> String {
        self.tr("frei", &[])
    }

    pub(crate) fn until(self, clock: &str) -> String {
        self.tr("bis %@", &[Arg::from(clock)])
    }

    pub(crate) fn just_now(self) -> String {
        self.tr("gerade eben", &[])
    }

    pub(crate) fn minutes_ago(self, mins: i64) -> String {
        self.tr("vor %lld min", &[Arg::from(mins)])
    }

    /// Kurzer Wochentag, 0 = Montag. Deutsch mit Punkt wie das Datumsformat der Swift-App („Do.“).
    pub(crate) fn weekday(self, monday_based: u32) -> String {
        const DE: [&str; 7] = ["Mo.", "Di.", "Mi.", "Do.", "Fr.", "Sa.", "So."];
        self.tr(DE[(monday_based % 7) as usize], &[])
    }

    /// Trenner zwischen Wochentag und Uhrzeit: „Do., 14:00“ bzw. „Thu 14:00“.
    pub(crate) fn weekday_clock(self, monday_based: u32, clock: &str) -> String {
        match self {
            Lang::De => format!("{}, {clock}", self.weekday(monday_based)),
            Lang::En => format!("{} {clock}", self.weekday(monday_based)),
        }
    }
}
