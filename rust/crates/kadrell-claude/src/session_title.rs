//! Prompt und Aufräumen für den lokalen Sessiontitel, dazu der Zustand von `SessionTitler`
//! (Swift: `Services/SessionTitle.swift`). Alles ist still: scheitert irgendetwas, bleibt es beim Titel von Claude Code
//! bzw. beim ersten Prompt. Der Sessionstart wartet nie auf das Modell.

use crate::local_model::{LocalModel, SESSION_TITLE_TIMEOUT};
use crate::{paths, text};
use serde_json::{Value, json};
use std::collections::HashSet;
use std::io::Read;

pub const MAX_LENGTH: usize = 60;
pub const README_LIMIT: usize = 400;

/// Beispiele ohne Projektnamen: die Gruppe in der Seitenleiste zeigt das Projekt schon, und ein kleines Modell macht
/// nach, womit die Beispiele anfangen. Sie sind erfunden, weil es sie sonst kopiert.
pub const SYSTEM: &str = r#"You name a coding session. Reply with JSON only: {"title": "<title>"}
The title has 2 to 5 words and says WHAT the session is about (topic only), like a commit subject.
The project is already shown next to the title: never start with or repeat the project or folder name.
Never copy the prompt. Never write "check", "look", "work on" or the folder path. Use the language of the prompt.
Examples: "Login timeout fix", "Newsletter-Versand", "CSV import"."#;

/// JSON-Schema für die strukturierte Antwort.
pub fn schema() -> Value {
    json!({"type": "object", "properties": {"title": {"type": "string"}}, "required": ["title"]})
}

pub fn user(cwd: &str, prompt: &str, readme: Option<&str>, home: &str) -> String {
    let mut out = format!(
        "Project folder: {}\nPath: {}\nPrompt: {}",
        paths::last_component(cwd),
        paths::short_path(cwd, home),
        prompt.chars().take(500).collect::<String>()
    );
    if let Some(r) = readme.filter(|r| !r.is_empty()) {
        out.push_str("\n\nREADME start:\n");
        out.push_str(r);
    }
    out
}

pub fn parse(reply: &str) -> Option<String> {
    clean(text::first_json_object(reply)?.get("title")?.as_str()?)
}

/// Eine Zeile, ohne Anführungszeichen und Schlusspunkt, höchstens `MAX_LENGTH` Zeichen. Leer = Fehler (nil).
pub fn clean(raw: &str) -> Option<String> {
    const QUOTES: &str = "\"'`„“”‚‘’«»";
    let strip = |c: char| (c.is_whitespace() && c != '\n' && c != '\r') || QUOTES.contains(c);
    let line = raw.lines().find(|l| !l.is_empty()).unwrap_or("");
    let t = line.trim_matches(strip).trim_end_matches('.').trim_matches(strip);
    if t.is_empty() { None } else { Some(t.chars().take(MAX_LENGTH).collect()) }
}

/// Anfang der `README.md` im Projektordner, nil ohne Datei.
pub fn readme_head(cwd: &str, limit: usize) -> Option<String> {
    let mut data = Vec::new();
    std::fs::File::open(format!("{cwd}/README.md")).ok()?.take(limit as u64 * 4).read_to_end(&mut data).ok()?;
    if data.is_empty() {
        return None;
    }
    Some(String::from_utf8_lossy(&data).chars().take(limit).collect())
}

/// Was `SessionTitler` von einer Session braucht (Felder von `Session` im Kern).
#[derive(Clone, Debug, PartialEq)]
pub struct TitleCandidate {
    pub id: String,
    pub cwd: String,
    pub first_prompt: Option<String>,
    /// Nur Claude-Sessions haben ein Transcript (keine Shell, kein Remote, kein Pi).
    pub has_transcript: bool,
    pub custom_name: Option<String>,
    pub local_title: Option<String>,
}

/// Was nach `note` zu tun ist.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TitleAction {
    Nothing,
    /// Vor dem ersten Prompt: Modell bereitmachen (Ollama starten, aufwärmen).
    WarmUp,
    /// Erste Eingabe liegt vor: `run` aufrufen.
    Ask,
}

