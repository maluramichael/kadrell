//! Reihen für die Statistik-Seite (Port von `StatsSeries.swift` und der reinen Tastenlogik aus `StatsModel.swift`):
//! Prompts in Balken und Projekte aufteilen, Limits je Konto als Linien mit gestrichelten Brücken über Messlücken.
//!
//! Zeitpunkte sind Sekunden seit der Unix-Epoche (`f64`). Kalenderfragen (Stundenraster, Tagesgrenzen, Wochenanfang)
//! beantwortet [`Cal`] mit einer Zeitzone und dem ersten Wochentag der Oberfläche (Swift: `Calendar.current`).

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, HashMap};

use chrono::{
    DateTime, Datelike, Duration, Months, NaiveDate, NaiveDateTime, TimeZone, Timelike, Weekday,
};

use crate::history::UsageSample;

const DAY: f64 = 24.0 * 3600.0;

/// Kalender: Zeitzone und erster Wochentag (Deutsch Montag, Englisch/US Sonntag).
#[derive(Clone, Debug)]
pub struct Cal<Tz: TimeZone> {
    pub tz: Tz,
    pub first_weekday: Weekday,
}

impl<Tz: TimeZone> Cal<Tz> {
    pub fn new(tz: Tz, first_weekday: Weekday) -> Self {
        Self { tz, first_weekday }
    }

    fn local(&self, ts: f64) -> Option<DateTime<Tz>> {
        self.tz.timestamp_opt(ts.floor() as i64, 0).single()
    }

    fn to_ts(&self, naive: NaiveDateTime) -> Option<f64> {
        let t = self
            .tz
            .from_local_datetime(&naive)
            .earliest()
            .or_else(|| self.tz.from_local_datetime(&naive).latest())?;
        Some(t.timestamp() as f64)
    }

    fn midnight(&self, date: NaiveDate) -> Option<f64> {
        self.to_ts(date.and_hms_opt(0, 0, 0)?)
    }

    pub fn start_of_day(&self, ts: f64) -> f64 {
        self.local(ts)
            .and_then(|d| self.midnight(d.date_naive()))
            .unwrap_or(ts)
    }

    fn start_of_week(&self, ts: f64) -> f64 {
        let Some(d) = self.local(ts) else { return ts };
        let back = (d.weekday().num_days_from_monday() + 7
            - self.first_weekday.num_days_from_monday())
            % 7;
        let date = d.date_naive() - Duration::days(i64::from(back));
        self.midnight(date).unwrap_or(ts)
    }

    fn start_of_month(&self, ts: f64) -> f64 {
        let Some(d) = self.local(ts) else { return ts };
        d.date_naive()
            .with_day(1)
            .and_then(|m| self.midnight(m))
            .unwrap_or(ts)
    }

    fn add_days(&self, ts: f64, days: i64) -> f64 {
        let moved = self
            .local(ts)
            .and_then(|d| self.to_ts(d.naive_local() + Duration::days(days)));
        moved.unwrap_or(ts + days as f64 * DAY)
    }

    fn add_month(&self, ts: f64) -> f64 {
        let moved = self
            .local(ts)
            .and_then(|d| self.to_ts(d.naive_local().checked_add_months(Months::new(1))?));
        moved.unwrap_or(ts + 30.0 * DAY)
    }

    fn hour_floor(&self, ts: f64, hours: u32) -> f64 {
        let Some(d) = self.local(ts) else { return ts };
        let hour = d.hour() / hours * hours;
        d.date_naive()
            .and_hms_opt(hour, 0, 0)
            .and_then(|n| self.to_ts(n))
            .unwrap_or(ts)
    }
}

/// Zeitraum der Statistik-Seite, zugleich die Breite der Prompt-Balken.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum StatsRange {
    Day,
    Week,
    Month,
    Year,
}

impl StatsRange {
    pub const ALL: [StatsRange; 4] = [
        StatsRange::Day,
        StatsRange::Week,
        StatsRange::Month,
        StatsRange::Year,
    ];

    pub fn raw(self) -> i32 {
        self as i32
    }

    pub fn from_raw(raw: i32) -> Option<StatsRange> {
        usize::try_from(raw)
            .ok()
            .and_then(|i| Self::ALL.get(i).copied())
    }

    /// Länge in Sekunden.
    pub fn span(self) -> f64 {
        match self {
            StatsRange::Day => DAY,
            StatsRange::Week => 7.0 * DAY,
            StatsRange::Month => 30.0 * DAY,
            StatsRange::Year => 365.0 * DAY,
        }
    }

    pub fn bar_start<Tz: TimeZone>(self, ts: f64, cal: &Cal<Tz>) -> f64 {
        match self {
            StatsRange::Day => cal.hour_floor(ts, 1),
            StatsRange::Week => cal.hour_floor(ts, 3),
            StatsRange::Month => cal.start_of_day(ts),
            StatsRange::Year => cal.start_of_week(ts),
        }
    }

