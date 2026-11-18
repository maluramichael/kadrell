//! Datum und Achsenmarken der Statistik, in der Sprache der Oberfläche. Die Swift-Seite nutzt `Date.FormatStyle`
//! (Charts-Achsen), hier stehen die Formate von Hand, passend zu `StatsRange.axisFormat/barFormat`.

use chrono::{DateTime, Datelike, Duration, Local, TimeZone, Timelike, Weekday};
use kadrell_accounts::{Cal, StatsRange};
use kadrell_accounts::Lang;

const WEEKDAYS_DE: [&str; 7] = ["Mo.", "Di.", "Mi.", "Do.", "Fr.", "Sa.", "So."];
const WEEKDAYS_EN: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
const MONTHS_DE: [&str; 12] = ["Jan.", "Feb.", "März", "Apr.", "Mai", "Juni", "Juli", "Aug.", "Sept.", "Okt.", "Nov.", "Dez."];
const MONTHS_EN: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/// Erster Wochentag: Deutsch Montag, sonst Sonntag.
pub fn first_weekday(lang: Lang) -> Weekday {
    if lang == Lang::De { Weekday::Mon } else { Weekday::Sun }
}

pub fn cal(lang: Lang) -> Cal<Local> {
    Cal::new(Local, first_weekday(lang))
}

fn local(ts: f64) -> Option<DateTime<Local>> {
    Local.timestamp_opt(ts.floor() as i64, 0).single()
}

pub fn weekday(d: &DateTime<Local>, lang: Lang) -> &'static str {
    let i = d.weekday().num_days_from_monday() as usize;
    if lang == Lang::De { WEEKDAYS_DE[i] } else { WEEKDAYS_EN[i] }
}

pub fn month(d: &DateTime<Local>, lang: Lang) -> &'static str {
    let i = d.month0() as usize;
    if lang == Lang::De { MONTHS_DE[i] } else { MONTHS_EN[i] }
}

/// „Do. 14:30“ für die Warnung vor dem Wochenlimit.
pub fn weekday_clock(ts: f64, lang: Lang) -> String {
    let Some(d) = local(ts) else { return String::new() };
    format!("{} {:02}:{:02}", weekday(&d, lang), d.hour(), d.minute())
}

/// „Do., 12. Jan. 14:30“ bzw. „Thu, Jan 12 14:30“ (`with_minutes` falsch: nur die volle Stunde).
pub fn date_time(ts: f64, lang: Lang, with_minutes: bool) -> String {
    let Some(d) = local(ts) else { return String::new() };
    let time = if with_minutes { format!("{:02}:{:02}", d.hour(), d.minute()) } else { format!("{:02}", d.hour()) };
    match lang {
        Lang::De => format!("{} {}. {} {time}", weekday(&d, lang), d.day(), month(&d, lang)),
        Lang::En => format!("{} {} {}, {time}", weekday(&d, lang), month(&d, lang), d.day()),
    }
}

/// „Do., 12. Jan.“ für die Tageslinien.
pub fn day_label(ts: f64, lang: Lang) -> String {
    let Some(d) = local(ts) else { return String::new() };
    match lang {
        Lang::De => format!("{} {}. {}", weekday(&d, lang), d.day(), month(&d, lang)),
        Lang::En => format!("{} {} {}", weekday(&d, lang), month(&d, lang), d.day()),
    }
}

/// Datum eines Balkens: bei 24 Std. und 7 Tagen mit Stunde, sonst mit Jahr.
pub fn bar_time(ts: f64, range: StatsRange, lang: Lang) -> String {
    match range {
        StatsRange::Day | StatsRange::Week => date_time(ts, lang, true),
        StatsRange::Month | StatsRange::Year => {
            let Some(d) = local(ts) else { return String::new() };
            match lang {
                Lang::De => format!("{}. {} {}", d.day(), month(&d, lang), d.year()),
                Lang::En => format!("{} {}, {}", month(&d, lang), d.day(), d.year()),
            }
        }
    }
}

/// „12. Januar 2027“ für den Zählbeginn und „12. Jan. 2027“ für den Rekord.
pub fn long_date(ts: f64, lang: Lang) -> String {
    let Some(d) = local(ts) else { return String::new() };
    match lang {
        Lang::De => format!("{}. {} {}", d.day(), month(&d, lang), d.year()),
        Lang::En => format!("{} {}, {}", month(&d, lang), d.day(), d.year()),
    }
}

/// Marke der Zeitachse: Position als Zeitpunkt und Beschriftung.
#[derive(Clone, Debug, PartialEq)]
pub struct Tick {
    pub at: f64,
    pub label: String,
}

