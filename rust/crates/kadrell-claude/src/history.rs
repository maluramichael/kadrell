//! Verlaufssuche: frühere Claude-Sessions aus `<configDir>/projects/<slug>/<sessionId>.jsonl` mit Titel und eigenen
//! Prompts (keine Antworten), inkrementell nachgelesen, durchsuchbar (Swift: `Services/SessionHistory.swift`).
//! Jedes Wort der Suche muss im Titel, im Ordner oder in einem Prompt stehen (Groß-/Kleinschreibung und Akzente egal),
//! Treffer im Titel rücken nach vorn, sonst die neueste zuerst. Ohne solchen Treffer gilt die unscharfe Suche, aber nur
//! über Titel und Projekt, sonst passt jeder lange Prompt.

use crate::{paths, text, transcript};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct HistorySession {
    pub session_id: String,
    pub path: String,
    /// Ordner der ersten Eingabe: dort liegt das Transcript, und von dort aus findet `claude --resume` es.
    #[serde(default)]
    pub cwd: String,
    #[serde(default)]
    pub custom_title: Option<String>,
    #[serde(default)]
    pub ai_title: Option<String>,
    #[serde(default)]
    pub prompts: Vec<String>,
    /// Zeitpunkt jeder Eingabe in Sekunden seit 1970, auch wiederholter, für die Statistik. Hier zählt jede Eingabe,
    /// `prompts` fasst gleiche direkt hintereinander zusammen.
    #[serde(default)]
    pub prompt_times: Vec<f64>,
    /// Änderungszeit des Transcripts in Sekunden seit 1970.
    #[serde(default)]
    pub last_active: f64,
    /// Dateigröße beim letzten Lesen: gleich groß heißt unverändert, die Datei wird nicht geöffnet.
    #[serde(default)]
    pub size: u64,
    /// Bis hierhin gelesen, immer hinter einem Zeilenende. Ein gewachsenes Transcript liest der nächste Lauf ab hier.
    #[serde(default)]
    pub parsed: u64,
}

impl HistorySession {
    pub fn new(session_id: &str, path: &str) -> HistorySession {
        HistorySession { session_id: session_id.into(), path: path.into(), ..HistorySession::default() }
    }

    pub fn title(&self) -> String {
        self.own_title()
            .or_else(|| self.ai_title.clone())
            .or_else(|| self.prompts.first().and_then(|p| transcript::flat_prompt(p, 80)))
            .unwrap_or_else(|| self.session_id.clone())
    }

    pub fn project(&self) -> &str {
        paths::last_component(&self.cwd)
    }

    /// Viele Sessions tragen als eigenen Titel nur den Ordnernamen; der sagt nichts, dann gilt Claudes Titel.
    fn own_title(&self) -> Option<String> {
        self.custom_title.clone().filter(|t| t.to_lowercase() != self.project().to_lowercase())
    }

    /// Ohne eigene Eingabe und ohne Titel gibt es nichts zu finden (z. B. abgebrochener Start).
    pub fn is_searchable(&self) -> bool {
        !self.prompts.is_empty() || self.custom_title.is_some() || self.ai_title.is_some()
    }
}

/// Längere Prompts (eingefügte Logs, Dateien) werden gekürzt, die Suche braucht den Anfang.
pub const PROMPT_LIMIT: usize = 2000;
/// Ältere Transcripts haben noch kein `origin`: Nachrichten anderer Sessions erkennt man dort nur am Anfang.
const PEER_PREFIX: &str = "Another Claude session sent a message:";

/// Alle vollständigen Zeilen aus `data` in `entry` übernehmen. Ergebnis: verbrauchte Bytes bis hinter das letzte `\n`,
/// eine angefangene letzte Zeile (Claude schreibt gerade) bleibt für den nächsten Lauf liegen. Nur Zeilen mit einer
/// Eingabe oder einem Titel werden als JSON gelesen; Tool-Ergebnisse (die großen Zeilen) fallen schon an der Bytesuche raus.
pub fn read(data: &[u8], entry: &mut HistorySession) -> usize {
    let mut start = 0;
    while let Some(nl) = data[start..].iter().position(|b| *b == b'\n') {
        line(&data[start..start + nl], entry);
        start += nl + 1;
    }
    start
}

