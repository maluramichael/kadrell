//! Schnellstart (⌘⇧I): eine Idee eintippen, Kadrell erkennt das Projekt am Namen im Text oder mit einem lokalen Modell und
//! startet dort eine Session mit der Idee als erstem Prompt (Swift: `Services/QuickStart.swift` und die Zustandsmaschine
//! `QuickStartModel` aus `UI/QuickStartView.swift`). Hier alles ohne Oberfläche: Kandidatenindex, Namenssuche, Prompt
//! fürs Modell und der Ablauf des Dialogs als Zustandsmaschine (`QuickStartFlow`). Die Modellanfrage selbst macht der
//! Aufrufer über `LocalModel` (12 s Timeout) und gibt die Antwort an `QuickStartFlow::model_reply`.

use crate::folder_index::{self, Use};
use crate::local_model::ModelError;
use crate::{paths, text};
use serde_json::{Value, json};
use std::collections::HashMap;

/// Woher ein Kandidat kommt; kleiner heißt wichtiger (Gruppe vor Session vor Ordner vor Repo).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Source {
    Group,
    Session,
    Folder,
    Repo,
}

/// Ein Projekt, in dem der Schnellstart eine Session starten kann: Gruppe, Ordner früherer Sessions, benutzter Ordner
/// oder Git-Repo unter dem Projektordner, je Pfad einmal zusammengefasst.
#[derive(Clone, Debug, PartialEq)]
pub struct QuickStartCandidate {
    pub path: String,
    pub name: String,
    pub group_id: Option<String>,
    pub source: Source,
    /// Weitere Namen, unter denen das Projekt im Text vorkommen kann (Gruppenname, Ordnername).
    pub aliases: Vec<String>,
    pub sessions: u32,
    /// Letzte Nutzung (Session gestartet, Ordner benutzt), Sekunden seit 1970, 0 = unbekannt.
    pub last_used: f64,
    /// Oft und kürzlich benutzt, aus der Frecency plus Bonus für Gruppen und Sessions.
    pub score: f64,
    /// Titel der letzten Sessions dort, nur als Kontext fürs Modell.
    pub titles: Vec<String>,
}

impl QuickStartCandidate {
    pub fn new(path: &str, name: &str, source: Source, aliases: Vec<String>) -> QuickStartCandidate {
        QuickStartCandidate { path: path.into(), name: name.into(), group_id: None, source, aliases, sessions: 0, last_used: 0.0, score: 0.0, titles: vec![] }
    }
}

/// Kopie der Felder von `Group` aus dem Kern, die der Index braucht.
#[derive(Clone, Debug)]
pub struct GroupInfo {
    pub id: String,
    pub name: String,
    pub cwd: String,
    /// Remote-Gruppe (ssh-Host): kein lokales Projekt.
    pub host: Option<String>,
}

/// Kopie der Felder von `Session` aus dem Kern (offene und geschlossene).
#[derive(Clone, Debug)]
pub struct SessionInfo {
    pub cwd: String,
    pub started_at_ms: f64,
    pub is_remote: bool,
    pub is_shell: bool,
    pub title: String,
}

/// Kandidaten aus der Historie, ohne Plattenzugriff außer dem Existenz-Check der Ordner (`exists`).
pub fn build_index(groups: &[GroupInfo], sessions: &[SessionInfo], uses: &HashMap<String, Use>, repos: &[String], frecency: &dyn Fn(&str) -> f64, exists: &dyn Fn(&str) -> bool, home: &str) -> Vec<QuickStartCandidate> {
    let mut by_path: HashMap<String, QuickStartCandidate> = HashMap::new();
    let mut order: Vec<String> = Vec::new();
    let mut upsert = |path: &str, source: Source, change: &mut dyn FnMut(&mut QuickStartCandidate)| {
        let p = folder_index::normalize(path, false, home);
        if p.is_empty() || p == "/" {
            return;
        }
        let base = paths::last_component(&p).to_string();
        if !by_path.contains_key(&p) {
            order.push(p.clone());
        }
        let c = by_path.entry(p.clone()).or_insert_with(|| QuickStartCandidate::new(&p, &base, source, vec![base.clone()]));
        c.source = c.source.min(source);
        change(c);
    };
    for g in groups.iter().filter(|g| g.host.is_none()) {
        upsert(&g.cwd, Source::Group, &mut |c| {
            c.name = g.name.clone();
            c.group_id = Some(g.id.clone());
            if !c.aliases.contains(&g.name) {
                c.aliases.insert(0, g.name.clone());
            }
        });
    }
    for s in sessions.iter().filter(|s| !s.is_remote && !s.cwd.is_empty()) {
        upsert(&s.cwd, Source::Session, &mut |c| {
            c.sessions += 1;
            c.last_used = c.last_used.max(s.started_at_ms / 1000.0);
            if !s.is_shell && c.titles.len() < 3 && !c.titles.contains(&s.title) {
                c.titles.push(s.title.clone());
            }
        });
    }
    for (p, u) in uses {
        upsert(p, Source::Folder, &mut |c| c.last_used = c.last_used.max(u.last));
    }
    for p in repos {
        upsert(p, Source::Repo, &mut |_| {});
    }
    order
        .iter()
        .filter_map(|p| by_path.remove(p))
        .filter(|c| c.source == Source::Group || exists(&c.path))
        .map(|mut c| {
            c.score = frecency(&c.path) + if c.group_id.is_some() { 0.5 } else { 0.0 } + f64::from(c.sessions) * 0.1;
            c
        })
        .collect()
}

