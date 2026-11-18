//! Letzte Textantwort von Claude, erste Eingabe und Tool-Pfade aus dem Transcript
//! `<configDir>/projects/<slug>/<sessionId>.jsonl` (Swift: `Services/Transcript.swift`).

use crate::{paths, text};
use serde_json::Value;
use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};

#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub path: String,
    pub size: u64,
    pub text: Option<String>,
    /// Pfade und Bash-Kommandos aus `tool_use`-Aufrufen im gelesenen Tail, neueste zuerst. Für die Worktree-Erkennung.
    pub tool_candidates: Vec<String>,
}

/// Über alle Projektordner gesucht statt aus `cwd` abgeleitet: nach einem Worktree-Wechsel stimmt der Slug nicht mehr.
pub fn path(session_id: &str, config_dir: &str) -> Option<String> {
    if session_id.is_empty() {
        return None;
    }
    let root = format!("{config_dir}/projects");
    std::fs::read_dir(&root)
        .ok()?
        .filter_map(Result::ok)
        .map(|d| format!("{root}/{}/{session_id}.jsonl", d.file_name().to_string_lossy()))
        .find(|p| std::path::Path::new(p).is_file())
}

/// Wie lange ein erfolgloses Suchen nach einem Transcript gilt, bevor wieder gesucht wird.
pub const NOT_FOUND_BACKOFF_MS: u64 = 30_000;

/// Zustand des Nachlesens: wo es nichts zu finden gab (statt bei jedem Poll den ganzen Ordner zu scannen) und wie
/// viele Eingaben des Nutzers seit dem letzten `take_new_messages` dazugekommen sind (für die Statistik).
#[derive(Default)]
pub struct TranscriptReader {
    not_found_until: HashMap<String, u64>,
    new_messages: usize,
}

impl TranscriptReader {
    /// Zählt nur Zuwachs seit dem letzten Lesen: ein Transcript zum ersten Mal zu lesen rechnet den alten Verlauf nicht ein.
    pub fn take_new_messages(&mut self) -> usize {
        std::mem::take(&mut self.new_messages)
    }

    /// Liest nur Transcripts neu, deren Größe sich geändert hat, gewachsene nur ab der alten Größe statt komplett.
    /// `want_text`: `last_text` kostet eine eigene JSON-Passage, nötig nur wenn die Nachrichtenzeile an ist.
    pub fn refresh(&mut self, ids: &[String], config_dir: &str, cache: &HashMap<String, Entry>, want_text: bool, now_ms: u64) -> HashMap<String, Entry> {
        let mut out = HashMap::new();
        for id in ids {
            let Some(path) = self.resolve(id, config_dir, cache, now_ms) else { continue };
            let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            if let Some(old) = cache.get(id) {
                if old.size == size {
                    out.insert(id.clone(), old.clone());
                    continue;
                }
                if size > old.size {
                    out.insert(id.clone(), self.grow(&path, size, old, want_text));
                    continue;
                }
            }
            // Erstes Lesen oder geschrumpft (z. B. `/clear`, neue Datei unter altem Pfad): voller Tail.
            out.insert(id.clone(), read_full(&path, size, want_text));
        }
        out
    }

    fn resolve(&mut self, id: &str, config_dir: &str, cache: &HashMap<String, Entry>, now_ms: u64) -> Option<String> {
        if let Some(e) = cache.get(id) {
            return Some(e.path.clone());
        }
        if self.not_found_until.get(id).is_some_and(|until| *until > now_ms) {
            return None;
        }
        match path(id, config_dir) {
            Some(p) => {
                self.not_found_until.remove(id);
                Some(p)
            }
            None => {
                self.not_found_until.insert(id.to_string(), now_ms + NOT_FOUND_BACKOFF_MS);
                None
            }
        }
    }