fn line(bytes: &[u8], entry: &mut HistorySession) {
    let has = |needle: &str| text::contains_bytes(bytes, needle.as_bytes());
    if has(r#""type":"user""#) {
        if !has(r#""type":"tool_result""#) {
            user(bytes, entry);
        }
    } else if has(r#""type":"custom-title""#) {
        entry.custom_title = object(bytes).and_then(|o| o.get("customTitle").and_then(Value::as_str).map(String::from)).or(entry.custom_title.take());
    } else if has(r#""type":"ai-title""#) {
        entry.ai_title = object(bytes).and_then(|o| o.get("aiTitle").and_then(Value::as_str).map(String::from)).or(entry.ai_title.take());
    }
}

fn object(bytes: &[u8]) -> Option<Map<String, Value>> {
    match serde_json::from_slice::<Value>(bytes).ok()? {
        Value::Object(o) => Some(o),
        _ => None,
    }
}

fn user(bytes: &[u8], entry: &mut HistorySession) {
    let Some(obj) = object(bytes) else { return };
    if entry.cwd.is_empty()
        && let Some(cwd) = obj.get("cwd").and_then(Value::as_str)
    {
        entry.cwd = cwd.to_string();
    }
    if is_machine_written(&obj) {
        return;
    }
    let Some(t) = transcript::user_text(&obj).filter(|t| !t.starts_with(PEER_PREFIX)) else { return };
    let Some(flat) = transcript::flat_prompt(&t, PROMPT_LIMIT) else { return };
    if let Some(time) = obj.get("timestamp").and_then(Value::as_str).and_then(parse_iso8601) {
        entry.prompt_times.push(time);
    }
    if entry.prompts.last() != Some(&flat) {
        entry.prompts.push(flat);
    }
}

/// Steht als `user` im Transcript, hat aber keiner getippt: Task-Meldungen, Nachrichten anderer Sessions
/// (`promptSource` „system“, `origin.kind` ≠ „human“) und die Zusammenfassung nach dem Verdichten des Kontexts.
fn is_machine_written(obj: &Map<String, Value>) -> bool {
    if obj.get("isCompactSummary").and_then(Value::as_bool) == Some(true) || obj.get("promptSource").and_then(Value::as_str) == Some("system") {
        return true;
    }
    obj.get("origin").and_then(|o| o.get("kind")).and_then(Value::as_str).is_some_and(|k| k != "human")
}

/// `2026-11-02T09:53:59.845Z` (Bruchteil und Versatz `±hh:mm` optional) als Sekunden seit 1970.
pub fn parse_iso8601(s: &str) -> Option<f64> {
    let (date, time) = s.split_once('T')?;
    let mut d = date.split('-');
    let (y, m, day): (i64, i64, i64) = (d.next()?.parse().ok()?, d.next()?.parse().ok()?, d.next()?.parse().ok()?);
    let (clock, offset) = split_offset(time)?;
    let mut parts = clock.split(':');
    let h: f64 = parts.next()?.parse().ok()?;
    let min: f64 = parts.next()?.parse().ok()?;
    let sec: f64 = parts.next()?.parse().ok()?;
    Some(days_from_civil(y, m, day) as f64 * 86400.0 + h * 3600.0 + min * 60.0 + sec - offset)
}

/// Uhrzeit ohne Zonenangabe und deren Versatz in Sekunden.
fn split_offset(time: &str) -> Option<(&str, f64)> {
    if let Some(clock) = time.strip_suffix('Z') {
        return Some((clock, 0.0));
    }
    let i = time.rfind(['+', '-'])?;
    let (clock, off) = time.split_at(i);
    let (h, m) = off[1..].split_once(':')?;
    let secs = h.parse::<f64>().ok()? * 3600.0 + m.parse::<f64>().ok()? * 60.0;
    Some((clock, if off.starts_with('-') { -secs } else { secs }))
}

/// Tage seit 1970-01-01 (Algorithmus von Howard Hinnant).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

// Dateien

#[derive(Clone, Debug, PartialEq)]
pub struct File {
    pub path: String,
    pub size: u64,
    pub mtime: f64,
}

