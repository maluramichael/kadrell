use super::*;
use kadrell_claude::folder_index::Use;
use kadrell_claude::local_model::ModelError;
use kadrell_claude::quick_start::Cancelled;

const HOME: &str = "/home/m";

/// Die Texte gelten deutsch, egal in welcher Sprache der Rechner läuft.
fn de() {
    kadrell_i18n::set_thread_language(kadrell_i18n::Language::De);
}

/// Wegwerfordner mit echten Unterordnern, beim Drop gelöscht.
struct Tmp(std::path::PathBuf);

impl Tmp {
    fn new() -> Tmp {
        de();
        let p = std::env::temp_dir().join(format!("kadrell-qs-{}", std::process::id())).join(format!("{:?}", std::thread::current().id()).replace(['(', ')'], ""));
        std::fs::create_dir_all(&p).unwrap();
        Tmp(p)
    }

    fn dir(&self, name: &str) -> String {
        let p = self.0.join(name);
        std::fs::create_dir_all(&p).unwrap();
        p.to_string_lossy().replace('\\', "/")
    }
}

impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn candidate(path: &str, name: &str, source: Source, score: f64) -> QuickStartCandidate {
    QuickStartCandidate { score, ..QuickStartCandidate::new(path, name, source, vec![name.to_string()]) }
}

/// Zwei echte Ordner: `kadrell` (Gruppe) und `lulububu` (Repo).
fn flow_in(tmp: &Tmp) -> QuickStartFlow {
    let a = candidate(&tmp.dir("kadrell"), "kadrell", Source::Group, 2.0);
    let b = candidate(&tmp.dir("lulububu"), "lulububu", Source::Repo, 1.0);
    QuickStartFlow::new(vec![a, b], None, HOME)
}

#[test]
fn every_text_of_the_dialog_is_in_the_catalog() {
    let all = [
        "SCHNELLSTART", "Idee tippen, z. B. „In Kadrell habe ich die Idee, dass …“", "Anderes Projekt suchen", "ERSTER PROMPT", "Kein Projekt gefunden.", "Starten", "Weiter",
        "⏎ Projekt erkennen · Esc schließen", "⏎ ohne Modell weiter · Esc zurück", "⏎ starten · ↑↓ oder tippen wählt ein anderes Projekt · Esc zurück", "erkannt: %@ · %@",
        "kein Name im Text · %@ hilft beim Zuordnen", "kein Name im Text · ohne lokales Modell zuletzt benutzte zuerst", "frage %@ …", "Antwort nicht lesbar", "übersprungen",
        "erkannt am Namen im Text", "mehrere Projekte passen zum Namen", "ähnlicher Name im Text, bitte prüfen", "kein Projekt im Text erkannt · zuletzt benutzte zuerst", "erkannt von %@",
        "%@ vermutet, bitte prüfen", "%@ findet kein passendes Projekt · zuletzt benutzte zuerst", "Modell: %@", "Projektverweis entfernt · Tab: Originaltext",
        "Originaltext · Tab: ohne Projektverweis", "%lld Sessions", "Gruppe", "benutzter Ordner", "%@ existiert nicht mehr", "Schnellstart: Idee rein, Session raus",
        "Alle Claude-Sessions durchsuchen, auch beendete", "Verlauf wird eingelesen …", "beim ersten Mal einmalig alle Transcripts, danach nur neue", "Session lässt sich nicht fortsetzen",
        "Den Ordner %@ gibt es nicht mehr.", "„%@“ läuft noch außerhalb dieses Kadrell (pid %lld). Erst dort beenden.", "Das Transcript von „%@“ ist nicht mehr da.",
    ];
    let missing: Vec<&str> = all.iter().copied().filter(|s| kadrell_i18n::id_for_de(s).is_none()).collect();
    assert!(missing.is_empty(), "nicht im Katalog: {missing:?}");
}