    /// Nur die seit `old.size` neu geschriebenen Bytes lesen und parsen; wo nichts Neueres trifft, gilt der alte Wert.
    /// Lag die letzte Größe mitten in einer Zeile, geht diese eine Nachricht verloren (wie in Swift).
    fn grow(&mut self, path: &str, size: u64, old: &Entry, want_text: bool) -> Entry {
        let Some(data) = read_from(path, old.size) else { return read_full(path, size, want_text) };
        self.new_messages += user_messages(&data);
        let new_candidates = tool_candidates(&data, 20);
        let new_text = if want_text { last_text(&data) } else { None };
        Entry {
            path: path.to_string(),
            size,
            text: new_text.or_else(|| old.text.clone()),
            tool_candidates: if new_candidates.is_empty() {
                old.tool_candidates.clone()
            } else {
                new_candidates.into_iter().chain(old.tool_candidates.iter().cloned()).take(20).collect()
            },
        }
    }
}

fn read_full(path: &str, size: u64, want_text: bool) -> Entry {
    let data = tail_data(path, 1 << 20);
    Entry {
        path: path.to_string(),
        size,
        text: if want_text { data.as_deref().and_then(last_text) } else { None },
        tool_candidates: data.as_deref().map(|d| tool_candidates(d, 20)).unwrap_or_default(),
    }
}

fn read_from(path: &str, offset: u64) -> Option<Vec<u8>> {
    let mut f = std::fs::File::open(path).ok()?;
    f.seek(SeekFrom::Start(offset)).ok()?;
    let mut data = Vec::new();
    f.read_to_end(&mut data).ok()?;
    Some(data)
}

/// Nur das letzte MB, reicht solange keine Tool-Ausgabe allein größer ist.
fn tail_data(path: &str, tail: u64) -> Option<Vec<u8>> {
    let size = std::fs::metadata(path).ok()?.len();
    read_from(path, size.saturating_sub(tail))
}

fn lines(data: &[u8]) -> impl DoubleEndedIterator<Item = &[u8]> {
    data.split(|b| *b == b'\n').filter(|l| !l.is_empty())
}

fn parse_object(line: &[u8]) -> Option<serde_json::Map<String, Value>> {
    match serde_json::from_slice::<Value>(line).ok()? {
        Value::Object(o) => Some(o),
        _ => None,
    }
}

/// Text einer echten Eingabe des Nutzers, sonst nil. Tool-Ergebnisse kommen ebenfalls als `user`-Zeile, zählen aber
/// nicht, ebenso wenig Meta-Zeilen, Sidechains (Subagenten) und die Ausgabe von Slash-Commands (`<command-name>` …).
/// Eine angeschnittene Zeile ist kein JSON und fällt damit von selbst raus.
pub fn user_text_line(line: &[u8]) -> Option<String> {
    user_text(&parse_object(line)?)
}