    pub fn bar_end<Tz: TimeZone>(self, start: f64, cal: &Cal<Tz>) -> f64 {
        match self {
            StatsRange::Day => start + 3600.0,
            StatsRange::Week => start + 3.0 * 3600.0,
            StatsRange::Month => cal.add_days(start, 1),
            StatsRange::Year => cal.add_days(start, 7),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PromptEvent {
    pub time: f64,
    pub project: String,
}

/// Eine Session aus dem Verlauf, soweit die Statistik sie braucht (Swift: `HistorySession`).
#[derive(Clone, Debug, PartialEq)]
pub struct PromptSource {
    pub cwd: String,
    pub prompt_times: Vec<f64>,
}

/// Teil eines gestapelten Balkens: `low`..`high` auf der Prompt-Achse. `slot` `None` heißt „Andere“.
#[derive(Clone, Debug, PartialEq)]
pub struct PromptBar {
    pub start: f64,
    pub end: f64,
    pub project: String,
    pub slot: Option<usize>,
    pub low: usize,
    pub high: usize,
}

impl PromptBar {
    pub fn id(&self) -> String {
        format!("{}-{}", self.start, self.project)
    }

    pub fn count(&self) -> usize {
        self.high - self.low
    }

    /// Gezeichneter Teil im sichtbaren Zeitraum. Der Rand schneidet einen Balken oft mitten durch; ungekürzt läge er
    /// über der Achse oder der Liste daneben. Ein Zehntel der Breite bleibt frei, damit Balken nicht zusammenlaufen.
    pub fn visible_span(&self, from: f64, to: f64) -> (f64, f64) {
        let left = self.start.max(from);
        let right = (self.end - (self.end - self.start) * 0.1).min(to);
        (left, left.max(right))
    }
}

/// Ein Projekt der Liste neben dem Chart. `slot` ist die Farbe im Chart, `None` heißt „Andere“ oder herausgefiltert.
#[derive(Clone, Debug, PartialEq)]
pub struct ProjectCount {
    pub name: String,
    pub count: usize,
    pub slot: Option<usize>,
    /// Letzter Prompt im Zeitraum, für die Sortierung „Zuletzt“.
    pub last: f64,
    /// Steht im Chart: ohne Filter alle, mit Filter nur die gewählten.
    pub included: bool,
}

/// Reihenfolge der Projektliste, `s` schaltet reihum.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ProjectSort {
    #[default]
    Count,
    Name,
    Recent,
}

impl ProjectSort {
    pub fn next(self) -> ProjectSort {
        match self {
            ProjectSort::Count => ProjectSort::Name,
            ProjectSort::Name => ProjectSort::Recent,
            ProjectSort::Recent => ProjectSort::Count,
        }
    }

    pub fn sorted(self, mut projects: Vec<ProjectCount>) -> Vec<ProjectCount> {
        projects.sort_by(|a, b| {
            let by_name = natural_cmp(&a.name, &b.name);
            match self {
                ProjectSort::Count => b.count.cmp(&a.count).then(by_name),
                ProjectSort::Name => by_name,
                ProjectSort::Recent => b
                    .last
                    .partial_cmp(&a.last)
                    .unwrap_or(Ordering::Equal)
                    .then(by_name),
            }
        });
        projects
    }
}

/// Wie `localizedStandardCompare`: ohne Rücksicht auf Groß- und Kleinschreibung, Zahlen nach ihrem Wert.
fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (mut x, mut y) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (x.peek().copied(), y.peek().copied()) {
            (None, None) => return a.cmp(b),
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(c), Some(d)) if c.is_ascii_digit() && d.is_ascii_digit() => {
                let ord = number_cmp(&take_digits(&mut x), &take_digits(&mut y));
                if ord != Ordering::Equal {
                    return ord;
                }
            }
            (Some(c), Some(d)) => {
                let ord = c.to_lowercase().cmp(d.to_lowercase());
                if ord != Ordering::Equal {
                    return ord;
                }
                x.next();
                y.next();
            }
        }
    }
}

fn take_digits(it: &mut std::iter::Peekable<std::str::Chars<'_>>) -> String {
    let mut s = String::new();
    while let Some(c) = it.peek().copied().filter(char::is_ascii_digit) {
        s.push(c);
        it.next();
    }
    s
}

fn number_cmp(a: &str, b: &str) -> Ordering {
    let (a, b) = (a.trim_start_matches('0'), b.trim_start_matches('0'));
    a.len().cmp(&b.len()).then_with(|| a.cmp(b))
}

/// Beginn eines Tags (in der Jahresansicht eines Monats) als Trennlinie in den Charts. `labeled`: mit Datum daran.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DayMark {
    pub date: f64,
    pub labeled: bool,
}

impl DayMark {
    /// 24 Std. und 30 Tage beschriften die Linie selbst (an jedem Tag bzw. Wochenanfang), 7 Tage und 1 Jahr tragen
    /// das Datum schon an der Achse.
    pub fn marks<Tz: TimeZone>(
        range: StatsRange,
        from: f64,
        to: f64,
        cal: &Cal<Tz>,
    ) -> Vec<DayMark> {
        let monthly = range == StatsRange::Year;
        let step = |d: f64| {
            if monthly {
                cal.add_month(d)
            } else {
                cal.add_days(d, 1)
            }
        };
        let mut date = if monthly {
            cal.start_of_month(from)
        } else {
            cal.start_of_day(from)
        };
        if date < from {
            date = step(date);
        }
        let mut out = Vec::new();
        while date <= to {
            out.push(DayMark {
                date,
                labeled: Self::labeled(date, range, cal),
            });
            let next = step(date);
            if next <= date {
                break;
            }
            date = next;
        }
        out
    }

    fn labeled<Tz: TimeZone>(date: f64, range: StatsRange, cal: &Cal<Tz>) -> bool {
        match range {
            StatsRange::Day => true,
            StatsRange::Month => cal
                .local(date)
                .is_some_and(|d| d.weekday() == cal.first_weekday),
            StatsRange::Week | StatsRange::Year => false,
        }
    }
}

pub struct PromptStats;

impl PromptStats {
    /// So viele Projekte bekommen eine eigene Farbe, alle weiteren liegen zusammen unter „Andere“.
    pub const COLOR_SLOTS: usize = 7;
    /// Projektname von „Andere“; ein Ordner kann nicht leer heißen.
    pub const OTHER: &'static str = "";

    /// Alle Eingaben aus den Transcripts. Ein fortgesetztes Transcript kann ältere Zeilen kopieren, deshalb zählt
    /// derselbe Zeitstempel nur einmal.
    pub fn events(sessions: &[PromptSource]) -> Vec<PromptEvent> {
        let mut seen: BTreeSet<u64> = BTreeSet::new();
        let mut out = Vec::new();
        for session in sessions {
            let project = Self::project_name(&session.cwd);
            for &time in &session.prompt_times {
                if seen.insert(time.to_bits()) {
                    out.push(PromptEvent {
                        time,
                        project: project.clone(),
                    });
                }
            }
        }
        out
    }

    /// Ein Worktree zählt zum Projekt, in dem er liegt. Pfade mit `/` und `\`.
    pub fn project_name(cwd: &str) -> String {
        let unified = cwd.replace('\\', "/");
        let base = unified
            .split("/.claude/worktrees/")
            .next()
            .unwrap_or(&unified);
        let trimmed = base.trim_end_matches('/');
        match trimmed.rsplit('/').next() {
            Some(last) if !last.is_empty() => last.to_string(),
            _ => base.to_string(),
        }
    }