/// Ergebnis der Namenssuche: alle Kandidaten in Reihenfolge, die besten zuerst.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct QuickStartMatch {
    pub ranked: Vec<QuickStartCandidate>,
    /// Wie gut der erste Kandidat zum Text passt: 2 Name steht im Text, 1 Wortanfang passt, 0 kein Bezug.
    pub top_strength: u8,
    /// Genau ein Kandidat steht mit vollem Namen im Text: dann braucht es kein Modell zum Entscheiden.
    pub confident: bool,
}

impl QuickStartMatch {
    pub fn top(&self) -> Option<&QuickStartCandidate> {
        self.ranked.first()
    }
}

/// Ordnernamen, die als Wort in fast jedem Satz vorkommen und deshalb nie allein ein Projekt erkennen lassen.
pub const STOPWORDS: [&str; 34] = [
    "app", "apps", "api", "docs", "doc", "test", "tests", "web", "src", "code", "lib", "tmp", "der", "die", "das", "und", "the", "and", "for", "neu", "new", "idee", "idea", "main", "home", "projekt", "project", "projects",
    "projekte", "dev", "data", "site", "server", "client",
];

/// Einfache Zuordnung über Namen im Text, ohne Modell. „In RoboThomas habe ich …“ findet die Gruppe RoboThomas und den
/// Ordner `robo-thomas` (Bindestriche, Groß-/Kleinschreibung und Akzente zählen nicht).
pub fn match_text(input: &str, candidates: &[QuickStartCandidate]) -> QuickStartMatch {
    let words = words(input);
    let mut scored: Vec<(&QuickStartCandidate, u8)> = candidates.iter().map(|c| (c, strength(&words, c))).collect();
    scored.sort_by(compare);
    let strong = scored.iter().filter(|s| s.1 == 2).count();
    QuickStartMatch { top_strength: scored.first().map(|s| s.1).unwrap_or(0), confident: strong == 1, ranked: scored.into_iter().map(|s| s.0.clone()).collect() }
}

fn compare(a: &(&QuickStartCandidate, u8), b: &(&QuickStartCandidate, u8)) -> std::cmp::Ordering {
    b.1.cmp(&a.1)
        .then_with(|| if a.1 > 0 { a.0.source.cmp(&b.0.source) } else { std::cmp::Ordering::Equal })
        .then_with(|| b.0.score.total_cmp(&a.0.score))
        .then_with(|| b.0.last_used.total_cmp(&a.0.last_used))
        .then_with(|| a.0.path.to_lowercase().cmp(&b.0.path.to_lowercase()))
}

/// 2: ein Name steht als Wort (oder als zusammengezogene Wortfolge, „robo thomas“) im Text.
/// 1: ein Wort ab 4 Zeichen ist Anfang des Namens oder der Name Anfang des Worts („kadrells“).
pub fn strength(words: &[String], c: &QuickStartCandidate) -> u8 {
    let names: Vec<String> = c.aliases.iter().map(|a| text::normalize_name(a)).filter(|n| n.chars().count() >= 3 && !STOPWORDS.contains(&n.as_str())).collect();
    if names.is_empty() || words.is_empty() {
        return 0;
    }
    let joined = joined_runs(words, 3);
    if names.iter().any(|n| joined.contains(n)) {
        return 2;
    }
    let weak = words.iter().any(|w| {
        let wl = w.chars().count();
        wl >= 4 && names.iter().any(|n| (n.starts_with(w.as_str()) && wl * 2 >= n.chars().count()) || (n.chars().count() >= 4 && w.starts_with(n.as_str())))
    });
    u8::from(weak)
}

/// Alle Folgen aus bis zu `max_length` benachbarten Wörtern, zusammengezogen: „robo“, „thomas“, „robothomas“.
pub fn joined_runs(words: &[String], max_length: usize) -> std::collections::HashSet<String> {
    let mut out = std::collections::HashSet::new();
    for i in 0..words.len() {
        let mut s = String::new();
        for w in &words[i..words.len().min(i + max_length)] {
            s.push_str(w);
            out.insert(s.clone());
        }
    }
    out
}

pub fn words(s: &str) -> Vec<String> {
    s.split(|c: char| !c.is_alphanumeric()).map(text::normalize_name).filter(|w| !w.is_empty()).collect()
}

/// Bereinigten Prompt des Modells nur übernehmen, wenn er wirklich nur den Projektverweis los ist: Er ist das Original mit
/// genau einem herausgeschnittenen Stück (vorn, hinten oder in der Mitte), Wort für Wort gleich, mindestens zwei Wörter
/// lang, und im herausgeschnittenen Stück steht ein Name des Projekts. Sonst gilt der Originaltext.
pub fn accept_cleaned(cleaned: &str, original: &str, candidate: &QuickStartCandidate) -> bool {
    let (cw, ow) = (words(cleaned), words(original));
    if cw.len() < 2 || cw.len() >= ow.len() {
        return false;
    }
    let prefix = cw.iter().zip(&ow).take_while(|(a, b)| a == b).count();
    let suffix = cw.iter().rev().zip(ow.iter().rev()).take_while(|(a, b)| a == b).count();
    if prefix + suffix < cw.len() {
        return false;
    }
    let head = prefix.min(cw.len());
    let tail = cw.len() - head;
    let removed = joined_runs(&ow[head..ow.len() - tail], 3);
    candidate.aliases.iter().map(|a| text::normalize_name(a)).any(|a| a.chars().count() >= 3 && removed.contains(&a))
}