/// Wie `user_text_line`, für eine schon geparste Zeile (die Verlaufssuche braucht daraus auch `cwd`).
pub fn user_text(obj: &serde_json::Map<String, Value>) -> Option<String> {
    let flag = |k: &str| obj.get(k).and_then(Value::as_bool) == Some(true);
    if obj.get("type").and_then(Value::as_str) != Some("user") || flag("isMeta") || flag("isSidechain") {
        return None;
    }
    let content = obj.get("message")?.get("content")?;
    let joined = match content {
        Value::String(s) => s.clone(),
        Value::Array(items) => items
            .iter()
            .filter(|i| i.get("type").and_then(Value::as_str) == Some("text"))
            .filter_map(|i| i.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join(" "),
        _ => String::new(),
    };
    let flat = joined.trim();
    if flat.is_empty() || flat.starts_with('<') { None } else { Some(flat.to_string()) }
}

/// Zählt die Eingaben des Nutzers im gelesenen Ausschnitt (für die Statistik).
pub fn user_messages(jsonl: &[u8]) -> usize {
    lines(jsonl).filter(|l| user_text_line(l).is_some()).count()
}

/// Erste echte Eingabe des Nutzers, gekürzt. Eine Nachricht aus lauter Bildern hat keinen Titel und zählt hier nicht.
pub fn first_prompt(path: &str, head: usize, limit: usize) -> Option<String> {
    let mut data = Vec::new();
    std::fs::File::open(path).ok()?.take(head as u64).read_to_end(&mut data).ok()?;
    lines(&data).filter_map(user_text_line).find_map(|t| flat_prompt(&t, limit))
}

/// Standardwerte von `firstPrompt(path:)`: 256 KB Kopf, 50 Zeichen.
pub fn first_prompt_default(path: &str) -> Option<String> {
    first_prompt(path, 1 << 18, 50)
}

/// Bildmarker `[Image #n]` entfernen.
fn strip_image_markers(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find("[Image #") {
        out.push_str(&rest[..i]);
        let after = &rest[i + "[Image #".len()..];
        let digits = after.bytes().take_while(u8::is_ascii_digit).count();
        if digits > 0 && after[digits..].starts_with(']') {
            rest = &after[digits + 1..];
        } else {
            out.push_str("[Image #");
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

/// Eine Nutzereingabe als Ersatztitel: Bildmarker raus, eine Zeile, gekürzt; Slash-Command-Ausgaben (`<…`) sind keiner.
pub fn flat_prompt(input: &str, limit: usize) -> Option<String> {
    let flat = text::squash_whitespace(&strip_image_markers(input));
    if flat.is_empty() || flat.starts_with('<') {
        return None;
    }
    if flat.chars().count() > limit {
        let cut: String = flat.chars().take(limit).collect();
        return Some(format!("{}…", cut.trim_matches([' ', '\t'])));
    }
    Some(flat)
}

/// Eine Antwort als Baumzeile: Markdown-Zeichen raus, eine Zeile.
pub fn flat_answer(input: &str) -> Option<String> {
    let cleaned: String = input.chars().filter(|c| !matches!(c, '`' | '*' | '#')).collect();
    let flat = text::squash_whitespace(&cleaned);
    if flat.is_empty() { None } else { Some(flat) }
}

fn assistant_content(line: &[u8]) -> Option<Vec<Value>> {
    let obj = parse_object(line)?;
    if obj.get("type").and_then(Value::as_str) != Some("assistant") || obj.get("isSidechain").and_then(Value::as_bool) == Some(true) {
        return None;
    }
    obj.get("message")?.get("content")?.as_array().cloned()
}

/// Von hinten die erste Assistant-Zeile mit Text. Eine angeschnittene erste Zeile ist kein JSON und fällt raus.
pub fn last_text(jsonl: &[u8]) -> Option<String> {
    lines(jsonl).rev().filter_map(assistant_content).find_map(|content| {
        let last = content
            .iter()
            .filter(|i| i.get("type").and_then(Value::as_str) == Some("text"))
            .filter_map(|i| i.get("text").and_then(Value::as_str))
            .next_back()
            .unwrap_or("");
        flat_answer(last)
    })
}

/// Pfade (`file_path`/`path`/`notebook_path`) und Bash-Kommandos aus `tool_use`-Aufrufen, neueste zuerst (jüngste Zeile
/// zuerst, innerhalb einer Zeile der letzte Aufruf zuerst). Sidechains (Subagenten) zählen nicht.
pub fn tool_candidates(jsonl: &[u8], limit: usize) -> Vec<String> {
    let mut out = Vec::new();
    for content in lines(jsonl).rev().filter_map(assistant_content) {
        for item in content.iter().rev() {
            let Some(input) = item.get("input").filter(|_| item.get("type").and_then(Value::as_str) == Some("tool_use")) else { continue };
            for key in ["file_path", "path", "notebook_path"] {
                if let Some(p) = input.get(key).and_then(Value::as_str).filter(|p| paths::is_abs(p)) {
                    out.push(p.to_string());
                }
            }
            if let Some(c) = input.get("command").and_then(Value::as_str) {
                out.push(c.to_string());
            }
            if out.len() >= limit {
                return out;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::testdir::TempDir;
    use std::io::Write;

    fn data(lines: &[&str]) -> Vec<u8> {
        lines.join("\n").into_bytes()
    }

    // TranscriptCountTests

    #[test]
    fn counts_plain_text_message() {
        assert_eq!(user_messages(&data(&[r#"{"type":"user","message":{"content":"Hallo"}}"#])), 1);
    }

    #[test]
    fn counts_text_block_message() {
        assert_eq!(user_messages(&data(&[r#"{"type":"user","message":{"content":[{"type":"text","text":"Bau das"}]}}"#])), 1);
    }

    #[test]
    fn ignores_assistant_lines() {
        assert_eq!(user_messages(&data(&[r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Fertig"}]}}"#])), 0);
    }

    /// Tool-Ergebnisse kommen als `type: user` zurück, sind aber keine Eingabe.
    #[test]
    fn ignores_tool_results() {
        assert_eq!(user_messages(&data(&[r#"{"type":"user","message":{"content":[{"type":"tool_result","content":"ok"}]}}"#])), 0);
    }

    #[test]
    fn ignores_meta_and_sidechain() {
        let meta = r#"{"type":"user","isMeta":true,"message":{"content":"Caveat"}}"#;
        let side = r#"{"type":"user","isSidechain":true,"message":{"content":"Subagent"}}"#;
        assert_eq!(user_messages(&data(&[meta, side])), 0);
    }

    #[test]
    fn ignores_command_output() {
        assert_eq!(user_messages(&data(&[r#"{"type":"user","message":{"content":"<command-name>/clear</command-name>"}}"#])), 0);
    }

    /// Ein Delta kann mitten in einer Zeile beginnen: der Rest ist kein JSON und darf nicht mitzählen.
    #[test]
    fn ignores_truncated_first_line() {
        let broken = r#"e":"user","message":{"content":"halb"}}"#;
        let good = r#"{"type":"user","message":{"content":"ganz"}}"#;
        assert_eq!(user_messages(&data(&[broken, good])), 1);
    }

    #[test]
    fn counts_several_messages() {
        let lines = [
            r#"{"type":"user","message":{"content":"eins"}}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"ok"}]}}"#,
            r#"{"type":"user","message":{"content":[{"type":"tool_result","content":"x"}]}}"#,
            r#"{"type":"user","message":{"content":[{"type":"text","text":"zwei"}]}}"#,
        ];
        assert_eq!(user_messages(&data(&lines)), 2);
        assert_eq!(user_messages(&[]), 0);
    }

    // TranscriptTests (SessionParsingTests)

    #[test]
    fn path_follows_config_dir() {
        let dir = TempDir::new("transcript");
        dir.write("projects/-p-proj/abc.jsonl", "{}");
        assert_eq!(path("abc", &dir.path()), Some(format!("{}/projects/-p-proj/abc.jsonl", dir.path())));
        assert_eq!(path("fehlt", &dir.path()), None);
        assert_eq!(path("", &dir.path()), None);
    }

    /// Angeschnittene erste Zeile, Tool-Aufruf ohne Text und Sidechain-Antwort werden übersprungen.
    #[test]
    fn last_text_skips_cut_tool_and_sidechain() {
        let jsonl = [
            r#","cut":"off"}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Fix ist drin.\n\nSoll ich **pushen**?"}]}}"#,
            r#"{"type":"user","message":{"content":"ja"}}"#,
            r#"{"type":"assistant","isSidechain":true,"message":{"content":[{"type":"text","text":"subagent"}]}}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Bash"}]}}"#,
        ];
        assert_eq!(last_text(&data(&jsonl)).as_deref(), Some("Fix ist drin. Soll ich pushen?"));
        assert_eq!(last_text(br#"{"type":"user"}"#), None);
    }

    /// Ersatztitel: Meta-Zeilen, Slash-Commands und Bild-Platzhalter fallen raus, lange Texte werden gekürzt.
    #[test]
    fn first_prompt_skips_meta_commands_and_images() {
        let jsonl = [
            r#"{"type":"user","isMeta":true,"message":{"content":"<local-command-caveat>x</local-command-caveat>"}}"#,
            r#"{"type":"user","message":{"content":"<command-name>/model</command-name>"}}"#,
            r#"{"type":"user","message":{"content":[{"type":"text","text":"[Image #1]  Umbenennen\ngeht nicht immer, bitte prüfen und dann reparieren"},{"type":"image"}]}}"#,
            r#"{"type":"user","message":{"content":"zweite"}}"#,
        ];
        let dir = TempDir::new("firstprompt");
        let file = dir.write("t.jsonl", &jsonl.join("\n"));
        assert_eq!(first_prompt(&file, 1 << 18, 29).as_deref(), Some("Umbenennen geht nicht immer,…"));
        assert_eq!(first_prompt_default(&file).as_deref(), Some("Umbenennen geht nicht immer, bitte prüfen und dann…"));
        assert_eq!(first_prompt_default("/gibt/es/nicht.jsonl"), None);
    }

    /// Worktree-Erkennung: Pfad-Felder und Bash-Kommandos aus `tool_use`, neueste zuerst, Sidechain fällt raus.
    #[test]
    fn tool_candidates_newest_first() {
        let jsonl = [
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Bash","input":{"command":"ls"}}]}}"#,
            r#"{"type":"assistant","isSidechain":true,"message":{"content":[{"type":"tool_use","name":"Edit","input":{"file_path":"/sub/x.php"}}]}}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Read","input":{"file_path":"relative.txt"}},{"type":"tool_use","name":"Edit","input":{"file_path":"/repo-wt-1/src/x.php"}}]}}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Bash","input":{"command":"cd /repo-wt-1 && npm test"}}]}}"#,
        ];
        assert_eq!(tool_candidates(&data(&jsonl), 20), ["cd /repo-wt-1 && npm test", "/repo-wt-1/src/x.php", "ls"]);
    }

    fn append(path: &str, line: &str) {
        let mut f = std::fs::OpenOptions::new().append(true).open(path).unwrap();
        f.write_all(format!("{line}\n").as_bytes()).unwrap();
    }

    fn size(path: &str) -> u64 {
        std::fs::metadata(path).unwrap().len()
    }

    /// `refresh` liest bei gewachsener Datei nur die neuen Bytes: neuer Text/Tool-Aufruf setzt sich durch, wächst die
    /// Datei aber ohne neue Assistant-Zeile, bleiben Text und Kandidaten des Caches stehen statt zu verschwinden.
    #[test]
    fn refresh_reads_only_growth_and_keeps_old_where_nothing_new() {
        let dir = TempDir::new("refresh");
        let path = dir.write("t.jsonl", &(r#"{"type":"assistant","message":{"content":[{"type":"text","text":"erste Antwort"}]}}"#.to_string() + "\n"));
        let entry = Entry { path: path.clone(), size: size(&path), text: Some("erste Antwort".into()), tool_candidates: vec!["ls".into()] };
        let cache = HashMap::from([("s1".to_string(), entry)]);
        let mut reader = TranscriptReader::default();
        let ids = ["s1".to_string()];

        append(&path, r#"{"type":"assistant","message":{"content":[{"type":"text","text":"zweite Antwort"},{"type":"tool_use","name":"Bash","input":{"command":"pwd"}}]}}"#);
        let grown = reader.refresh(&ids, "/nonexistent", &cache, true, 0);
        assert_eq!(grown["s1"].text.as_deref(), Some("zweite Antwort"));
        assert_eq!(grown["s1"].tool_candidates, ["pwd", "ls"]);
        assert_eq!(grown["s1"].size, size(&path));

        append(&path, r#"{"type":"user","message":{"content":"ok"}}"#);
        let no_new = reader.refresh(&ids, "/nonexistent", &grown, true, 0);
        assert_eq!(no_new["s1"].text.as_deref(), Some("zweite Antwort"));
        assert_eq!(no_new["s1"].tool_candidates, ["pwd", "ls"]);
        assert_eq!(no_new["s1"].size, size(&path));
        assert_eq!(reader.take_new_messages(), 1, "nur der Zuwachs zählt");

        // Unveränderte Größe: der Cache-Eintrag geht unverändert durch, ohne die Datei erneut zu lesen.
        let unchanged = reader.refresh(&ids, "/nonexistent", &no_new, true, 0);
        assert_eq!(unchanged["s1"].text.as_deref(), Some("zweite Antwort"));

        // `want_text: false`: kein Text, Tool-Kandidaten bleiben für die Worktree-Erkennung.
        let fresh = HashMap::from([("s1".to_string(), Entry { path: path.clone(), size: 0, text: None, tool_candidates: vec![] })]);
        let no_text = reader.refresh(&ids, "/nonexistent", &fresh, false, 0);
        assert_eq!(no_text["s1"].text, None);
        assert_eq!(no_text["s1"].tool_candidates, ["pwd"]);
    }

    #[test]
    fn missing_transcript_is_not_searched_again_for_30_seconds() {
        let dir = TempDir::new("notfound");
        let mut reader = TranscriptReader::default();
        let ids = ["neu".to_string()];
        assert!(reader.refresh(&ids, &dir.path(), &HashMap::new(), true, 1000).is_empty());
        dir.write("projects/-p/neu.jsonl", r#"{"type":"user","message":{"content":"x"}}"#);
        assert!(reader.refresh(&ids, &dir.path(), &HashMap::new(), true, 1000 + NOT_FOUND_BACKOFF_MS - 1).is_empty());
        assert_eq!(reader.refresh(&ids, &dir.path(), &HashMap::new(), true, 1000 + NOT_FOUND_BACKOFF_MS).len(), 1);
    }

    #[test]
    fn image_markers_and_flat_answer() {
        assert_eq!(flat_prompt("[Image #12] a [Image #x] b", 50).as_deref(), Some("a [Image #x] b"));
        assert_eq!(flat_answer("`  `").as_deref(), None);
    }

    // StatsTranscriptWiringTests und TranscriptCountTests.testEmptyDataCountsNothing

    #[test]
    fn empty_data_counts_nothing() {
        assert_eq!(user_messages(&[]), 0);
    }

    const MESSAGE: &str = "{\"type\":\"user\",\"message\":{\"content\":\"eine Nachricht\"}}
";

    fn refresh_once(reader: &mut TranscriptReader, dir: &TempDir, cache: &HashMap<String, Entry>) -> HashMap<String, Entry> {
        reader.refresh(&["11111111-2222-3333-4444-555555555555".to_string()], &dir.path(), cache, false, 0)
    }

    /// Ein Transcript zum ersten Mal zu lesen rechnet den alten Verlauf nicht nachträglich ein.
    #[test]
    fn first_read_counts_nothing() {
        let dir = TempDir::new("firstread");
        dir.write("projects/proj/11111111-2222-3333-4444-555555555555.jsonl", &MESSAGE.repeat(3));
        let mut reader = TranscriptReader::default();
        refresh_once(&mut reader, &dir, &HashMap::new());
        assert_eq!(reader.take_new_messages(), 0);
    }

    /// Nach `/clear` schrumpft die Datei: das ist kein Zuwachs und zählt nicht.
    #[test]
    fn shrunk_transcript_counts_nothing() {
        let dir = TempDir::new("shrunk");
        let file = "projects/proj/11111111-2222-3333-4444-555555555555.jsonl";
        dir.write(file, &MESSAGE.repeat(3));
        let mut reader = TranscriptReader::default();
        let cache = refresh_once(&mut reader, &dir, &HashMap::new());
        dir.write(file, MESSAGE);
        refresh_once(&mut reader, &dir, &cache);
        assert_eq!(reader.take_new_messages(), 0);
    }
}