#[derive(Default)]
pub struct SessionTitler {
    warmed: HashSet<String>,
    attempted: HashSet<String>,
}

impl SessionTitler {
    fn eligible(s: &TitleCandidate, active: bool) -> bool {
        s.has_transcript && s.custom_name.is_none() && s.local_title.is_none() && active
    }

    /// Aus jeder Statusmeldung einer Session: vor dem ersten Prompt Modell bereitmachen, danach einmal nach dem Titel fragen.
    /// `active`: die Funktion ist eingeschaltet und hat ein Modell.
    pub fn note(&mut self, s: &TitleCandidate, active: bool) -> TitleAction {
        if !Self::eligible(s, active) {
            TitleAction::Nothing
        } else if s.first_prompt.is_some() {
            TitleAction::Ask
        } else if self.warmed.insert(s.id.clone()) {
            TitleAction::WarmUp
        } else {
            TitleAction::Nothing
        }
    }

    /// Fragt höchstens einmal je Session (bis zum nächsten Start von Kadrell), Fehler jeder Art bleiben unsichtbar.
    /// Liefert den fertigen Titel. `readme` liest den README-Anfang eines Ordners.
    pub fn run(&mut self, s: &TitleCandidate, active: bool, model: &dyn LocalModel, readme: &dyn Fn(&str) -> Option<String>, home: &str) -> Option<String> {
        let prompt = self.begin(s, active)?;
        request(&s.cwd, &prompt, model, readme, home)
    }

    /// Der kurze, nicht blockierende Teil von `run`: prüft und merkt den Versuch vor. Liefert den ersten Prompt, mit dem
    /// `request` (auf einem Hintergrund-Thread) fragen soll; `None`, wenn nichts zu tun ist oder schon gefragt wurde.
    pub fn begin(&mut self, s: &TitleCandidate, active: bool) -> Option<String> {
        let prompt = s.first_prompt.as_deref().filter(|_| Self::eligible(s, active))?;
        self.attempted.insert(s.id.clone()).then(|| prompt.to_string())
    }
}