/// Frage ans Modell: kurze Kandidatenliste, Antwort als JSON mit Index, Sicherheit und bereinigtem Prompt.
/// Mehr Kandidaten machen ein kleines Modell langsam und unsicher: die per Namen passenden plus die zuletzt benutzten.
pub const PROMPT_LIMIT: usize = 25;

pub const SYSTEM: &str = r#"You map a user's idea to one of their software projects. Reply with JSON only:
{"index": <number of the project from the list, or -1 if none fits>, "sure": <true only if the text clearly names or describes that project>, "prompt": "<the idea without the reference to the project>"}
For "prompt": remove only the words that point to the project (e.g. "In RoboThomas I have the idea that the robot beeps" -> "the robot beeps", "Für Kadrell: Dark Mode einbauen" -> "Dark Mode einbauen"). Keep the language and everything else word for word. If unsure, return the idea unchanged."#;

pub fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {"index": {"type": "integer"}, "sure": {"type": "boolean"}, "prompt": {"type": "string"}},
        "required": ["index", "sure", "prompt"],
    })
}

/// Tag aus Sekunden seit 1970 als `yyyy-MM-dd` (UTC; Swift nimmt die Ortszeit, für die Modellfrage egal).
pub fn format_date(secs: f64) -> String {
    let z = (secs / 86_400.0).floor() as i64 + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

pub fn user(idea: &str, candidates: &[QuickStartCandidate], name_match: Option<usize>, home: &str) -> String {
    let lines: Vec<String> = candidates
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let mut parts = vec![format!("{i}: {}", c.name), paths::short_path(&c.path, home)];
            if c.sessions > 0 {
                parts.push(format!("{} sessions", c.sessions));
            }
            if c.last_used > 0.0 {
                parts.push(format!("last used {}", format_date(c.last_used)));
            }
            if !c.titles.is_empty() {
                parts.push(format!("recent: {}", c.titles.iter().take(2).cloned().collect::<Vec<_>>().join("; ")));
            }
            parts.join(" | ")
        })
        .collect();
    let mut out = format!("Projects:\n{}", lines.join("\n"));
    if let Some(n) = name_match {
        out.push_str(&format!("\n\nName search suggests: {n}"));
    }
    out + "\n\nIdea: " + idea
}

#[derive(Clone, Debug, PartialEq)]
pub struct Answer {
    pub index: i64,
    pub sure: bool,
    pub prompt: String,
}

pub fn parse_answer(reply: &str) -> Option<Answer> {
    let obj = text::first_json_object(reply)?;
    let index = obj.get("index").and_then(|v| v.as_i64().or_else(|| v.as_f64().map(|f| f as i64)))?;
    Some(Answer { index, sure: obj.get("sure").and_then(Value::as_bool).unwrap_or(false), prompt: obj.get("prompt").and_then(Value::as_str).unwrap_or("").to_string() })
}

// Ablauf des Dialogs

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Idea,
    Thinking,
    Pick,
}

/// Woher die Vorauswahl kommt, eine Zeile über der Liste (Texte tragen die Oberfläche, `Display` gibt den deutschen Grundtext).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OriginKind {
    NameConfident,
    NameAmbiguous,
    NameWeak,
    NoName,
    ModelSure,
    ModelUnsure,
    ModelNoProject,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Origin {
    pub kind: OriginKind,
    /// Anzeigename des Modells.
    pub model: String,
    /// Warum das Modell nicht half („Antwort nicht lesbar“, „übersprungen“, Fehlertext).
    pub failure: Option<String>,
}

impl std::fmt::Display for Origin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let m = &self.model;
        match self.kind {
            OriginKind::NameConfident => write!(f, "erkannt am Namen im Text")?,
            OriginKind::NameAmbiguous => write!(f, "mehrere Projekte passen zum Namen")?,
            OriginKind::NameWeak => write!(f, "ähnlicher Name im Text, bitte prüfen")?,
            OriginKind::NoName => write!(f, "kein Projekt im Text erkannt · zuletzt benutzte zuerst")?,
            OriginKind::ModelSure => write!(f, "erkannt von {m}")?,
            OriginKind::ModelUnsure => write!(f, "{m} vermutet, bitte prüfen")?,
            OriginKind::ModelNoProject => write!(f, "{m} findet kein passendes Projekt · zuletzt benutzte zuerst")?,
        }
        match &self.failure {
            Some(why) => write!(f, " · Modell: {why}"),
            None => Ok(()),
        }
    }
}