#[test]
fn candidates_rank_group_before_session_before_folder_before_repo_and_drop_missing_folders() {
    let tmp = Tmp::new();
    let (g, s, f, r, gone) = (tmp.dir("group"), tmp.dir("sess"), tmp.dir("used"), tmp.dir("repo"), format!("{}/gone", tmp.dir("x")));
    let inputs = Inputs {
        groups: vec![GroupInfo { id: "g1".into(), name: "Gruppe".into(), cwd: g.clone(), host: None }],
        sessions: vec![SessionInfo { cwd: s.clone(), started_at_ms: 1_000.0, is_remote: false, is_shell: false, title: "T".into() }, SessionInfo { cwd: gone.clone(), started_at_ms: 1.0, is_remote: false, is_shell: false, title: "X".into() }],
        home: HOME.into(),
    };
    let mut index = FolderIndex::load("/nonexistent/profile", HOME);
    index.uses.insert(f.clone(), Use { count: 3, last: 100.0 });
    index.repos = vec![r.clone()];
    let list = candidates(&inputs, &index, 200.0);
    let by_path = |p: &str| list.iter().find(|c| c.path == p).map(|c| (c.source, c.group_id.clone()));
    assert_eq!(by_path(&g), Some((Source::Group, Some("g1".into()))));
    assert_eq!(by_path(&s).map(|x| x.0), Some(Source::Session));
    assert_eq!(by_path(&f).map(|x| x.0), Some(Source::Folder));
    assert_eq!(by_path(&r).map(|x| x.0), Some(Source::Repo));
    assert_eq!(by_path(&gone), None, "gelöschte Ordner fallen weg");
    assert!(list.iter().find(|c| c.path == f).unwrap().score > list.iter().find(|c| c.path == r).unwrap().score, "benutzt schlägt nur gefunden");
}

#[test]
fn idea_with_a_project_name_preselects_it_and_enter_starts_with_the_idea() {
    let tmp = Tmp::new();
    let mut flow = flow_in(&tmp);
    flow.set_idea("In Kadrell habe ich die Idee, dass alles schneller wird");
    assert!(live_text(&flow, HOME).starts_with("erkannt: kadrell"), "{}", live_text(&flow, HOME));
    assert_eq!(confirm(&mut flow, HOME), Confirmed::ShowPick);
    assert_eq!(flow.phase, Phase::Pick);
    assert_eq!(flow.list[0].name, "kadrell");
    assert_eq!(origin_text(flow.origin.as_ref().unwrap()), "erkannt am Namen im Text");
    match confirm(&mut flow, HOME) {
        Confirmed::Start { path, prompt, .. } => {
            assert!(path.ends_with("/kadrell"));
            assert_eq!(prompt, "In Kadrell habe ich die Idee, dass alles schneller wird");
        }
        other => panic!("erwartet Start: {other:?}"),
    }
}

#[test]
fn empty_idea_beeps() {
    let tmp = Tmp::new();
    let mut flow = flow_in(&tmp);
    flow.set_idea("   ");
    assert_eq!(confirm(&mut flow, HOME), Confirmed::Beep);
    assert_eq!(flow.phase, Phase::Idea);
}

#[test]
fn arrows_and_typing_choose_another_project() {
    let tmp = Tmp::new();
    let mut flow = flow_in(&tmp);
    flow.set_idea("In Kadrell habe ich eine Idee");
    confirm(&mut flow, HOME);
    flow.move_selection(1);
    assert_eq!(flow.selected, 1);
    flow.move_selection(5);
    assert_eq!(flow.selected, 1, "bleibt in der Liste");
    flow.set_filter("lulu");
    assert_eq!((flow.list.len(), flow.selected, flow.list[0].name.as_str()), (1, 0, "lulububu"));
    assert!(matches!(confirm(&mut flow, HOME), Confirmed::Start { path, .. } if path.ends_with("/lulububu")));
    flow.set_filter("zzzzzz");
    assert!(flow.list.is_empty());
    assert_eq!(confirm(&mut flow, HOME), Confirmed::Beep, "ohne Treffer nichts starten, nur piepen");
}

#[test]
fn without_a_name_in_the_text_the_most_used_come_first() {
    let tmp = Tmp::new();
    let mut flow = flow_in(&tmp);
    flow.set_idea("Eine Idee ohne Namen");
    confirm(&mut flow, HOME);
    assert_eq!(flow.list[0].name, "kadrell");
    assert_eq!(origin_text(flow.origin.as_ref().unwrap()), "kein Projekt im Text erkannt · zuletzt benutzte zuerst");
}

