use super::*;
use kadrell_claude::folder_index::Use;
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
fn empty_idea_does_nothing() {
    let tmp = Tmp::new();
    let mut flow = flow_in(&tmp);
    flow.set_idea("   ");
    assert_eq!(confirm(&mut flow, HOME), Confirmed::Nothing);
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
    assert_eq!(confirm(&mut flow, HOME), Confirmed::Nothing, "ohne Treffer nichts starten");
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