/// Der blockierende Teil von `run`: README lesen, das Modell fragen (höchstens `SESSION_TITLE_TIMEOUT`), Antwort
/// bereinigen. Jeder Fehler gibt `None`.
pub fn request(cwd: &str, prompt: &str, model: &dyn LocalModel, readme: &dyn Fn(&str) -> Option<String>, home: &str) -> Option<String> {
    let head = readme(cwd);
    let reply = model.ask(SYSTEM, &user(cwd, prompt, head.as_deref(), home), SESSION_TITLE_TIMEOUT).ok()?;
    parse(&reply)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::local_model::ModelError;
    use crate::paths::testdir::TempDir;
    use std::cell::Cell;
    use std::time::Duration;

    #[test]
    fn clean_takes_first_line_and_strips_quotes_and_full_stop() {
        assert_eq!(clean("  \"Kadrell Sessiontitel.\"\nzweite Zeile").as_deref(), Some("Kadrell Sessiontitel"));
        assert_eq!(clean("„Dark Mode einbauen“").as_deref(), Some("Dark Mode einbauen"));
    }

    #[test]
    fn clean_rejects_empty_and_caps_length() {
        assert_eq!(clean("  \"\"  "), None);
        assert_eq!(clean(""), None);
        assert_eq!(clean(&"a".repeat(200)).unwrap().chars().count(), 60);
    }

    #[test]
    fn parse_reads_title_from_json() {
        assert_eq!(parse(r#"{"title": "Hook-Fehler beheben."}"#).as_deref(), Some("Hook-Fehler beheben"));
        assert_eq!(parse("kein json"), None);
        assert_eq!(parse(r#"{"title": ""}"#), None);
    }

    #[test]
    fn readme_head_is_capped_and_missing_file_gives_none() {
        let dir = TempDir::new("title");
        assert_eq!(readme_head(&dir.path(), 400), None);
        dir.write("README.md", &"x".repeat(1000));
        assert_eq!(readme_head(&dir.path(), 400).unwrap().chars().count(), 400);
    }

    #[test]
    fn user_text_has_folder_short_path_prompt_and_readme() {
        let u = user("/h/me/proj", &"p".repeat(600), Some("README"), "/h/me");
        assert!(u.starts_with("Project folder: proj\nPath: ~/proj\nPrompt: pppp"));
        assert_eq!(u.lines().nth(2).unwrap().chars().count(), "Prompt: ".len() + 500, "Prompt auf 500 Zeichen gekürzt");
        assert!(u.ends_with("\n\nREADME start:\nREADME"));
        assert!(!user("/x", "y", None, "/h").contains("README"));
    }

    struct Model {
        reply: Result<String, ModelError>,
        asked: Cell<usize>,
    }

    impl LocalModel for Model {
        fn ask(&self, _: &str, _: &str, timeout: Duration) -> Result<String, ModelError> {
            assert_eq!(timeout, SESSION_TITLE_TIMEOUT);
            self.asked.set(self.asked.get() + 1);
            self.reply.clone()
        }
    }

    fn candidate(first_prompt: Option<&str>) -> TitleCandidate {
        TitleCandidate { id: "a".into(), cwd: "/tmp/proj".into(), first_prompt: first_prompt.map(String::from), has_transcript: true, custom_name: None, local_title: None }
    }

    #[test]
    fn titler_asks_once_after_first_prompt_and_reports_title() {
        let model = Model { reply: Ok(r#"{"title":"Sessiontitel bauen"}"#.into()), asked: Cell::new(0) };
        let mut titler = SessionTitler::default();
        let s = candidate(Some("baue den Titel"));
        let readme = |_: &str| Some("README".to_string());
        assert_eq!(titler.run(&s, true, &model, &readme, "/h").as_deref(), Some("Sessiontitel bauen"));
        assert_eq!(titler.run(&s, true, &model, &readme, "/h"), None);
        assert_eq!(model.asked.get(), 1);
    }

    #[test]
    fn titler_skips_shells_custom_names_and_existing_titles() {
        let model = Model { reply: Ok("{}".into()), asked: Cell::new(0) };
        let mut titler = SessionTitler::default();
        let mut shell = candidate(Some("x"));
        shell.has_transcript = false;
        let mut named = candidate(Some("x"));
        named.custom_name = Some("Eigener".into());
        let mut titled = candidate(Some("x"));
        titled.local_title = Some("Schon da".into());
        for s in [&shell, &named, &titled] {
            assert_eq!(titler.run(s, true, &model, &|_| None, "/h"), None);
            assert_eq!(titler.note(s, true), TitleAction::Nothing);
        }
        assert_eq!(titler.run(&candidate(Some("x")), false, &model, &|_| None, "/h"), None, "Funktion aus");
        assert_eq!(model.asked.get(), 0);
    }

    #[test]
    fn titler_failure_is_silent() {
        let model = Model { reply: Err(ModelError::Timeout), asked: Cell::new(0) };
        assert_eq!(SessionTitler::default().run(&candidate(Some("x")), true, &model, &|_| None, "/h"), None);
        assert_eq!(model.asked.get(), 1);
    }

    #[test]
    fn begin_marks_the_attempt_so_a_second_status_report_does_not_ask_again() {
        let mut titler = SessionTitler::default();
        let s = candidate(Some("baue den Titel"));
        assert_eq!(titler.begin(&s, true).as_deref(), Some("baue den Titel"));
        assert_eq!(titler.begin(&s, true), None);
        assert_eq!(titler.begin(&candidate(None), true), None, "ohne ersten Prompt nichts zu fragen");
    }

    #[test]
    fn note_warms_up_once_before_the_first_prompt() {
        let mut titler = SessionTitler::default();
        assert_eq!(titler.note(&candidate(None), true), TitleAction::WarmUp);
        assert_eq!(titler.note(&candidate(None), true), TitleAction::Nothing);
        assert_eq!(titler.note(&candidate(Some("jetzt")), true), TitleAction::Ask);
    }
}