/// Achsenmarken wie `StatsRange.axisValues`: 24 Std. alle vier Stunden, 7 Tage jeden Tag, 30 Tage alle fünf Tage,
/// ein Jahr alle zwei Monate.
pub fn ticks(range: StatsRange, from: f64, now: f64, lang: Lang) -> Vec<Tick> {
    match range {
        StatsRange::Day => hour_ticks(from, now),
        StatsRange::Week => day_ticks(from, now, 1, |d| weekday(d, lang).to_string()),
        StatsRange::Month => day_ticks(from, now, 5, |d| match lang {
            Lang::De => format!("{}. {}", d.day(), month(d, lang)),
            Lang::En => format!("{} {}", month(d, lang), d.day()),
        }),
        StatsRange::Year => month_ticks(from, now, lang),
    }
}

fn hour_ticks(from: f64, now: f64) -> Vec<Tick> {
    let mut out = Vec::new();
    let mut at = (from / 3600.).ceil() * 3600.;
    while at <= now {
        if let Some(d) = local(at).filter(|d| d.hour() % 4 == 0 && d.minute() == 0) {
            out.push(Tick { at, label: format!("{:02}", d.hour()) });
        }
        at += 3600.;
    }
    out
}

fn day_ticks(from: f64, now: f64, step: i64, label: impl Fn(&DateTime<Local>) -> String) -> Vec<Tick> {
    let mut out = Vec::new();
    let Some(mut d) = local(from) else { return out };
    // Erster Mitternachtsbeginn nach `from`.
    d = Local.from_local_datetime(&(d.date_naive() + Duration::days(1)).and_hms_opt(0, 0, 0).unwrap_or_default()).earliest().unwrap_or(d);
    let mut n = 0;
    while (d.timestamp() as f64) <= now {
        if n % step == 0 {
            out.push(Tick { at: d.timestamp() as f64, label: label(&d) });
        }
        n += 1;
        let next = d.date_naive() + Duration::days(1);
        match Local.from_local_datetime(&next.and_hms_opt(0, 0, 0).unwrap_or_default()).earliest() {
            Some(x) => d = x,
            None => break,
        }
    }
    out
}

fn month_ticks(from: f64, now: f64, lang: Lang) -> Vec<Tick> {
    let mut out = Vec::new();
    let Some(start) = local(from) else { return out };
    let (mut y, mut m) = (start.year(), start.month());
    loop {
        (y, m) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
        let Some(d) = Local.with_ymd_and_hms(y, m, 1, 0, 0, 0).earliest() else { break };
        if (d.timestamp() as f64) > now {
            break;
        }
        if m % 2 == 1 {
            out.push(Tick { at: d.timestamp() as f64, label: month(&d, lang).to_string() });
        }
    }
    out
}

/// Sinnvolle Rasterwerte für die Prompt-Achse (0 bis `max`, höchstens fünf Linien).
pub fn count_ticks(max: usize) -> Vec<usize> {
    let mut step = 1;
    for s in [1, 2, 5] .into_iter().chain((1..9).flat_map(|p| [10usize.pow(p), 2 * 10usize.pow(p), 5 * 10usize.pow(p)])) {
        step = s;
        if max / s <= 4 {
            break;
        }
    }
    (0..=max.div_ceil(step)).map(|i| i * step).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn count_ticks_are_few_and_round() {
        assert_eq!(count_ticks(3), vec![0, 1, 2, 3]);
        assert_eq!(count_ticks(8), vec![0, 2, 4, 6, 8]);
        assert_eq!(count_ticks(37), vec![0, 10, 20, 30, 40]);
        assert!(count_ticks(1000).len() <= 6);
    }

    #[test]
    fn hour_ticks_every_four_hours() {
        let now = 1_800_000_000.;
        let t = ticks(StatsRange::Day, now - 86_400., now, Lang::De);
        assert!((5..=7).contains(&t.len()), "{} Marken", t.len());
        assert!(t.windows(2).all(|w| (w[1].at - w[0].at - 4. * 3600.).abs() < 3601.));
    }

    #[test]
    fn week_has_a_tick_per_day_and_month_every_five() {
        let now = 1_800_000_000.;
        assert!((6..=8).contains(&ticks(StatsRange::Week, now - 7. * 86_400., now, Lang::En).len()));
        assert!((5..=7).contains(&ticks(StatsRange::Month, now - 30. * 86_400., now, Lang::En).len()));
        assert!((5..=7).contains(&ticks(StatsRange::Year, now - 365. * 86_400., now, Lang::De).len()));
    }

    #[test]
    fn dates_follow_the_language() {
        let ts = Local.with_ymd_and_hms(2027, 1, 14, 14, 30, 0).unwrap().timestamp() as f64;
        assert_eq!(date_time(ts, Lang::De, true), "Do. 14. Jan. 14:30");
        assert_eq!(date_time(ts, Lang::En, true), "Thu Jan 14, 14:30");
        assert_eq!(long_date(ts, Lang::De), "14. Jan. 2027");
        assert_eq!(first_weekday(Lang::De), Weekday::Mon);
    }
}