/// `<uuid>.jsonl` mit einer UUID in der üblichen Schreibweise (36 Zeichen mit Bindestrichen).
fn transcript_id(name: &str) -> Option<&str> {
    let id = name.strip_suffix(".jsonl")?;
    let ok = id.len() == 36 && id.bytes().enumerate().all(|(i, b)| if matches!(i, 8 | 13 | 18 | 23) { b == b'-' } else { b.is_ascii_hexdigit() });
    ok.then_some(id)
}

/// Transcripts direkt unter `<root>/projects/<slug>/`, nur `<uuid>.jsonl`. Subagenten liegen eine Ebene tiefer,
/// beiseitegelegte Fassungen (`.orphaned-…`) heißen anders, beides zählt nicht.
pub fn files(root: &str) -> Vec<File> {
    let projects = format!("{root}/projects");
    let Ok(dirs) = std::fs::read_dir(&projects) else { return Vec::new() };
    let mut out = Vec::new();
    for dir in dirs.filter_map(Result::ok) {
        let folder = format!("{projects}/{}", dir.file_name().to_string_lossy());
        let Ok(entries) = std::fs::read_dir(&folder) else { continue };
        for e in entries.filter_map(Result::ok) {
            let name = e.file_name().to_string_lossy().into_owned();
            if transcript_id(&name).is_none() {
                continue;
            }
            let Ok(meta) = e.metadata() else { continue };
            let mtime = meta.modified().ok().and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs_f64()).unwrap_or(0.0);
            out.push(File { path: format!("{folder}/{name}"), size: meta.len(), mtime });
        }
    }
    out
}

/// Neu oder gewachsen: ab `parsed` weiterlesen. Geschrumpft (neu geschrieben): von vorn.
pub fn update(file: &File, old: Option<&HistorySession>) -> HistorySession {
    let id = transcript_id(paths::last_component(&file.path)).unwrap_or_default();
    let mut entry = old.filter(|o| o.parsed <= file.size).cloned().unwrap_or_else(|| HistorySession::new(id, &file.path));
    entry.size = file.size;
    entry.last_active = file.mtime;
    if entry.parsed < file.size
        && let Some(data) = read_from(&file.path, entry.parsed)
    {
        entry.parsed += read(&data, &mut entry) as u64;
    }
    entry
}

fn read_from(path: &str, offset: u64) -> Option<Vec<u8>> {
    let mut f = std::fs::File::open(path).ok()?;
    f.seek(SeekFrom::Start(offset)).ok()?;
    let mut data = Vec::new();
    f.read_to_end(&mut data).ok()?;
    Some(data)
}

/// Ganzer Bestand: unveränderte Einträge bleiben, geänderte und neue werden parallel gelesen, gelöschte fallen weg.
/// `true` im Ergebnis, wenn sich etwas geändert hat.
pub fn refresh(old: &HashMap<String, HistorySession>, root: &str) -> (HashMap<String, HistorySession>, bool) {
    let mut out = HashMap::new();
    let mut todo = Vec::new();
    for f in files(root) {
        match old.get(&f.path).filter(|o| o.size == f.size) {
            Some(o) => {
                out.insert(f.path.clone(), HistorySession { last_active: f.mtime, ..o.clone() });
            }
            None => todo.push(f),
        }
    }
    let changed = !todo.is_empty() || out.len() != old.len();
    let workers = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(2).max(2);
    let chunk = todo.len().div_ceil(workers).max(1);
    std::thread::scope(|scope| {
        let handles: Vec<_> = todo.chunks(chunk).map(|part| scope.spawn(move || part.iter().map(|f| update(f, old.get(&f.path))).collect::<Vec<_>>())).collect();
        for h in handles {
            for e in h.join().unwrap_or_default() {
                out.insert(e.path.clone(), e);
            }
        }
    });
    (out, changed)
}

/// Treffer der Verlaufssuche; `snippet` ist der passende Prompt ab kurz vor der Fundstelle.
#[derive(Clone, Debug, PartialEq)]
pub struct HistoryHit {
    pub session: HistorySession,
    pub snippet: Option<String>,
}