    /// Farbe je Projekt, nach den Prompts der letzten 30 Tage. Bewusst unabhängig vom gewählten Zeitraum: Ein
    /// Projekt behält beim Umschalten seine Farbe.
    pub fn slots(events: &[PromptEvent], now: f64) -> HashMap<String, usize> {
        let recent_from = now - StatsRange::Month.span();
        let mut recent: HashMap<&str, usize> = HashMap::new();
        let mut total: HashMap<&str, usize> = HashMap::new();
        for e in events {
            *total.entry(&e.project).or_default() += 1;
            if e.time >= recent_from {
                *recent.entry(&e.project).or_default() += 1;
            }
        }
        let rank = |p: &str| {
            (
                recent.get(p).copied().unwrap_or(0),
                total.get(p).copied().unwrap_or(0),
            )
        };
        let mut ranked: Vec<&str> = total.keys().copied().collect();
        ranked.sort_by(|a, b| rank(b).cmp(&rank(a)).then(a.cmp(b)));
        ranked
            .into_iter()
            .take(Self::COLOR_SLOTS)
            .enumerate()
            .map(|(i, p)| (p.to_string(), i))
            .collect()
    }

    pub fn in_range(events: &[PromptEvent], range: StatsRange, now: f64) -> Vec<PromptEvent> {
        let from = now - range.span();
        events
            .iter()
            .filter(|e| e.time >= from && e.time <= now)
            .cloned()
            .collect()
    }

    /// Gestapelte Balken für die schon gefilterten Eingaben: Projekte in Farbreihenfolge, „Andere“ oben.
    pub fn bars<Tz: TimeZone>(
        events: &[PromptEvent],
        range: StatsRange,
        slots: &HashMap<String, usize>,
        cal: &Cal<Tz>,
    ) -> Vec<PromptBar> {
        let mut buckets: BTreeMap<i64, HashMap<String, usize>> = BTreeMap::new();
        for e in events {
            let start = range.bar_start(e.time, cal) as i64;
            let project = if slots.contains_key(&e.project) {
                e.project.clone()
            } else {
                Self::OTHER.to_string()
            };
            *buckets
                .entry(start)
                .or_default()
                .entry(project)
                .or_default() += 1;
        }
        buckets
            .into_iter()
            .flat_map(|(start, counts)| {
                let start = start as f64;
                Self::stack(&counts, start, range.bar_end(start, cal), slots)
            })
            .collect()
    }

    fn stack(
        counts: &HashMap<String, usize>,
        start: f64,
        end: f64,
        slots: &HashMap<String, usize>,
    ) -> Vec<PromptBar> {
        let mut projects: Vec<&String> = counts.keys().collect();
        projects.sort_by_key(|p| slots.get(*p).copied().unwrap_or(Self::COLOR_SLOTS));
        let mut low = 0;
        projects
            .into_iter()
            .map(|project| {
                let high = low + counts[project];
                let bar = PromptBar {
                    start,
                    end,
                    project: project.clone(),
                    slot: slots.get(project).copied(),
                    low,
                    high,
                };
                low = high;
                bar
            })
            .collect()
    }

    /// Alle Projekte des Zeitraums, auch herausgefilterte (sonst ließen sie sich nicht wieder wählen), in `sort`.
    pub fn projects(
        events: &[PromptEvent],
        slots: &HashMap<String, usize>,
        filter: &BTreeSet<String>,
        sort: ProjectSort,
    ) -> Vec<ProjectCount> {
        let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
        let mut last: HashMap<&str, f64> = HashMap::new();
        for e in events {
            *counts.entry(&e.project).or_default() += 1;
            let l = last.entry(&e.project).or_insert(0.0);
            *l = l.max(e.time);
        }
        let list = counts
            .into_iter()
            .map(|(name, count)| {
                let included = filter.is_empty() || filter.contains(name);
                let slot = if included {
                    slots.get(name).copied()
                } else {
                    None
                };
                ProjectCount {
                    name: name.to_string(),
                    count,
                    slot,
                    last: last.get(name).copied().unwrap_or(0.0),
                    included,
                }
            })
            .collect();
        sort.sorted(list)
    }