#[test]
fn tab_toggles_only_when_the_model_offered_a_cleaned_prompt() {
    let tmp = Tmp::new();
    let mut flow = flow_in(&tmp);
    flow.set_idea("In Kadrell habe ich die Idee, dass alles schneller wird");
    confirm(&mut flow, HOME);
    assert!(!flow.toggle_cleaned(), "ohne Modell gibt es keinen bereinigten Prompt");
    flow.cleaned = Some("alles soll schneller werden".into());
    assert_eq!(flow.prompt(), "alles soll schneller werden");
    assert!(flow.toggle_cleaned());
    assert_eq!(flow.prompt(), "In Kadrell habe ich die Idee, dass alles schneller wird");
    assert_eq!(cleaned_label(false), "Originaltext · Tab: ohne Projektverweis");
}

#[test]
fn escape_goes_back_one_step_then_closes() {
    let tmp = Tmp::new();
    let mut flow = flow_in(&tmp);
    flow.set_idea("In Kadrell eine Idee");
    confirm(&mut flow, HOME);
    assert_eq!(cancel(&mut flow), Cancelled::Back);
    assert_eq!(flow.phase, Phase::Idea);
    assert_eq!(flow.idea(), "In Kadrell eine Idee", "die Idee bleibt stehen");
    assert_eq!(cancel(&mut flow), Cancelled::Close);
}

#[test]
fn a_folder_that_is_gone_is_reported_not_started() {
    de();
    let gone = "/nonexistent/kadrell-weg/projekt";
    let mut flow = QuickStartFlow::new(vec![candidate(gone, "projekt", Source::Group, 1.0)], None, HOME);
    flow.set_idea("Idee für projekt");
    confirm(&mut flow, HOME);
    assert_eq!(confirm(&mut flow, HOME), Confirmed::Missing("/nonexistent/kadrell-weg/projekt existiert nicht mehr".into()));
    assert_eq!(flow.phase, Phase::Pick, "die Wahl bleibt offen");
}

#[test]
fn clicking_a_row_starts_there() {
    let tmp = Tmp::new();
    let mut flow = flow_in(&tmp);
    flow.set_idea("Eine Idee");
    confirm(&mut flow, HOME);
    assert!(matches!(pick(&mut flow, 1, HOME), Confirmed::Start { path, .. } if path.ends_with("/lulububu")));
    assert_eq!(pick(&mut flow, 9, HOME), Confirmed::Nothing);
}

#[test]
fn texts_follow_phase_and_candidate_source() {
    de();
    assert_eq!(button(Phase::Idea), "Weiter");
    assert_eq!(button(Phase::Pick), "Starten");
    assert!(hint(Phase::Thinking).starts_with("⏎ ohne Modell"));
    let mut c = candidate("/x/a", "a", Source::Repo, 0.0);
    assert_eq!(source_text(&c), "Git");
    c.source = Source::Folder;
    assert_eq!(source_text(&c), "benutzter Ordner");
    c.sessions = 3;
    assert_eq!(source_text(&c), "3 Sessions");
}

// --- Mit lokalem Modell (Swift: QuickStartModel mit `ask`) ---

fn flow_with_model(tmp: &Tmp, strip_reference: bool) -> QuickStartFlow {
    let a = candidate(&tmp.dir("kadrell"), "kadrell", Source::Group, 2.0);
    let b = candidate(&tmp.dir("lulububu"), "lulububu", Source::Repo, 1.0);
    let mut flow = QuickStartFlow::new(vec![a, b], Some("qwen3:4b"), HOME);
    flow.strip_reference = strip_reference;
    flow
}

fn asked(flow: &mut QuickStartFlow, idea: &str) -> ModelRequest {
    flow.set_idea(idea);
    match confirm(flow, HOME) {
        Confirmed::Ask(request) => request,
        other => panic!("Modellfrage erwartet, bekommen {other:?}"),
    }
}