/// Durchsuchbarer Stand, neueste Session zuerst.
pub struct HistoryIndex {
    pub sessions: Vec<HistorySession>,
    /// Gefaltet (ohne Akzente, klein), damit die Suche ohne Umrechnung läuft.
    haystacks: Vec<String>,
    titles: Vec<String>,
    /// Titel und Projekt gefaltet als Zeichen, für die unscharfe Suche.
    labels: Vec<Vec<char>>,
}

impl HistoryIndex {
    pub fn new(entries: Vec<HistorySession>) -> HistoryIndex {
        let mut sessions: Vec<HistorySession> = entries.into_iter().filter(HistorySession::is_searchable).collect();
        sessions.sort_by(|a, b| b.last_active.total_cmp(&a.last_active));
        let titles = sessions.iter().map(|s| text::fold(&s.title())).collect();
        let labels = sessions.iter().map(|s| text::fold(&format!("{} {}", s.title(), s.project())).chars().collect()).collect();
        let haystacks = sessions
            .iter()
            .map(|s| {
                let mut parts = vec![s.title(), s.cwd.clone()];
                parts.extend(s.prompts.iter().cloned());
                text::fold(&parts.join("\n"))
            })
            .collect();
        HistoryIndex { sessions, haystacks, titles, labels }
    }

    pub fn search(&self, query: &str, limit: usize) -> Vec<HistoryHit> {
        let words: Vec<&str> = query.split_whitespace().collect();
        if words.is_empty() {
            return self.sessions.iter().take(limit).map(|s| HistoryHit { session: s.clone(), snippet: None }).collect();
        }
        let needles: Vec<String> = words.iter().map(|w| text::fold(w)).collect();
        let mut ranked: Vec<(usize, usize)> = (0..self.sessions.len())
            .filter(|i| needles.iter().all(|n| self.haystacks[*i].contains(n.as_str())))
            .map(|i| (i, needles.iter().filter(|n| self.titles[i].contains(n.as_str())).count()))
            .collect();
        if ranked.is_empty() {
            return self.fuzzy(query, limit);
        }
        ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        // Längstes Wort, bei Gleichstand das erste, wie `max(by:)` in Swift.
        let probe = words.iter().fold(words[0], |best, w| if w.chars().count() > best.chars().count() { w } else { best });
        ranked.into_iter().take(limit).map(|(i, _)| HistoryHit { session: self.sessions[i].clone(), snippet: snippet(&self.sessions[i], probe) }).collect()
    }

    /// Wie `search` mit dem Standardlimit von 200 Treffern.
    pub fn search_default(&self, query: &str) -> Vec<HistoryHit> {
        self.search(query, 200)
    }

    fn fuzzy(&self, query: &str, limit: usize) -> Vec<HistoryHit> {
        let q: Vec<char> = text::fold(query).chars().filter(|c| !c.is_whitespace()).collect();
        if q.len() < 3 {
            return Vec::new();
        }
        (0..self.sessions.len()).filter(|i| tight(&q, &self.labels[*i])).take(limit).map(|i| HistoryHit { session: self.sessions[i].clone(), snippet: None }).collect()
    }
}

/// Unscharf wie ⌘P (Buchstaben in Reihenfolge), aber eng beieinander: über höchstens die dreifache Länge der Suche
/// verteilt. Sonst passt in langen Titeln fast jede Buchstabenfolge.
pub fn tight(q: &[char], s: &[char]) -> bool {
    s.iter().enumerate().filter(|(_, c)| **c == q[0]).any(|(start, _)| {
        let end = s.len().min(start + 3 * q.len());
        let (mut i, mut j) = (1, start + 1);
        while i < q.len() && j < end {
            if s[j] == q[i] {
                i += 1;
            }
            j += 1;
        }
        i == q.len()
    })
}

/// Erster Prompt mit dem Wort, ab ein paar Zeichen davor; passt es nur im Titel, kein Ausschnitt.
pub fn snippet(s: &HistorySession, word: &str) -> Option<String> {
    let needle = text::fold_chars(word);
    if needle.is_empty() {
        return None;
    }
    for p in &s.prompts {
        let hay = text::fold_chars(p);
        let Some(at) = hay.windows(needle.len()).position(|w| w == needle.as_slice()) else { continue };
        let start = at.saturating_sub(30);
        let rest: String = p.chars().skip(start).take(160).collect();
        return Some(if start > 0 { format!("…{rest}") } else { rest });
    }
    None
}