/// Anfrage ans Modell; der Aufrufer fragt (blockierend, `QUICK_START_TIMEOUT`) und ruft `model_reply` auf.
#[derive(Clone, Debug, PartialEq)]
pub struct ModelRequest {
    pub system: String,
    pub user: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Analysis {
    /// Leere Eingabe.
    Beep,
    /// Projektwahl steht (Phase `Pick`).
    Done,
    /// Phase `Thinking`: die Anfrage stellen.
    AskModel(ModelRequest),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Submitted {
    Analysis(Analysis),
    Start(Started),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Started {
    /// Kandidat und Prompt, mit dem die Session startet.
    Go(QuickStartCandidate, String),
    Beep,
    /// Der Ordner existiert nicht mehr (`error` ist gesetzt).
    Missing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cancelled {
    Close,
    Back,
}

pub const NOT_READABLE: &str = "Antwort nicht lesbar";
pub const SKIPPED: &str = "übersprungen";

pub struct QuickStartFlow {
    pub candidates: Vec<QuickStartCandidate>,
    idea: String,
    /// Namenssuche während des Tippens, für den Hinweis unter dem Feld.
    pub live: QuickStartMatch,
    filter: String,
    pub phase: Phase,
    pub list: Vec<QuickStartCandidate>,
    pub selected: usize,
    pub origin: Option<Origin>,
    /// Vom Modell bereinigter Prompt (Projektverweis entfernt), nur wenn `accept_cleaned` ihn durchlässt.
    pub cleaned: Option<String>,
    pub use_cleaned: bool,
    pub error: Option<String>,
    /// Anzeigename des Modells der Funktion, nil = Schnellstart ohne Modell (nur Namenssuche).
    pub model_name: Option<String>,
    /// Einstellung „Projektverweis aus dem Prompt nehmen“ (Standard an).
    pub strip_reference: bool,
    home: String,
    ranked: Vec<QuickStartCandidate>,
    pending: Option<QuickStartMatch>,
}

impl QuickStartFlow {
    pub fn new(candidates: Vec<QuickStartCandidate>, model_name: Option<&str>, home: &str) -> QuickStartFlow {
        QuickStartFlow {
            candidates,
            idea: String::new(),
            live: QuickStartMatch::default(),
            filter: String::new(),
            phase: Phase::Idea,
            list: vec![],
            selected: 0,
            origin: None,
            cleaned: None,
            use_cleaned: true,
            error: None,
            model_name: model_name.map(String::from),
            strip_reference: true,
            home: home.into(),
            ranked: vec![],
            pending: None,
        }
    }

    pub fn idea(&self) -> &str {
        &self.idea
    }

    pub fn filter(&self) -> &str {
        &self.filter
    }

    pub fn prompt(&self) -> String {
        self.cleaned.clone().filter(|_| self.use_cleaned).unwrap_or_else(|| self.idea.trim().to_string())
    }

    /// Neuer Stand aus dem Index (Repo-Scan fertig). Eine schon gezeigte Auswahl bleibt, wie sie ist.
    pub fn update_candidates(&mut self, candidates: Vec<QuickStartCandidate>) {
        self.candidates = candidates;
        if self.phase == Phase::Idea {
            self.live = match_text(&self.idea, &self.candidates);
        }
    }

    pub fn set_idea(&mut self, idea: &str) {
        if idea == self.idea {
            return;
        }
        self.idea = idea.to_string();
        if self.phase == Phase::Thinking {
            self.back_to_idea();
        }
        self.live = match_text(&self.idea, &self.candidates);
    }

    /// Tippen in der Projektwahl filtert wie ⌘N über Ordner- und Gruppennamen.
    pub fn set_filter(&mut self, filter: &str) {
        if filter != self.filter {
            self.filter = filter.to_string();
            self.update_list();
        }
    }

    pub fn submit(&mut self) -> Submitted {
        match self.phase {
            Phase::Idea => Submitted::Analysis(self.analyze()),
            Phase::Thinking => {
                self.skip_model();
                Submitted::Analysis(Analysis::Done)
            }
            Phase::Pick => Submitted::Start(self.start()),
        }
    }

    /// Esc: aus der Projektwahl zurück zur Idee, beim Warten aufs Modell ebenso, sonst zu.
    pub fn cancel(&mut self) -> Cancelled {
        if self.phase == Phase::Idea {
            return Cancelled::Close;
        }
        self.back_to_idea();
        Cancelled::Back
    }

    fn back_to_idea(&mut self) {
        self.pending = None;
        self.phase = Phase::Idea;
        self.filter.clear();
        self.error = None;
    }

    /// Erst die Namenssuche. Das Modell fragt Kadrell nur, wenn es eingeschaltet ist und entweder der Name nicht eindeutig
    /// ist oder es den Projektverweis aus dem Prompt nehmen soll.
    pub fn analyze(&mut self) -> Analysis {
        let input = self.idea.trim().to_string();
        if input.is_empty() {
            return Analysis::Beep;
        }
        let m = match_text(&input, &self.candidates);
        self.ranked = m.ranked.clone();
        self.cleaned = None;
        self.use_cleaned = true;
        self.error = None;
        if self.model_name.is_none() || self.candidates.is_empty() || (m.confident && !self.strip_reference) {
            self.finish(&m, None, None);
            return Analysis::Done;
        }
        self.phase = Phase::Thinking;
        let shortlist = &m.ranked[..m.ranked.len().min(PROMPT_LIMIT)];
        let request = ModelRequest { system: SYSTEM.into(), user: user(&input, shortlist, (m.top_strength > 0).then_some(0), &self.home) };
        self.pending = Some(m);
        Analysis::AskModel(request)
    }

    /// Antwort (oder Fehler) des Modells; zählt nur in der Phase `Thinking`.
    pub fn model_reply(&mut self, reply: Result<String, ModelError>) {
        let (Phase::Thinking, Some(m)) = (self.phase, self.pending.take()) else { return };
        match reply {
            Ok(text) => match parse_answer(&text) {
                Some(answer) => self.finish(&m, Some(answer), None),
                None => self.finish(&m, None, Some(NOT_READABLE.to_string())),
            },
            Err(e) => self.finish(&m, None, Some(e.to_string())),
        }
    }

    /// Enter beim Warten: nicht länger aufs Modell warten, die Namenssuche entscheidet.
    fn skip_model(&mut self) {
        self.pending = None;
        let m = match_text(&self.idea, &self.candidates);
        self.finish(&m, None, Some(SKIPPED.to_string()));
    }

    fn finish(&mut self, m: &QuickStartMatch, answer: Option<Answer>, failure: Option<String>) {
        self.ranked = m.ranked.clone();
        let (pick, origin) = self.decide(m, answer.as_ref(), failure);
        self.origin = Some(origin);
        self.accept_cleaned_prompt(answer.as_ref(), pick.as_ref());
        if let Some(i) = pick.and_then(|p| self.ranked.iter().position(|c| *c == p)) {
            let c = self.ranked.remove(i);
            self.ranked.insert(0, c);
        }
        self.pending = None;
        self.phase = Phase::Pick;
        self.filter.clear();
        self.update_list();
    }

    fn name_origin(&self, m: &QuickStartMatch) -> OriginKind {
        match m.top_strength {
            2 if m.confident => OriginKind::NameConfident,
            2 => OriginKind::NameAmbiguous,
            1 => OriginKind::NameWeak,
            _ => OriginKind::NoName,
        }
    }

    /// Welches Projekt vorn steht und warum: eindeutiger Name schlägt das Modell, sonst entscheidet dessen Antwort.
    fn decide(&self, m: &QuickStartMatch, answer: Option<&Answer>, failure: Option<String>) -> (Option<QuickStartCandidate>, Origin) {
        let model = self.model_name.clone().unwrap_or_default();
        let mut origin = Origin { kind: self.name_origin(m), model, failure: None };
        let mut pick = m.top().cloned();
        let in_list = |i: i64| usize::try_from(i).ok().filter(|i| *i < self.ranked.len().min(PROMPT_LIMIT));
        match answer {
            Some(a) if !m.confident => match in_list(a.index) {
                Some(i) => {
                    pick = self.ranked.get(i).cloned();
                    origin.kind = if a.sure { OriginKind::ModelSure } else { OriginKind::ModelUnsure };
                }
                None if a.index < 0 => origin.kind = OriginKind::ModelNoProject,
                None => {}
            },
            _ => {
                if !m.confident {
                    origin.failure = failure;
                }
            }
        }
        (pick, origin)
    }

    fn accept_cleaned_prompt(&mut self, answer: Option<&Answer>, pick: Option<&QuickStartCandidate>) {
        let (Some(a), Some(p)) = (answer, pick) else { return };
        if a.sure && self.strip_reference && accept_cleaned(&a.prompt, &self.idea, p) {
            self.cleaned = Some(a.prompt.trim().to_string());
        }
    }

    fn update_list(&mut self) {
        let q = self.filter.trim_matches([' ', '\t']).to_string();
        self.list = if q.is_empty() {
            self.ranked.clone()
        } else {
            // Nur Treffer im Namen (Rang 0 bis 2): die Buchstabenfolge irgendwo im langen Pfad trifft sonst fast alles.
            let mut hits: Vec<(usize, u8, &QuickStartCandidate)> =
                self.ranked.iter().enumerate().filter_map(|(i, c)| folder_index::rank(&q, &format!("{}/{}", c.path, c.name)).filter(|r| *r < 3).map(|r| (i, r, c))).collect();
            hits.sort_by_key(|h| (h.1, h.0));
            hits.into_iter().map(|h| h.2.clone()).collect()
        };
        self.selected = 0;
    }

    pub fn move_selection(&mut self, d: isize) {
        let max = self.list.len().saturating_sub(1) as isize;
        self.selected = (self.selected as isize + d).clamp(0, max) as usize;
    }

    /// Tab in der Projektwahl: zwischen bereinigtem Prompt und Originaltext wechseln. false = es gibt keinen bereinigten.
    pub fn toggle_cleaned(&mut self) -> bool {
        if self.cleaned.is_none() {
            return false;
        }
        self.use_cleaned = !self.use_cleaned;
        true
    }

    pub fn pick(&mut self, i: usize) -> Option<Started> {
        if i >= self.list.len() {
            return None;
        }
        self.selected = i;
        Some(self.start())
    }

    pub fn start(&mut self) -> Started {
        let Some(c) = self.list.get(self.selected).cloned() else { return Started::Beep };
        if !folder_index::is_directory(&c.path) {
            self.error = Some(format!("{} existiert nicht mehr", paths::short_path(&c.path, &self.home)));
            return Started::Missing;
        }
        Started::Go(c, self.prompt())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::testdir::TempDir;

    fn candidate(path: &str, name: Option<&str>, source: Source, score: f64, group_id: Option<&str>) -> QuickStartCandidate {
        let base = paths::last_component(path).to_string();
        let aliases = match name {
            Some(n) => vec![n.to_string(), base.clone()],
            None => vec![base.clone()],
        };
        QuickStartCandidate { name: name.map(String::from).unwrap_or(base), group_id: group_id.map(String::from), score, ..QuickStartCandidate::new(path, "", source, aliases) }
    }

    fn projects() -> Vec<QuickStartCandidate> {
        vec![
            candidate("/p/kadrell", Some("Kadrell"), Source::Group, 5.0, Some("g1")),
            candidate("/p/robo-thomas", None, Source::Repo, 1.0, None),
            candidate("/p/bloom", Some("Bloom"), Source::Group, 3.0, Some("g2")),
            candidate("/p/app", None, Source::Repo, 9.0, None),
        ]
    }

    #[test]
    fn index_merges_groups_sessions_uses_and_repos() {
        let g = GroupInfo { id: "g1".into(), name: "Kadrell".into(), cwd: "/p/kadrell".into(), host: None };
        let remote = GroupInfo { id: "g9".into(), name: "pc".into(), cwd: "~".into(), host: Some("pc".into()) };
        let s = SessionInfo { cwd: "/p/kadrell".into(), started_at_ms: 1_700_000_000_000.0, is_remote: false, is_shell: false, title: "Statusleiste".into() };
        let gone = SessionInfo { cwd: "/p/weg".into(), started_at_ms: 1.0, is_remote: false, is_shell: false, title: String::new() };
        let uses = HashMap::from([("/p/bloom".to_string(), Use { count: 3, last: 10.0 })]);
        let repos = vec!["/p/kadrell".to_string(), "/p/robo-thomas".to_string()];
        let index = build_index(&[g, remote], &[s, gone], &uses, &repos, &|p| if p == "/p/bloom" { 2.0 } else { 0.0 }, &|p| p != "/p/weg", "/h");
        assert_eq!(index.iter().map(|c| c.path.as_str()).collect::<Vec<_>>(), ["/p/kadrell", "/p/bloom", "/p/robo-thomas"], "je Pfad einmal, Remote und verschwundene Ordner nicht");
        let k = &index[0];
        assert_eq!((k.name.as_str(), k.group_id.as_deref(), k.source, k.sessions), ("Kadrell", Some("g1"), Source::Group, 1));
        assert_eq!(k.titles, ["Statusleiste"]);
        assert_eq!(k.last_used, 1_700_000_000.0);
        assert_eq!(index[1].last_used, 10.0);
        assert_eq!(index[1].score, 2.0);
        assert!((k.score - 0.6).abs() < 0.001, "Gruppe und Sessions geben Bonus");
    }

    #[test]
    fn name_in_text_is_confident() {
        let m = match_text("In RoboThomas habe ich die Idee, dass der Roboter piept", &projects());
        assert_eq!(m.top().unwrap().path, "/p/robo-thomas", "Bindestrich und Großschreibung zählen nicht");
        assert_eq!(m.top_strength, 2);
        assert!(m.confident);
        let spaced = match_text("bei robo thomas: Akku anzeigen", &projects());
        assert_eq!(spaced.top().unwrap().path, "/p/robo-thomas", "zusammengezogene Wortfolge");
        assert!(spaced.confident);
    }

    #[test]
    fn stopword_folder_never_matches_alone() {
        let m = match_text("Die App soll einen Dark Mode bekommen", &projects());
        assert_eq!(m.top_strength, 0, "„app“ ist kein Projektname im Satz");
        assert!(!m.confident);
        assert_eq!(m.top().unwrap().path, "/p/app", "ohne Treffer zählt die Nutzung");
    }

    #[test]
    fn prefix_is_weak_and_ambiguity_is_not_confident() {
        let weak = match_text("im kadrel noch einen Knopf", &projects());
        assert_eq!(weak.top().unwrap().path, "/p/kadrell");
        assert_eq!(weak.top_strength, 1);
        assert!(!weak.confident);
        let two = match_text("Kadrell und Bloom angleichen", &projects());
        assert_eq!(two.top_strength, 2);
        assert!(!two.confident, "zwei Namen im Text: das Modell oder der Nutzer entscheidet");
    }

    #[test]
    fn cleaned_prompt_only_when_only_the_reference_is_gone() {
        let c = &projects()[1];
        let original = "In RoboThomas habe ich die Idee, dass der Roboter beim Start piept.";
        assert!(accept_cleaned("der Roboter beim Start piept.", original, c));
        assert!(accept_cleaned("Der Roboter beim Start piept", original, c));
        assert!(!accept_cleaned("Der Roboter soll beim Start laut piepen", original, c), "umformuliert");
        assert!(!accept_cleaned("habe ich die Idee, dass der Roboter beim Start", original, c), "Projektname bleibt drin, Inhalt fehlt");
        assert!(!accept_cleaned(original, original, c));
        assert!(!accept_cleaned("piept", original, c), "zu kurz");
    }

    #[test]
    fn prompt_lists_candidates_and_parses_answer() {
        let mut c = projects()[0].clone();
        c.sessions = 4;
        c.last_used = 1_761_800_000.0;
        c.titles = vec!["Statusleiste".into()];
        let u = user("Idee", &[c, projects()[2].clone()], Some(0), "/h");
        assert!(u.contains("0: Kadrell | /p/kadrell | 4 sessions | last used 2025-10-"));
        assert!(u.contains("1: Bloom"));
        assert!(u.contains("recent: Statusleiste"));
        assert!(u.contains("Name search suggests: 0"));
        assert!(u.ends_with("Idea: Idee"));
        assert_eq!(parse_answer("```json\n{\"index\": 1, \"sure\": true, \"prompt\": \"X\"}\n```"), Some(Answer { index: 1, sure: true, prompt: "X".into() }));
        assert_eq!(parse_answer("weiß nicht"), None);
        assert_eq!(format_date(0.0), "1970-01-01");
        assert_eq!(format_date(1_793_613_239.0), "2026-11-02");
    }

    // Ablauf im Dialog

    /// Existierende Ordner, damit `start` sie findet; Tempordner bleibt über den Test erhalten.
    fn existing_projects(t: &TempDir) -> Vec<QuickStartCandidate> {
        for n in ["kadrell", "robo-thomas", "bloom"] {
            t.mkdir(n);
        }
        vec![
            candidate(&t.sub("kadrell"), Some("Kadrell"), Source::Group, 5.0, Some("g1")),
            candidate(&t.sub("robo-thomas"), None, Source::Folder, 1.0, None),
            candidate(&t.sub("bloom"), Some("Bloom"), Source::Folder, 3.0, None),
        ]
    }

    fn type_idea(flow: &mut QuickStartFlow, idea: &str) -> Submitted {
        flow.set_idea(idea);
        flow.submit()
    }

    fn go(s: Started) -> (String, String) {
        match s {
            Started::Go(c, p) => (c.name, p),
            other => panic!("Start erwartet, bekommen {other:?}"),
        }
    }

    #[test]
    fn without_model_name_match_starts_with_original_text() {
        let t = TempDir::new("quick");
        let mut flow = QuickStartFlow::new(existing_projects(&t), None, "/h");
        flow.set_idea("In RoboThomas habe ich die Idee, dass er piept");
        assert_eq!(flow.live.top().unwrap().name, "robo-thomas", "Hinweis schon beim Tippen");
        assert_eq!(flow.submit(), Submitted::Analysis(Analysis::Done));
        assert_eq!(flow.phase, Phase::Pick, "erst bestätigen, dann starten");
        assert_eq!(flow.list[0].name, "robo-thomas");
        let Submitted::Start(started) = flow.submit() else { panic!("Start erwartet") };
        assert_eq!(go(started), ("robo-thomas".into(), "In RoboThomas habe ich die Idee, dass er piept".into()));
    }

    #[test]
    fn typing_and_arrows_pick_another_project() {
        let t = TempDir::new("quick");
        let mut flow = QuickStartFlow::new(existing_projects(&t), None, "/h");
        type_idea(&mut flow, "Ein neuer Knopf");
        assert_eq!(flow.list[0].name, "Kadrell", "ohne Namen: meistbenutztes zuerst");
        flow.move_selection(1);
        assert_eq!(flow.list[flow.selected].name, "Bloom");
        flow.set_filter("rob");
        assert_eq!(flow.list.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["robo-thomas"]);
        let Submitted::Start(started) = flow.submit() else { panic!("Start erwartet") };
        assert_eq!(go(started).0, "robo-thomas");
    }

    #[test]
    fn escape_goes_back_then_closes() {
        let t = TempDir::new("quick");
        let mut flow = QuickStartFlow::new(existing_projects(&t), None, "/h");
        type_idea(&mut flow, "Kadrell bekommt X");
        assert_eq!(flow.cancel(), Cancelled::Back);
        assert_eq!(flow.phase, Phase::Idea);
        assert_eq!(flow.idea(), "Kadrell bekommt X", "Idee bleibt beim Zurückgehen");
        assert_eq!(flow.cancel(), Cancelled::Close);
    }

    fn ask(flow: &mut QuickStartFlow, idea: &str) -> ModelRequest {
        match type_idea(flow, idea) {
            Submitted::Analysis(Analysis::AskModel(r)) => r,
            other => panic!("Modellanfrage erwartet, bekommen {other:?}"),
        }
    }

    /// Antwort des Modells, das den Index des Projekts mit `name` aus der Liste in der Anfrage nimmt.
    fn pick_reply(request: &ModelRequest, name: &str, prompt: &str) -> Result<String, ModelError> {
        let index = request.user.lines().find(|l| l.contains(name)).and_then(|l| l.split(':').next()?.parse::<i64>().ok()).unwrap_or(-1);
        Ok(format!("{{\"index\": {index}, \"sure\": true, \"prompt\": \"{prompt}\"}}"))
    }

    #[test]
    fn model_decides_when_name_is_unclear_and_cleans_prompt() {
        let t = TempDir::new("quick");
        let mut flow = QuickStartFlow::new(existing_projects(&t), Some("qwen3:4b"), "/h");
        let request = ask(&mut flow, "Beim Roboter möchte ich, dass er beim Start piept");
        assert_eq!(flow.phase, Phase::Thinking);
        assert!(request.system.contains("JSON"));
        assert!(request.user.contains("Idea: Beim Roboter möchte ich, dass er beim Start piept"));
        flow.model_reply(pick_reply(&request, "robo-thomas", "dass er beim Start piept"));
        assert_eq!(flow.phase, Phase::Pick);
        assert_eq!(flow.list[0].name, "robo-thomas", "Modell wählt aus der Liste");
        assert_eq!(flow.cleaned, None, "„Roboter“ ist kein Name des Projekts: der Prompt bleibt, wie er ist");
        assert!(flow.origin.as_ref().unwrap().to_string().contains("qwen3:4b"));
    }

    #[test]
    fn model_strips_reference_when_safe() {
        let t = TempDir::new("quick");
        let mut flow = QuickStartFlow::new(existing_projects(&t), Some("m"), "/h");
        let request = ask(&mut flow, "In RoboThomas: der Roboter piept beim Start");
        flow.model_reply(Ok(r#"{"index": 0, "sure": true, "prompt": "der Roboter piept beim Start"}"#.into()));
        let _ = request;
        assert_eq!(flow.list[0].name, "robo-thomas", "eindeutiger Name schlägt das Modell");
        assert_eq!(flow.prompt(), "der Roboter piept beim Start");
        assert!(flow.toggle_cleaned());
        assert_eq!(flow.prompt(), "In RoboThomas: der Roboter piept beim Start", "Tab: zurück zum Originaltext");
        assert!(flow.toggle_cleaned());
        let Submitted::Start(started) = flow.submit() else { panic!("Start erwartet") };
        assert_eq!(go(started).1, "der Roboter piept beim Start");
    }

    #[test]
    fn strip_reference_off_skips_the_model_when_the_name_is_clear() {
        let t = TempDir::new("quick");
        let mut flow = QuickStartFlow::new(existing_projects(&t), Some("m"), "/h");
        flow.strip_reference = false;
        assert_eq!(type_idea(&mut flow, "In RoboThomas: piept"), Submitted::Analysis(Analysis::Done));
        assert_eq!(flow.origin.as_ref().unwrap().kind, OriginKind::NameConfident);
    }

    #[test]
    fn model_failure_falls_back_to_name_search() {
        let t = TempDir::new("quick");
        let mut flow = QuickStartFlow::new(existing_projects(&t), Some("m"), "/h");
        ask(&mut flow, "irgendwas Neues");
        flow.model_reply(Err(ModelError::Timeout));
        assert_eq!(flow.phase, Phase::Pick);
        assert_eq!(flow.list[0].name, "Kadrell");
        assert!(flow.origin.as_ref().unwrap().to_string().contains(&ModelError::Timeout.to_string()));

        let mut second = QuickStartFlow::new(existing_projects(&t), Some("m"), "/h");
        ask(&mut second, "noch was");
        second.model_reply(Ok("weiß nicht".into()));
        assert_eq!(second.origin.as_ref().unwrap().failure.as_deref(), Some(NOT_READABLE));
    }

    #[test]
    fn enter_while_thinking_skips_the_model() {
        let t = TempDir::new("quick");
        let mut flow = QuickStartFlow::new(existing_projects(&t), Some("m"), "/h");
        ask(&mut flow, "bloom kriegt einen Export");
        assert_eq!(flow.phase, Phase::Thinking);
        assert_eq!(flow.submit(), Submitted::Analysis(Analysis::Done));
        assert_eq!(flow.phase, Phase::Pick);
        assert_eq!(flow.list[0].name, "Bloom");
        flow.model_reply(Ok(r#"{"index": 2, "sure": true, "prompt": "x"}"#.into()));
        assert_eq!(flow.list[0].name, "Bloom", "späte Antwort ändert nichts mehr");
    }

    #[test]
    fn typing_while_thinking_goes_back_and_empty_idea_beeps() {
        let t = TempDir::new("quick");
        let mut flow = QuickStartFlow::new(existing_projects(&t), Some("m"), "/h");
        assert_eq!(flow.submit(), Submitted::Analysis(Analysis::Beep));
        ask(&mut flow, "etwas Neues");
        flow.set_idea("etwas Neues, mehr");
        assert_eq!(flow.phase, Phase::Idea);
    }

    #[test]
    fn missing_folder_reports_instead_of_starting() {
        let t = TempDir::new("quick");
        let mut candidates = existing_projects(&t);
        candidates.push(candidate(&t.sub("weg"), Some("Weg"), Source::Group, 99.0, Some("g")));
        let mut flow = QuickStartFlow::new(candidates, None, &t.path());
        type_idea(&mut flow, "Ein neuer Knopf");
        assert_eq!(flow.list[0].name, "Weg");
        assert_eq!(flow.start(), Started::Missing);
        assert!(flow.error.as_deref().unwrap().ends_with("existiert nicht mehr"));
        assert!(flow.pick(99).is_none());
        assert_eq!(go(flow.pick(1).unwrap()).0, "Kadrell");
    }
}