#[test]
fn without_a_clear_name_the_model_is_asked_and_its_pick_goes_first() {
    let tmp = Tmp::new();
    let mut flow = flow_with_model(&tmp, true);
    let request = asked(&mut flow, "Der Roboter soll beim Start piepen");
    assert_eq!(flow.phase, Phase::Thinking);
    assert!(request.user.contains("Idea: Der Roboter soll beim Start piepen") && request.user.contains("0: kadrell"));
    model_reply(&mut flow, Ok(r#"{"index": 1, "sure": true, "prompt": "Der Roboter soll beim Start piepen"}"#.into()));
    assert_eq!(flow.phase, Phase::Pick);
    assert_eq!(flow.list[0].name, "lulububu", "die Wahl des Modells steht vorn");
    assert_eq!(origin_text(flow.origin.as_ref().unwrap()), "erkannt von qwen3:4b");
}

#[test]
fn an_unsure_model_asks_for_a_check_and_no_project_is_said_so() {
    let tmp = Tmp::new();
    let mut flow = flow_with_model(&tmp, true);
    asked(&mut flow, "Irgendeine Idee");
    model_reply(&mut flow, Ok(r#"{"index": 0, "sure": false, "prompt": "x"}"#.into()));
    assert_eq!(origin_text(flow.origin.as_ref().unwrap()), "qwen3:4b vermutet, bitte prüfen");
    let mut again = flow_with_model(&tmp, true);
    asked(&mut again, "Irgendeine Idee");
    model_reply(&mut again, Ok(r#"{"index": -1, "sure": false, "prompt": "x"}"#.into()));
    assert_eq!(origin_text(again.origin.as_ref().unwrap()), "qwen3:4b findet kein passendes Projekt · zuletzt benutzte zuerst");
}

#[test]
fn model_failures_are_shown_behind_the_name_hint_and_never_block() {
    let tmp = Tmp::new();
    let mut flow = flow_with_model(&tmp, true);
    asked(&mut flow, "Irgendeine Idee");
    model_reply(&mut flow, Err(ModelError::Timeout));
    assert_eq!(flow.phase, Phase::Pick);
    assert_eq!(origin_text(flow.origin.as_ref().unwrap()), "kein Projekt im Text erkannt · zuletzt benutzte zuerst · Modell: keine Antwort in der Zeit");
    let mut unreadable = flow_with_model(&tmp, true);
    asked(&mut unreadable, "Irgendeine Idee");
    model_reply(&mut unreadable, Ok("weiß nicht".into()));
    assert!(origin_text(unreadable.origin.as_ref().unwrap()).ends_with("Modell: Antwort nicht lesbar"));
    let mut down = flow_with_model(&tmp, true);
    asked(&mut down, "Irgendeine Idee");
    model_reply(&mut down, Err(ModelError::Unreachable("127.0.0.1".into())));
    assert!(origin_text(down.origin.as_ref().unwrap()).ends_with("Modell: nicht erreichbar: 127.0.0.1"));
}

#[test]
fn enter_while_thinking_skips_the_model_and_a_late_reply_is_ignored() {
    let tmp = Tmp::new();
    let mut flow = flow_with_model(&tmp, true);
    asked(&mut flow, "Irgendeine Idee");
    assert_eq!(confirm(&mut flow, HOME), Confirmed::ShowPick);
    assert!(origin_text(flow.origin.as_ref().unwrap()).ends_with("Modell: übersprungen"));
    let before = flow.list.clone();
    model_reply(&mut flow, Ok(r#"{"index": 1, "sure": true, "prompt": "x"}"#.into()));
    assert_eq!(flow.list, before, "die späte Antwort ändert nichts");
}

#[test]
fn typing_or_escape_while_thinking_goes_back_to_the_idea() {
    let tmp = Tmp::new();
    let mut flow = flow_with_model(&tmp, true);
    asked(&mut flow, "Irgendeine Idee");
    flow.set_idea("Irgendeine andere Idee");
    assert_eq!(flow.phase, Phase::Idea, "Tippen bricht die Frage ab");
    asked(&mut flow, "Noch eine Idee");
    assert_eq!(cancel(&mut flow), Cancelled::Back);
    assert_eq!(flow.phase, Phase::Idea);
}

#[test]
fn strip_reference_setting_decides_whether_a_clear_name_still_asks_and_whether_the_cleaned_prompt_counts() {
    let tmp = Tmp::new();
    let idea = "In Kadrell habe ich die Idee, dass alles schneller wird";
    let mut off = flow_with_model(&tmp, false);
    off.set_idea(idea);
    assert_eq!(confirm(&mut off, HOME), Confirmed::ShowPick, "eindeutiger Name und Einstellung aus: kein Modell");
    let mut on = flow_with_model(&tmp, true);
    asked(&mut on, idea);
    model_reply(&mut on, Ok(r#"{"index": 0, "sure": true, "prompt": "alles schneller wird"}"#.into()));
    assert_eq!(on.cleaned.as_deref(), Some("alles schneller wird"), "Projektverweis entfernt");
    assert_eq!(on.prompt(), "alles schneller wird");
    assert_eq!(cleaned_label(on.use_cleaned), "Projektverweis entfernt · Tab: Originaltext");
}