/// Name der Cache-Datei: aus `~/.claude` abgeleitet, deshalb in den Caches und für alle Profile gemeinsam, je
/// Claude-Datenordner eine Datei. `/`, `\` und `:` fallen aus dem Namen (Windows verbietet den Doppelpunkt).
pub fn cache_path(cache_dir: &str, config_dir: &str) -> String {
    let name: String = config_dir.chars().map(|c| if matches!(c, '/' | '\\' | ':') { '_' } else { c }).collect();
    format!("{cache_dir}/de.malura.kadrell.rust/session-history-v3{name}.json")
}

pub fn load_cache(path: &str) -> HashMap<String, HistorySession> {
    let list: Vec<HistorySession> = std::fs::read(path).ok().and_then(|d| serde_json::from_slice(&d).ok()).unwrap_or_default();
    list.into_iter().map(|s| (s.path.clone(), s)).collect()
}

pub fn save_cache(entries: &HashMap<String, HistorySession>, path: &str) {
    let list: Vec<&HistorySession> = entries.values().collect();
    let Ok(data) = serde_json::to_vec(&list) else { return };
    if let Some(dir) = std::path::Path::new(path).parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let tmp = format!("{path}.tmp");
    if std::fs::write(&tmp, data).is_ok() {
        let _ = std::fs::rename(&tmp, path);
    }
}

/// Index aller Transcripts für die Verlaufssuche. Beim Start und bei jedem Öffnen der Suche werden nur neue und
/// gewachsene Transcripts nachgelesen (`refresh`, auf einem Hintergrund-Thread), die Suche selbst geht nur gegen den Speicher.
#[derive(Default)]
pub struct SessionHistory {
    /// nil bis der erste Lauf fertig ist.
    pub index: Option<HistoryIndex>,
    entries: HashMap<String, HistorySession>,
    running: bool,
    waiters: Vec<Waiter>,
}

/// Wird nach einem Lauf mit `changed` aufgerufen (true = der Index ist neu).
pub type Waiter = Box<dyn FnOnce(bool) + Send>;

/// Der schwere Teil eines Laufs (Dateien lesen, Cache schreiben, Index bauen), für einen Hintergrund-Thread.
pub struct RefreshJob {
    base: HashMap<String, HistorySession>,
    config_dir: String,
    cache_file: String,
    first: bool,
}

/// Ergebnis von [`RefreshJob::run`], zurück an [`SessionHistory::finish`].
pub struct RefreshDone {
    fresh: HashMap<String, HistorySession>,
    next: Option<HistoryIndex>,
    changed: bool,
}

impl RefreshJob {
    pub fn run(self) -> RefreshDone {
        let (fresh, changed) = refresh(&self.base, &self.config_dir);
        if changed {
            save_cache(&fresh, &self.cache_file);
        }
        let next = (changed || self.first).then(|| HistoryIndex::new(fresh.values().cloned().collect()));
        RefreshDone { fresh, next, changed: changed || self.first }
    }
}

impl SessionHistory {
    /// Nachlesen anfordern (Swift: `refresh(configDir:then:)`). Läuft schon ein Lauf, wird die Anforderung eingereiht
    /// (`then` kommt nach dem laufenden Lauf mit dessen Ergebnis) und es gibt keinen neuen Job. Sonst kommt der Job
    /// zurück, den der Aufrufer auf einem Hintergrund-Thread ausführt und mit [`finish`](Self::finish) abschließt.
    /// Der erste Lauf startet vom Cache auf der Platte.
    pub fn request(&mut self, config_dir: &str, cache_file: &str, then: Option<Waiter>) -> Option<RefreshJob> {
        self.waiters.extend(then);
        if self.running {
            return None;
        }
        self.running = true;
        let first = self.index.is_none();
        let base = if first { load_cache(cache_file) } else { self.entries.clone() };
        Some(RefreshJob { base, config_dir: config_dir.to_string(), cache_file: cache_file.to_string(), first })
    }

