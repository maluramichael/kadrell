//! Kleine Sprachauswahl für die wenigen Texte, die dieses Crate selbst baut (Anzeigename, Nutzungszeile,
//! Wochentag). `kadrell-core` bringt später die Fluent-Texte; bis dahin genügt Deutsch und Englisch hier.

/// Oberflächensprache (Einstellung `language`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Lang {
    #[default]
    De,
    En,
}

impl Lang {
    pub(crate) fn account(self) -> &'static str {
        match self {
            Lang::De => "Konto",
            Lang::En => "Account",
        }
    }

    pub(crate) fn current_account(self) -> &'static str {
        match self {
            Lang::De => "Aktueller Account",
            Lang::En => "Current account",
        }
    }

    pub(crate) fn free(self) -> &'static str {
        match self {
            Lang::De => "frei",
            Lang::En => "free",
        }
    }

    pub(crate) fn until(self, clock: &str) -> String {
        match self {
            Lang::De => format!("bis {clock}"),
            Lang::En => format!("until {clock}"),
        }
    }

    pub(crate) fn just_now(self) -> &'static str {
        match self {
            Lang::De => "gerade eben",
            Lang::En => "just now",
        }
    }

    pub(crate) fn minutes_ago(self, mins: i64) -> String {
        match self {
            Lang::De => format!("vor {mins} min"),
            Lang::En => format!("{mins} min ago"),
        }
    }

    /// Kurzer Wochentag, 0 = Montag. Deutsch mit Punkt wie das Datumsformat der Swift-App („Do.“).
    pub(crate) fn weekday(self, monday_based: u32) -> &'static str {
        const DE: [&str; 7] = ["Mo.", "Di.", "Mi.", "Do.", "Fr.", "Sa.", "So."];
        const EN: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
        let i = (monday_based % 7) as usize;
        match self {
            Lang::De => DE[i],
            Lang::En => EN[i],
        }
    }

    /// Trenner zwischen Wochentag und Uhrzeit: „Do., 14:00“ bzw. „Thu 14:00“.
    pub(crate) fn weekday_clock(self, monday_based: u32, clock: &str) -> String {
        match self {
            Lang::De => format!("{}, {clock}", self.weekday(monday_based)),
            Lang::En => format!("{} {clock}", self.weekday(monday_based)),
        }
    }
}