    /// Mit Filter bekommen gewählte Projekte ohne eigene Farbe die frei gewordenen: Sonst stünde ein gewähltes Projekt
    /// grau als „Andere“ im Chart. Projekte mit Farbe behalten sie. (Swift: zweite Überladung von `slots`.)
    pub fn slots_filtered(
        slots: &HashMap<String, usize>,
        filter: &BTreeSet<String>,
        events: &[PromptEvent],
    ) -> HashMap<String, usize> {
        if filter.is_empty() {
            return slots.clone();
        }
        let mut out: HashMap<String, usize> = slots
            .iter()
            .filter(|(k, _)| filter.contains(*k))
            .map(|(k, v)| (k.clone(), *v))
            .collect();
        let mut free: Vec<usize> = (0..Self::COLOR_SLOTS)
            .filter(|s| !out.values().any(|v| v == s))
            .collect();
        let mut counts: HashMap<&str, usize> = HashMap::new();
        for e in events
            .iter()
            .filter(|e| filter.contains(&e.project) && !out.contains_key(&e.project))
        {
            *counts.entry(&e.project).or_default() += 1;
        }
        let mut names: Vec<&str> = counts.keys().copied().collect();
        names.sort_by(|a, b| counts[b].cmp(&counts[a]).then(a.cmp(b)));
        for name in names {
            if free.is_empty() {
                break;
            }
            out.insert(name.to_string(), free.remove(0));
        }
        out
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LimitKind {
    Session,
    Weekly,
    Fable,
}

impl LimitKind {
    pub const ALL: [LimitKind; 3] = [LimitKind::Session, LimitKind::Weekly, LimitKind::Fable];

    pub fn raw(self) -> i32 {
        self as i32
    }

    pub fn value(self, sample: &UsageSample) -> Option<i32> {
        match self {
            LimitKind::Session => sample.session,
            LimitKind::Weekly => sample.weekly,
            LimitKind::Fable => sample.fable,
        }
    }
}

/// `segment` trennt die Linie an Messlücken: Lief Kadrell nicht, gibt es für die Zeit keine Werte. Die Lücke
/// überbrückt eine gestrichelte Linie (`bridge`), damit sichtbar bleibt, dass dort nichts gemessen wurde.
#[derive(Clone, Debug, PartialEq)]
pub struct LimitPoint {
    pub date: f64,
    pub value: i32,
    pub kind: LimitKind,
    pub segment: usize,
    pub bridge: bool,
}

impl LimitPoint {
    pub fn id(&self) -> String {
        format!("{}-{}", self.series(), self.date)
    }

    pub fn series(&self) -> String {
        format!(
            "{}-{}{}",
            self.kind.raw(),
            if self.bridge { "b" } else { "" },
            self.segment
        )
    }
}

/// Ein Stück einer gestrichelten Brücke: Anfang und Ende als (Zeitpunkt, Wert).
type Bridge = ((f64, i32), (f64, i32));

pub struct LimitStats;

impl LimitStats {
    /// Mehr Punkte je Linie sieht man auf der Kachel nicht.
    pub const MAX_POINTS: f64 = 240.0;
    /// Ein inaktives Konto wird alle 20 Minuten gemessen, Lücken darunter sind normal.
    pub const MIN_GAP: f64 = 3600.0;

    pub fn points(
        samples: &[UsageSample],
        range: StatsRange,
        now: f64,
        kinds: &[LimitKind],
    ) -> Vec<LimitPoint> {
        let from = now - range.span();
        let mut visible: Vec<&UsageSample> = samples.iter().filter(|s| s.t >= from).collect();
        visible.sort_by(|a, b| a.t.partial_cmp(&b.t).unwrap_or(Ordering::Equal));
        kinds
            .iter()
            .flat_map(|k| Self::line(&visible, *k, range.span() / Self::MAX_POINTS))
            .collect()
    }

    /// Je Bucket der Höchststand: So bleibt die Spitze vor einem Reset auch in der Jahresansicht sichtbar.
    fn line(samples: &[&UsageSample], kind: LimitKind, width: f64) -> Vec<LimitPoint> {
        let mut peaks: Vec<(f64, i32)> = Vec::new();
        let mut bucket: Option<i64> = None;
        for s in samples {
            let Some(value) = kind.value(s) else { continue };
            let b = (s.t / width) as i64;
            if bucket != Some(b) {
                peaks.push((s.t, value));
                bucket = Some(b);
            } else if let Some(last) = peaks.last_mut().filter(|l| value >= l.1) {
                *last = (s.t, value);
            }
        }
        Self::segmented(&peaks, kind, Self::MIN_GAP.max(3.0 * width))
    }

    fn segmented(peaks: &[(f64, i32)], kind: LimitKind, gap: f64) -> Vec<LimitPoint> {
        let mut segment = 0;
        let mut previous: Option<f64> = None;
        peaks
            .iter()
            .map(|&(t, value)| {
                if previous.is_some_and(|p| t - p > gap) {
                    segment += 1;
                }
                previous = Some(t);
                LimitPoint {
                    date: t,
                    value,
                    kind,
                    segment,
                    bridge: false,
                }
            })
            .collect()
    }

    /// Gestrichelte Stücke, mit denen jede Linie den ganzen Zeitraum durchläuft: über Messlücken von Punkt zu Punkt,
    /// vorn ab dem Rand (Richtung letzter Wert davor), hinten mit dem letzten Wert bis jetzt. Je Stück zwei Punkte.
    pub fn bridges(
        points: &[LimitPoint],
        samples: &[UsageSample],
        range: StatsRange,
        now: f64,
        kinds: &[LimitKind],
    ) -> Vec<LimitPoint> {
        let from = now - range.span();
        let mut index = 0;
        let mut out = Vec::new();
        for &kind in kinds {
            let line: Vec<&LimitPoint> = points.iter().filter(|p| p.kind == kind).collect();
            let mut pairs = Self::lead(&line, Self::last_before(samples, kind, from), from, now);
            pairs.extend(Self::inner(&line));
            pairs.extend(Self::trail(&line, now));
            for (a, b) in pairs {
                for (date, value) in [a, b] {
                    out.push(LimitPoint {
                        date,
                        value,
                        kind,
                        segment: index,
                        bridge: true,
                    });
                }
                index += 1;
            }
        }
        out
    }

    fn last_before(samples: &[UsageSample], kind: LimitKind, before: f64) -> Option<(f64, i32)> {
        let mut best: Option<&UsageSample> = None;
        for s in samples
            .iter()
            .filter(|s| s.t < before && kind.value(s).is_some())
        {
            if best.is_none_or(|b| b.t < s.t) {
                best = Some(s);
            }
        }
        best.and_then(|s| kind.value(s).map(|v| (s.t, v)))
    }

    /// Vom linken Rand zum ersten Punkt, auf der Geraden vom letzten Wert vor dem Zeitraum. Gibt es im Zeitraum
    /// keinen Punkt, gilt der Wert davor bis jetzt. Ohne Wert davor beginnt die Linie beim ersten Punkt.
    fn lead(line: &[&LimitPoint], before: Option<(f64, i32)>, from: f64, now: f64) -> Vec<Bridge> {
        let Some((b_date, b_value)) = before else {
            return Vec::new();
        };
        let Some(first) = line.first() else {
            return vec![((from, b_value), (now, b_value))];
        };
        let share = (from - b_date) / (first.date - b_date).max(1.0);
        let value = (f64::from(b_value) + f64::from(first.value - b_value) * share).round() as i32;
        if first.date > from {
            vec![((from, value), (first.date, first.value))]
        } else {
            Vec::new()
        }
    }

    fn inner(line: &[&LimitPoint]) -> Vec<Bridge> {
        line.windows(2)
            .filter(|w| w[0].segment != w[1].segment)
            .map(|w| ((w[0].date, w[0].value), (w[1].date, w[1].value)))
            .collect()
    }

    /// Der letzte Wert gilt bis jetzt, bis eine neue Messung kommt.
    fn trail(line: &[&LimitPoint], now: f64) -> Vec<Bridge> {
        match line.last() {
            Some(last) if now > last.date => vec![((last.date, last.value), (now, last.value))],
            _ => Vec::new(),
        }
    }
}

/// Eine Kachel der Limits. Titel und Detail entstehen in der Oberfläche, weil sie von der Sprache abhängen.
#[derive(Clone, Debug, PartialEq)]
pub struct LimitSource {
    pub id: String,
    pub title: String,
    pub detail: String,
    pub kinds: Vec<LimitKind>,
    /// Letzter Stand aus dem Konto-Cache: So hat die Kachel gleich einen Punkt, auch vor der ersten neuen Messung.
    pub current: Option<UsageSample>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AccountLimits {
    pub id: String,
    pub title: String,
    pub detail: String,
    pub points: Vec<LimitPoint>,
    /// Gestrichelte Stücke über Lücken und bis an die Ränder, siehe [`LimitStats::bridges`].
    pub bridges: Vec<LimitPoint>,
}

impl AccountLimits {
    pub fn is_empty(&self) -> bool {
        self.points.is_empty() && self.bridges.is_empty()
    }
}

/// Was die Statistik-Seite zeigt, fertig gerechnet. Ein Jahr Verlauf dauert spürbar: nicht auf dem UI-Thread bauen.
#[derive(Clone, Debug, PartialEq)]
pub struct StatsSnapshot {
    pub range: StatsRange,
    pub from: f64,
    pub now: f64,
    pub limits: Vec<AccountLimits>,
    pub bars: Vec<PromptBar>,
    pub projects: Vec<ProjectCount>,
    pub prompt_total: usize,
    pub days: Vec<DayMark>,
    /// Gewählte Projekte; leer heißt alle.
    pub filter: BTreeSet<String>,
    pub sort: ProjectSort,
}

impl StatsSnapshot {
    #[allow(clippy::too_many_arguments)]
    pub fn make<Tz: TimeZone>(
        range: StatsRange,
        now: f64,
        samples: &[UsageSample],
        sources: &[LimitSource],
        events: &[PromptEvent],
        filter: &BTreeSet<String>,
        sort: ProjectSort,
        cal: &Cal<Tz>,
    ) -> StatsSnapshot {
        let visible = PromptStats::in_range(events, range, now);
        let shown: Vec<PromptEvent> = if filter.is_empty() {
            visible.clone()
        } else {
            visible
                .iter()
                .filter(|e| filter.contains(&e.project))
                .cloned()
                .collect()
        };
        let slots = PromptStats::slots_filtered(&PromptStats::slots(events, now), filter, &visible);
        let from = now - range.span();
        StatsSnapshot {
            range,
            from,
            now,
            limits: Self::limits(range, now, samples, sources),
            bars: PromptStats::bars(&shown, range, &slots, cal),
            projects: PromptStats::projects(&visible, &slots, filter, sort),
            prompt_total: shown.len(),
            days: DayMark::marks(range, from, now, cal),
            filter: filter.clone(),
            sort,
        }
    }

    fn limits(
        range: StatsRange,
        now: f64,
        samples: &[UsageSample],
        sources: &[LimitSource],
    ) -> Vec<AccountLimits> {
        let mut grouped: HashMap<&str, Vec<UsageSample>> = HashMap::new();
        for s in samples {
            grouped.entry(&s.account).or_default().push(s.clone());
        }
        sources
            .iter()
            .map(|source| {
                let own = Self::merged(
                    grouped.remove(source.id.as_str()).unwrap_or_default(),
                    source.current.as_ref(),
                );
                let points = LimitStats::points(&own, range, now, &source.kinds);
                let bridges = LimitStats::bridges(&points, &own, range, now, &source.kinds);
                AccountLimits {
                    id: source.id.clone(),
                    title: source.title.clone(),
                    detail: source.detail.clone(),
                    points,
                    bridges,
                }
            })
            .collect()
    }

    /// Der Cache-Stand zählt nur, wenn er neuer ist als die letzte Zeile, sonst stünde dieselbe Messung doppelt da.
    fn merged(mut samples: Vec<UsageSample>, current: Option<&UsageSample>) -> Vec<UsageSample> {
        let newest = samples.iter().map(|s| s.t).fold(0.0, f64::max);
        if let Some(c) = current.filter(|c| !c.is_empty() && c.t > newest) {
            samples.push(c.clone());
        }
        samples
    }
}

/// Eine Taste der Statistik-Seite, unabhängig vom Fenstersystem.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatsInput {
    Char(char),
    Left,
    Right,
    Up,
    Down,
    Delete,
}

/// Tasten der Projektliste: Pfeile Cursor, Leertaste filtert, Entf hebt den Filter auf, `s` sortiert reihum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatsListAction {
    Up,
    Down,
    Toggle,
    Clear,
    Sort,
}

impl StatsListAction {
    pub fn from_input(input: StatsInput) -> Option<StatsListAction> {
        match input {
            StatsInput::Up => Some(StatsListAction::Up),
            StatsInput::Down => Some(StatsListAction::Down),
            StatsInput::Delete => Some(StatsListAction::Clear),
            StatsInput::Char(' ') => Some(StatsListAction::Toggle),
            StatsInput::Char('s') => Some(StatsListAction::Sort),
            _ => None,
        }
    }
}

/// Reine Tastenlogik der Statistik-Seite (Swift: `StatsModel`).
pub struct StatsView;

impl StatsView {
    /// Cursor der Projektliste um einen Eintrag bewegen, an den Enden bleibt er stehen. Ohne gültigen Cursor
    /// beginnt er vorn (nach unten) bzw. hinten (nach oben).
    pub fn moved(cursor: Option<&str>, names: &[String], down: bool) -> Option<String> {
        if names.is_empty() {
            return None;
        }
        let Some(i) = cursor.and_then(|c| names.iter().position(|n| n == c)) else {
            return if down {
                names.first().cloned()
            } else {
                names.last().cloned()
            };
        };
        let next = if down {
            (i + 1).min(names.len() - 1)
        } else {
            i.saturating_sub(1)
        };
        Some(names[next].clone())
    }