    /// Lauf übernehmen und alle wartenden Aufrufer mit `changed` benachrichtigen.
    pub fn finish(&mut self, done: RefreshDone) {
        self.entries = done.fresh;
        if let Some(next) = done.next {
            self.index = Some(next);
        }
        self.running = false;
        for waiter in std::mem::take(&mut self.waiters) {
            waiter(done.changed);
        }
    }

    /// Nachlesen und Index neu bauen, auf dem aufrufenden Thread. true = der Index ist neu. Läuft schon ein Lauf
    /// (Anforderung eingereiht), kehrt der Aufruf mit false zurück.
    pub fn refresh(&mut self, config_dir: &str, cache_file: &str) -> bool {
        let Some(job) = self.request(config_dir, cache_file, None) else { return false };
        let done = job.run();
        let changed = done.changed;
        self.finish(done);
        changed
    }

    /// Alle Sessions mit Eingaben oder Titel, neueste zuerst (für die Statistik).
    pub fn sessions(&self) -> &[HistorySession] {
        self.index.as_ref().map(|i| i.sessions.as_slice()).unwrap_or(&[])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::testdir::TempDir;
    use std::io::Write;

    const ID: &str = "2bf3b100-502a-44fa-aecf-499333e74ad8";

    fn user_line(text: &str, cwd: &str, extra: &str) -> String {
        format!(r#"{{"type":"user","cwd":"{cwd}"{extra},"message":{{"role":"user","content":"{text}"}}}}"#)
    }

    fn user(text: &str) -> String {
        user_line(text, "/p", "")
    }

    fn parse(lines: &[String]) -> HistorySession {
        let mut e = HistorySession::new(ID, "/x");
        read((lines.join("\n") + "\n").as_bytes(), &mut e);
        e
    }

    #[test]
    fn reads_prompts_cwd_and_latest_title() {
        let e = parse(&[
            user_line("Suche mit Qwen und Ollama", "/a", ""),
            user_line("weiter", "/b", ""),
            r#"{"type":"ai-title","aiTitle":"Alt"}"#.into(),
            r#"{"type":"ai-title","aiTitle":"Lokale Modelle"}"#.into(),
        ]);
        assert_eq!(e.prompts, ["Suche mit Qwen und Ollama", "weiter"]);
        assert_eq!(e.cwd, "/a", "Ordner der ersten Eingabe, dort liegt das Transcript");
        assert_eq!(e.title(), "Lokale Modelle");
    }

    /// Die Statistik zählt jede Eingabe mit ihrem Zeitpunkt, auch wiederholte; Zeilen ohne Eingabe zählen nicht.
    #[test]
    fn records_time_of_every_prompt() {
        let at = r#","timestamp":"2026-11-02T09:53:59.845Z""#;
        let e = parse(&[user_line("weiter", "/p", at), user_line("weiter", "/p", at), user_line("<command-name>/x", "/p", at), user("ohne Zeit")]);
        assert_eq!(e.prompts, ["weiter", "ohne Zeit"]);
        assert_eq!(e.prompt_times.len(), 2);
        assert!((e.prompt_times[0] - 1_793_613_239.845).abs() < 0.01);
    }

    #[test]
    fn iso8601_variants() {
        assert_eq!(parse_iso8601("1970-01-01T00:00:00Z"), Some(0.0));
        assert_eq!(parse_iso8601("2026-11-02T10:53:59.845+01:00").map(|t| (t * 1000.0).round()), Some(1_793_613_239_845.0));
        assert_eq!(parse_iso8601("kein datum"), None);
    }

    #[test]
    fn custom_title_wins_over_ai_title() {
        let e = parse(&[r#"{"type":"custom-title","customTitle":"Mein Name"}"#.into(), r#"{"type":"ai-title","aiTitle":"KI"}"#.into(), user("x")]);
        assert_eq!(e.title(), "Mein Name");
    }

    #[test]
    fn skips_answers_tool_results_and_machine_messages() {
        let e = parse(&[
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Antwort"}]}}"#.into(),
            r#"{"type":"user","message":{"content":[{"type":"tool_result","content":"Ausgabe"}]}}"#.into(),
            user_line("Zusammenfassung", "/p", r#","isCompactSummary":true"#),
            user_line("Task fertig", "/p", r#","promptSource":"system""#),
            user_line("Peer", "/p", r#","origin":{"kind":"peer"}"#),
            user("Another Claude session sent a message: hallo"),
            user_line("echt", "/p", r#","origin":{"kind":"human"},"promptSource":"typed""#),
        ]);
        assert_eq!(e.prompts, ["echt"]);
    }

    /// Claude schreibt gerade: die angefangene letzte Zeile bleibt liegen und wird beim nächsten Lauf gelesen.
    #[test]
    fn leaves_partial_last_line() {
        let mut e = HistorySession::new(ID, "/x");
        let full = user("eins") + "\n";
        let used = read((full.clone() + &user("zw")).as_bytes(), &mut e);
        assert_eq!(used, full.len());
        assert_eq!(e.prompts, ["eins"]);
    }

    fn write_transcript(dir: &TempDir, lines: &[String], name: Option<&str>) -> String {
        dir.write(&format!("projects/-p/{}.jsonl", name.unwrap_or(ID)), &(lines.join("\n") + "\n"))
    }

    #[test]
    fn refresh_reads_only_grown_part_and_drops_deleted() {
        let dir = TempDir::new("history");
        let path = write_transcript(&dir, &[user("eins")], None);
        write_transcript(&dir, &[user("weg")], Some("11111111-1111-1111-1111-111111111111"));
        write_transcript(&dir, &[user("nie")], Some(&format!("{ID}.orphaned-1-x")));
        let (entries, changed) = refresh(&HashMap::new(), &dir.path());
        assert!(changed);
        assert_eq!(entries.len(), 2, "beiseitegelegte Fassungen zählen nicht");
        assert_eq!(entries[&path].prompts, ["eins"]);

        let mut f = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
        f.write_all((user("zwei") + "\n").as_bytes()).unwrap();
        drop(f);
        std::fs::remove_file(dir.sub("projects/-p/11111111-1111-1111-1111-111111111111.jsonl")).unwrap();
        let (entries, changed) = refresh(&entries, &dir.path());
        assert!(changed);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[&path].prompts, ["eins", "zwei"]);

        let (_, changed) = refresh(&entries, &dir.path());
        assert!(!changed, "unverändert: nichts neu gelesen");
    }

    fn session(title: &str, prompts: &[&str], at: f64) -> HistorySession {
        HistorySession { prompts: prompts.iter().map(|p| p.to_string()).collect(), last_active: at, ai_title: Some(title.into()), cwd: "/p/proj".into(), ..HistorySession::new(&format!("id-{at}"), &format!("/{at}")) }
    }

    fn titles(hits: &[HistoryHit]) -> Vec<String> {
        hits.iter().map(|h| h.session.title()).collect()
    }

    #[test]
    fn search_needs_every_word_and_ignores_case_and_accents() {
        let index = HistoryIndex::new(vec![session("Benchmark", &["lokale Suche mit Qwen und Ollama"], 1.0), session("Logo", &["Qwen Image für Logos"], 2.0)]);
        assert_eq!(titles(&index.search_default("qwen OLLAMA")), ["Benchmark"]);
        assert_eq!(titles(&index.search_default("fur logos")), ["Logo"]);
    }

    #[test]
    fn title_hits_first_then_newest() {
        let index = HistoryIndex::new(vec![session("Alt", &["ollama"], 1.0), session("Neu", &["ollama"], 3.0), session("Ollama Setup", &["x"], 2.0)]);
        assert_eq!(titles(&index.search_default("ollama")), ["Ollama Setup", "Neu", "Alt"]);
    }

    #[test]
    fn empty_query_lists_newest_first_and_snippet_starts_near_hit() {
        let long = format!("{} Minimodelle lokal", "a".repeat(100));
        let index = HistoryIndex::new(vec![session("A", &[&long], 1.0), session("B", &["x"], 2.0)]);
        assert_eq!(titles(&index.search_default("")), ["B", "A"]);
        let snippet = index.search_default("minimodelle")[0].snippet.clone().unwrap_or_default();
        assert!(snippet.starts_with('…'));
        assert!(snippet.contains("Minimodelle lokal"));
    }

    #[test]
    fn fuzzy_fallback_on_title_only() {
        let index = HistoryIndex::new(vec![session("Kadrell Statusleiste", &["x"], 1.0)]);
        assert_eq!(index.search_default("kdrstat").len(), 1);
        assert!(index.search_default("zzzq").is_empty());
    }

    /// Ein eigener Titel, der nur der Ordnername ist, verdrängt Claudes Titel nicht.
    #[test]
    fn folder_name_as_custom_title_falls_back_to_ai_title() {
        let mut s = HistorySession { cwd: "/p/kadrell".into(), ..HistorySession::new("x", "/x") };
        s.custom_title = Some("kadrell".into());
        s.ai_title = Some("Verlaufssuche bauen".into());
        assert_eq!(s.title(), "Verlaufssuche bauen");
        s.custom_title = Some("Mein Name".into());
        assert_eq!(s.title(), "Mein Name");
    }

    #[test]
    fn fuzzy_needs_letters_close_together() {
        let c = |s: &str| s.chars().collect::<Vec<_>>();
        assert!(tight(&c("kdrst"), &c("kadrell status")));
        assert!(!tight(&c("kdrst"), &c("kai der große rest von irgendwas statt")));
    }

    #[test]
    fn snippet_positions_survive_accents() {
        let s = HistorySession { prompts: vec![format!("{}Äpfel und Birnen", "é".repeat(40))], ..HistorySession::new("x", "/x") };
        let sn = snippet(&s, "apfel").unwrap();
        assert!(sn.starts_with('…') && sn.contains("Äpfel und Birnen"));
    }

    #[test]
    fn cache_name_has_no_path_separators_or_colons() {
        let p = cache_path("/c", "C:\\Users\\me\\.claude");
        assert_eq!(p, "/c/de.malura.kadrell.rust/session-history-v3C__Users_me_.claude.json");
    }

    #[test]
    fn session_history_reads_cache_then_increments() {
        let dir = TempDir::new("histcache");
        let cache = dir.sub("cache/h.json");
        write_transcript(&dir, &[user("Suche etwas"), r#"{"type":"ai-title","aiTitle":"Titel"}"#.into()], None);
        let mut h = SessionHistory::default();
        assert!(h.refresh(&dir.path(), &cache));
        assert_eq!(h.sessions().len(), 1);
        assert!(!h.refresh(&dir.path(), &cache), "nichts geändert");
        let mut fresh = SessionHistory::default();
        assert!(fresh.refresh(&dir.path(), &cache), "erster Lauf baut den Index immer");
        assert_eq!(fresh.sessions()[0].title(), "Titel");
        assert_eq!(fresh.index.unwrap().search_default("suche")[0].session.title(), "Titel");
    }

    /// Eine Anforderung während eines Laufs wird eingereiht und bekommt das Ergebnis dieses Laufs.
    #[test]
    fn refresh_requests_during_a_run_are_queued_and_notified() {
        use std::sync::{Arc, Mutex};
        let dir = TempDir::new("histqueue");
        let cache = dir.sub("cache/h.json");
        write_transcript(&dir, &[user("Suche etwas")], None);
        let calls: Arc<Mutex<Vec<(&'static str, bool)>>> = Arc::default();
        let waiter = |name: &'static str| -> Option<Waiter> {
            let calls = calls.clone();
            Some(Box::new(move |changed| calls.lock().unwrap().push((name, changed))))
        };
        let mut h = SessionHistory::default();
        let job = h.request(&dir.path(), &cache, waiter("a")).expect("erster Aufruf startet den Lauf");
        assert!(h.request(&dir.path(), &cache, waiter("b")).is_none(), "zweiter Aufruf wartet");
        assert!(!h.refresh(&dir.path(), &cache), "ohne Waiter ebenfalls eingereiht");
        assert!(calls.lock().unwrap().is_empty());
        h.finish(job.run());
        assert_eq!(*calls.lock().unwrap(), [("a", true), ("b", true)]);
        assert_eq!(h.sessions().len(), 1);
        let again = h.request(&dir.path(), &cache, waiter("c")).expect("danach wieder frei");
        h.finish(again.run());
        assert_eq!(calls.lock().unwrap().last(), Some(&("c", false)), "nichts geändert");
    }
}