    /// Zeitraum nach einer Taste: 1 bis 4 wählen direkt, Pfeile gehen einen Schritt und bleiben am Rand stehen.
    pub fn range_after(current: StatsRange, input: StatsInput) -> Option<StatsRange> {
        match input {
            StatsInput::Left => Some(StatsRange::from_raw(current.raw() - 1).unwrap_or(current)),
            StatsInput::Right => Some(StatsRange::from_raw(current.raw() + 1).unwrap_or(current)),
            StatsInput::Char(c) => StatsRange::from_raw(c.to_digit(10)? as i32 - 1),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    const NOW: f64 = 1_800_000_000.0; // 2027-01-15 08:00 UTC

    fn cal() -> Cal<Utc> {
        Cal::new(Utc, Weekday::Mon)
    }

    fn event(seconds_ago: f64, project: &str) -> PromptEvent {
        PromptEvent {
            time: NOW - seconds_ago,
            project: project.into(),
        }
    }

    fn set(items: &[&str]) -> BTreeSet<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    fn slots(items: &[(&str, usize)]) -> HashMap<String, usize> {
        items.iter().map(|(k, v)| (k.to_string(), *v)).collect()
    }

    // MARK: Prompts

    #[test]
    fn events_count_copied_lines_once() {
        let a = PromptSource {
            cwd: "/x/kadrell".into(),
            prompt_times: vec![1.0, 2.0],
        };
        let b = PromptSource {
            cwd: "/x/other".into(),
            prompt_times: vec![2.0, 3.0],
        };
        let mut times: Vec<f64> = PromptStats::events(&[a, b])
            .iter()
            .map(|e| e.time)
            .collect();
        times.sort_by(f64::total_cmp);
        assert_eq!(times, vec![1.0, 2.0, 3.0]);
    }

    #[test]
    fn worktree_counts_as_its_project() {
        assert_eq!(
            PromptStats::project_name("/x/kadrell/.claude/worktrees/stats-charts"),
            "kadrell"
        );
        assert_eq!(PromptStats::project_name("/x/kadrell"), "kadrell");
        assert_eq!(
            PromptStats::project_name("C:\\dev\\kadrell\\.claude\\worktrees\\a1"),
            "kadrell"
        );
        assert_eq!(PromptStats::project_name("C:\\dev\\kadrell"), "kadrell");
    }

    #[test]
    fn bars_stack_projects_in_one_hour() {
        let s = slots(&[("a", 0), ("b", 1)]);
        let events = [
            event(10.0, "a"),
            event(20.0, "a"),
            event(30.0, "b"),
            event(40.0, "c"),
        ];
        let bars = PromptStats::bars(&events, StatsRange::Day, &s, &cal());
        assert_eq!(
            bars.iter().map(|b| b.project.as_str()).collect::<Vec<_>>(),
            vec!["a", "b", PromptStats::OTHER]
        );
        assert_eq!(
            bars.iter().map(|b| b.low).collect::<Vec<_>>(),
            vec![0, 2, 3]
        );
        assert_eq!(
            bars.iter().map(|b| b.high).collect::<Vec<_>>(),
            vec![2, 3, 4]
        );
        assert_eq!(bars[2].slot, None);
    }

    #[test]
    fn bucket_width_follows_range() {
        let cal = cal();
        let start = StatsRange::Week.bar_start(NOW + 3600.0, &cal);
        assert_eq!(cal.local(start).unwrap().hour() % 3, 0);
        assert_eq!(StatsRange::Week.bar_end(start, &cal) - start, 3.0 * 3600.0);
        let month = StatsRange::Month.bar_start(NOW, &cal);
        assert_eq!(StatsRange::Month.bar_end(month, &cal) - month, DAY);
        let year = StatsRange::Year.bar_start(NOW, &cal);
        assert_eq!(
            cal.local(year).unwrap().weekday(),
            Weekday::Mon,
            "Woche beginnt am ersten Wochentag"
        );
        assert_eq!(StatsRange::Year.bar_end(year, &cal) - year, 7.0 * DAY);
    }

    /// Die Farbe hängt an den letzten 30 Tagen, nicht am gewählten Zeitraum, und ab dem achten Projekt gibt es keine mehr.
    #[test]
    fn slots_prefer_recent_projects_and_stop() {
        let mut events: Vec<PromptEvent> = (0..5)
            .map(|i| event(f64::from(i) + 40.0 * DAY, "alt"))
            .collect();
        events.push(event(100.0, "neu"));
        let s = PromptStats::slots(&events, NOW);
        assert_eq!(s["neu"], 0);
        assert_eq!(s["alt"], 1);
        let many: Vec<PromptEvent> = (0..12)
            .map(|i| event(f64::from(i), &format!("p{i}")))
            .collect();
        assert_eq!(
            PromptStats::slots(&many, NOW).len(),
            PromptStats::COLOR_SLOTS
        );
    }

    /// Der Rand des Zeitraums schneidet Balken: Sie liegen sonst über der Achse und der Projektliste.
    #[test]
    fn bar_is_cut_at_range_edges() {
        let bar = PromptBar {
            start: 1000.0,
            end: 2000.0,
            project: "a".into(),
            slot: Some(0),
            low: 0,
            high: 1,
        };
        assert_eq!(
            bar.visible_span(f64::MIN, f64::MAX),
            (1000.0, 1900.0),
            "ein Zehntel frei"
        );
        assert_eq!(bar.visible_span(1200.0, 1500.0), (1200.0, 1500.0));
        let sliver = bar.visible_span(1950.0, f64::MAX);
        assert_eq!(
            sliver.0, sliver.1,
            "liegt nur die Lücke im Zeitraum, bleibt nichts übrig"
        );
    }

    #[test]
    fn in_range_and_projects() {
        let events = [
            event(10.0, "a"),
            event(20.0, "a"),
            event(30.0, "b"),
            event(2.0 * DAY, "b"),
        ];
        let visible = PromptStats::in_range(&events, StatsRange::Day, NOW);
        assert_eq!(visible.len(), 3);
        let projects = PromptStats::projects(
            &visible,
            &slots(&[("a", 0)]),
            &BTreeSet::new(),
            ProjectSort::Count,
        );
        assert_eq!(
            projects.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(),
            vec!["a", "b"]
        );
        assert_eq!(
            projects.iter().map(|p| p.count).collect::<Vec<_>>(),
            vec![2, 1]
        );
        assert_eq!(
            projects.iter().map(|p| p.slot).collect::<Vec<_>>(),
            vec![Some(0), None]
        );
        assert_eq!(projects[0].last, NOW - 10.0, "letzter Prompt des Projekts");
    }

    // MARK: Filtern und Sortieren

    fn snapshot(events: &[PromptEvent], filter: &BTreeSet<String>) -> StatsSnapshot {
        StatsSnapshot::make(
            StatsRange::Day,
            NOW,
            &[],
            &[],
            events,
            filter,
            ProjectSort::Count,
            &cal(),
        )
    }

    /// Gefiltert stehen nur die gewählten Projekte im Chart, die Liste behält alle, damit man sie wieder wählen kann.
    #[test]
    fn filter_keeps_all_projects_in_list() {
        let events = [
            event(10.0, "a"),
            event(20.0, "b"),
            event(30.0, "b"),
            event(40.0, "c"),
        ];
        let snap = snapshot(&events, &set(&["b"]));
        assert_eq!(snap.prompt_total, 2);
        assert_eq!(
            snap.bars
                .iter()
                .map(|b| b.project.as_str())
                .collect::<BTreeSet<_>>(),
            BTreeSet::from(["b"])
        );
        assert_eq!(
            snap.projects
                .iter()
                .map(|p| p.name.as_str())
                .collect::<Vec<_>>(),
            vec!["b", "a", "c"]
        );
        assert_eq!(
            snap.projects.iter().map(|p| p.included).collect::<Vec<_>>(),
            vec![true, false, false]
        );
        assert_eq!(snap.projects[1].slot, None, "herausgefiltert: keine Farbe");
    }

    /// Ein gewähltes Projekt ohne eigene Farbe bekommt eine freie, statt grau als „Andere“ zu erscheinen.
    #[test]
    fn filtered_project_without_color_gets_free_slot() {
        let all: HashMap<String, usize> = (0..PromptStats::COLOR_SLOTS)
            .map(|i| (format!("p{i}"), i))
            .collect();
        let events = [event(10.0, "p0"), event(20.0, "rest")];
        let out = PromptStats::slots_filtered(&all, &set(&["p0", "rest"]), &events);
        assert_eq!(out["p0"], 0, "behält die eigene Farbe");
        assert_eq!(out["rest"], 1, "erste freie Farbe");
        assert_eq!(
            PromptStats::slots_filtered(&all, &BTreeSet::new(), &events),
            all,
            "ohne Filter unverändert"
        );
    }

    #[test]
    fn project_sort_orders() {
        let events = [
            event(300.0, "b"),
            event(200.0, "b"),
            event(100.0, "C"),
            event(50.0, "a"),
        ];
        let names = |sort| {
            PromptStats::projects(&events, &HashMap::new(), &BTreeSet::new(), sort)
                .into_iter()
                .map(|p| p.name)
                .collect::<Vec<_>>()
        };
        assert_eq!(names(ProjectSort::Count), vec!["b", "a", "C"]);
        assert_eq!(
            names(ProjectSort::Name),
            vec!["a", "b", "C"],
            "ohne Rücksicht auf Groß- und Kleinschreibung"
        );
        assert_eq!(names(ProjectSort::Recent), vec!["a", "C", "b"]);
        assert_eq!(ProjectSort::Recent.next(), ProjectSort::Count, "reihum");
    }

    #[test]
    fn natural_order_compares_numbers_by_value() {
        assert_eq!(natural_cmp("p2", "p10"), Ordering::Less);
        assert_eq!(natural_cmp("P2", "p2").reverse(), natural_cmp("p2", "P2"));
        assert_eq!(natural_cmp("a", "B"), Ordering::Less);
        assert!(
            !natural_cmp("007", "7").is_eq(),
            "gleicher Wert, aber nicht identisch: stabile Reihenfolge"
        );
    }

    // MARK: Tagestrenner

    #[test]
    fn day_marks_at_midnight() {
        let cal = cal();
        let marks = DayMark::marks(StatsRange::Week, NOW - StatsRange::Week.span(), NOW, &cal);
        assert_eq!(marks.len(), 7);
        assert!(marks.iter().all(|m| {
            cal.local(m.date)
                .is_some_and(|d| d.hour() == 0 && d.minute() == 0)
        }));
        assert!(
            !marks.iter().any(|m| m.labeled),
            "7 Tage: Datum steht an der Achse"
        );
        let day = DayMark::marks(StatsRange::Day, NOW - StatsRange::Day.span(), NOW, &cal);
        assert_eq!(day.len(), 1, "24 Std.: genau ein Tageswechsel");
        assert!(day[0].labeled);
    }

    #[test]
    fn day_marks_month_labels_week_start_year_uses_months() {
        let cal = cal();
        let month = DayMark::marks(StatsRange::Month, NOW - StatsRange::Month.span(), NOW, &cal);
        assert_eq!(month.len(), 30);
        let labeled: Vec<&DayMark> = month.iter().filter(|m| m.labeled).collect();
        assert!(
            labeled
                .iter()
                .all(|m| cal.local(m.date).unwrap().weekday() == cal.first_weekday)
        );
        assert_eq!(labeled.len(), 4);
        let year = DayMark::marks(StatsRange::Year, NOW - StatsRange::Year.span(), NOW, &cal);
        assert_eq!(year.len(), 12);
        assert!(year.iter().all(|m| cal.local(m.date).unwrap().day() == 1));
    }

    // MARK: Limits

    fn sample(seconds_ago: f64, weekly: i32) -> UsageSample {
        UsageSample {
            weekly: Some(weekly),
            ..UsageSample::new(NOW - seconds_ago, "a")
        }
    }

    fn gap_samples() -> Vec<UsageSample> {
        // Abstände größer als ein Bucket (6 Minuten): Sonst fasst die Verdichtung die Messungen zusammen.
        vec![
            sample(3.0 * 3600.0, 10),
            sample(3.0 * 3600.0 - 900.0, 11),
            sample(1200.0, 20),
            sample(300.0, 21),
        ]
    }

    #[test]
    fn limit_line_breaks_at_gap() {
        let points = LimitStats::points(&gap_samples(), StatsRange::Day, NOW, &[LimitKind::Weekly]);
        assert_eq!(
            points.iter().map(|p| p.segment).collect::<Vec<_>>(),
            vec![0, 0, 1, 1]
        );
    }

    /// Lücken und Ränder füllen gestrichelte Stücke: über die Lücke von Punkt zu Punkt, hinten bis jetzt.
    #[test]
    fn bridges_span_gap_and_trail() {
        let samples = gap_samples();
        let points = LimitStats::points(&samples, StatsRange::Day, NOW, &[LimitKind::Weekly]);
        let bridges = LimitStats::bridges(
            &points,
            &samples,
            StatsRange::Day,
            NOW,
            &[LimitKind::Weekly],
        );
        assert!(bridges.iter().all(|b| b.bridge));
        assert_eq!(
            bridges.iter().map(|b| b.value).collect::<Vec<_>>(),
            vec![11, 20, 21, 21],
            "Lücke, dann bis jetzt"
        );
        assert_eq!(bridges.last().unwrap().date, NOW);
        let series: BTreeSet<String> = bridges.iter().map(LimitPoint::series).collect();
        assert_eq!(series.len(), 2, "zwei getrennte Stücke");
        assert!(
            points.iter().all(|p| !series.contains(&p.series())),
            "eigene Serien, nicht an die Messlinie gehängt"
        );
    }

    /// Vorn beginnt die Linie am Rand, auf der Geraden vom letzten Wert davor zum ersten im Zeitraum.
    #[test]
    fn bridge_leads_from_value_before_range() {
        let samples = [sample(30.0 * 3600.0, 0), sample(18.0 * 3600.0, 60)];
        let points = LimitStats::points(&samples, StatsRange::Day, NOW, &[LimitKind::Weekly]);
        let bridges = LimitStats::bridges(
            &points,
            &samples,
            StatsRange::Day,
            NOW,
            &[LimitKind::Weekly],
        );
        assert_eq!(bridges[0].date, NOW - StatsRange::Day.span());
        assert_eq!(bridges[0].value, 30, "halber Weg von 0 zu 60");
    }

    /// Nichts gemessen im Zeitraum, aber davor: der letzte Wert gilt über den ganzen Zeitraum.
    #[test]
    fn bridge_carries_old_value_through_empty_range() {
        let samples = [sample(2.0 * DAY, 42)];
        let source = LimitSource {
            id: "a".into(),
            title: "A".into(),
            detail: String::new(),
            kinds: vec![LimitKind::Weekly],
            current: None,
        };
        let snap = StatsSnapshot::make(
            StatsRange::Day,
            NOW,
            &samples,
            &[source],
            &[],
            &BTreeSet::new(),
            ProjectSort::Count,
            &cal(),
        );
        let limits = &snap.limits[0];
        assert!(limits.points.is_empty());
        assert!(!limits.is_empty());
        assert_eq!(
            limits.bridges.iter().map(|b| b.value).collect::<Vec<_>>(),
            vec![42, 42]
        );
        assert_eq!(
            limits.bridges.iter().map(|b| b.date).collect::<Vec<_>>(),
            vec![NOW - StatsRange::Day.span(), NOW]
        );
    }

    /// Über ein Jahr fasst ein Punkt viele Messungen zusammen: Die Spitze vor dem Reset bleibt, der kleinere Wert fällt weg.
    #[test]
    fn limit_buckets_keep_peak() {
        let width = StatsRange::Year.span() / LimitStats::MAX_POINTS;
        let base = (NOW / width).floor() * width;
        let samples = [
            UsageSample {
                weekly: Some(90),
                ..UsageSample::new(base + 1.0, "a")
            },
            UsageSample {
                weekly: Some(3),
                ..UsageSample::new(base + 2.0, "a")
            },
        ];
        let points = LimitStats::points(&samples, StatsRange::Year, NOW, &[LimitKind::Weekly]);
        assert_eq!(points.iter().map(|p| p.value).collect::<Vec<_>>(), vec![90]);
    }

    #[test]
    fn limit_line_skips_old_and_missing_values() {
        let samples = [
            sample(2.0 * DAY, 5),
            UsageSample {
                session: Some(7),
                ..UsageSample::new(NOW - 60.0, "a")
            },
        ];
        assert!(
            LimitStats::points(&samples, StatsRange::Day, NOW, &[LimitKind::Weekly]).is_empty()
        );
        let session = LimitStats::points(&samples, StatsRange::Day, NOW, &[LimitKind::Session]);
        assert_eq!(session.iter().map(|p| p.value).collect::<Vec<_>>(), vec![7]);
    }

    /// Der Stand aus dem Konto-Cache füllt die Kachel vor der ersten neuen Messung, doppelt zählt er nicht.
    #[test]
    fn snapshot_adds_newer_cached_value_once() {
        let old = sample(600.0, 10);
        let source = |current: UsageSample| LimitSource {
            id: "a".into(),
            title: "A".into(),
            detail: String::new(),
            kinds: vec![LimitKind::Weekly],
            current: Some(current),
        };
        let values = |snap: StatsSnapshot| {
            snap.limits[0]
                .points
                .iter()
                .map(|p| p.value)
                .collect::<Vec<_>>()
        };
        let make = |src: LimitSource| {
            StatsSnapshot::make(
                StatsRange::Day,
                NOW,
                std::slice::from_ref(&old),
                &[src],
                &[],
                &BTreeSet::new(),
                ProjectSort::Count,
                &cal(),
            )
        };
        assert_eq!(values(make(source(sample(60.0, 12)))), vec![10, 12]);
        assert_eq!(values(make(source(old.clone()))), vec![10]);
    }

    // MARK: Tastatur

    #[test]
    fn range_keys() {
        use StatsInput::*;
        assert_eq!(
            StatsView::range_after(StatsRange::Week, Char('1')),
            Some(StatsRange::Day)
        );
        assert_eq!(
            StatsView::range_after(StatsRange::Week, Char('4')),
            Some(StatsRange::Year)
        );
        assert_eq!(StatsView::range_after(StatsRange::Week, Char('5')), None);
        assert_eq!(StatsView::range_after(StatsRange::Week, Char('0')), None);
        assert_eq!(StatsView::range_after(StatsRange::Week, Char('x')), None);
        assert_eq!(
            StatsView::range_after(StatsRange::Week, Right),
            Some(StatsRange::Month)
        );
        assert_eq!(
            StatsView::range_after(StatsRange::Day, Left),
            Some(StatsRange::Day),
            "bleibt am Rand stehen"
        );
        assert_eq!(
            StatsView::range_after(StatsRange::Week, Char(' ')),
            None,
            "Leertaste wählt keinen Zeitraum"
        );
    }

    #[test]
    fn list_keys() {
        use StatsInput::*;
        assert_eq!(StatsListAction::from_input(Up), Some(StatsListAction::Up));
        assert_eq!(
            StatsListAction::from_input(Down),
            Some(StatsListAction::Down)
        );
        assert_eq!(
            StatsListAction::from_input(Char(' ')),
            Some(StatsListAction::Toggle)
        );
        assert_eq!(
            StatsListAction::from_input(Char('s')),
            Some(StatsListAction::Sort)
        );
        assert_eq!(
            StatsListAction::from_input(Delete),
            Some(StatsListAction::Clear)
        );
        assert_eq!(StatsListAction::from_input(Char('x')), None);
    }

    #[test]
    fn cursor_moves() {
        let names: Vec<String> = ["a", "b", "c"].map(String::from).to_vec();
        let moved = |c: Option<&str>, down| StatsView::moved(c, &names, down);
        assert_eq!(moved(None, true).as_deref(), Some("a"));
        assert_eq!(moved(None, false).as_deref(), Some("c"));
        assert_eq!(moved(Some("a"), true).as_deref(), Some("b"));
        assert_eq!(
            moved(Some("c"), true).as_deref(),
            Some("c"),
            "bleibt am Ende"
        );
        assert_eq!(
            moved(Some("gone"), true).as_deref(),
            Some("a"),
            "verschwundenes Projekt: von vorn"
        );
        assert_eq!(StatsView::moved(Some("a"), &[], true), None);
    }
}
