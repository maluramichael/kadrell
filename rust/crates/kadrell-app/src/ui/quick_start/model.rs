//! Schnellstart ohne Oberfläche: Kandidaten aus Gruppen, Sessions und Ordnerhistorie, die Texte des Dialogs und die
//! Wirkung von ⏎ und Esc auf die Zustandsmaschine `QuickStartFlow` aus `kadrell-claude` (Swift: `QuickStartModel` in
//! `UI/QuickStartView.swift`). Die Maschine selbst und ihre Regeln (Namenssuche, Modellfrage, bereinigter Prompt) liegen
//! im Kern; hier steht nur, was die Oberfläche daraus macht.

use kadrell_claude::folder_index::{self, FolderIndex};
use kadrell_claude::local_model::ModelError;
use kadrell_claude::quick_start::{
    Analysis, Cancelled, GroupInfo, ModelRequest, NOT_READABLE, Origin, OriginKind, Phase, QuickStartCandidate, QuickStartFlow, SKIPPED, SessionInfo, Source, Started, Submitted, build_index,
};
use kadrell_i18n::trd;
use kadrell_models::http::describe;

use crate::ui::folders::short_path;

/// Was der Index aus dem Zustand der App braucht (Kopie, damit er nach dem Repo-Scan neu entstehen kann).
#[derive(Clone, Debug, Default)]
pub struct Inputs {
    pub groups: Vec<GroupInfo>,
    /// Offene und geschlossene Sessions.
    pub sessions: Vec<SessionInfo>,
    pub home: String,
}

/// Kandidaten: Gruppen, Ordner früherer Sessions, benutzte Ordner und Git-Repos, je Pfad einmal, wichtigste zuerst.
pub fn candidates(inputs: &Inputs, index: &FolderIndex, now: f64) -> Vec<QuickStartCandidate> {
    let frecency = |p: &str| index.frecency(p, now);
    build_index(&inputs.groups, &inputs.sessions, &index.uses, &index.repos, &frecency, &folder_index::is_directory, &inputs.home)
}

// --- Texte ---

pub fn title() -> String {
    trd!("SCHNELLSTART")
}

pub fn idea_placeholder() -> String {
    trd!("Idee tippen, z. B. „In Kadrell habe ich die Idee, dass …“")
}

pub fn filter_placeholder() -> String {
    trd!("Anderes Projekt suchen")
}

pub fn first_prompt_label() -> String {
    trd!("ERSTER PROMPT")
}

pub fn empty_list() -> String {
    trd!("Kein Projekt gefunden.")
}

pub fn button(phase: Phase) -> String {
    if phase == Phase::Pick { trd!("Starten") } else { trd!("Weiter") }
}

pub fn hint(phase: Phase) -> String {
    match phase {
        Phase::Idea => trd!("⏎ Projekt erkennen · Esc schließen"),
        Phase::Thinking => trd!("⏎ ohne Modell weiter · Esc zurück"),
        Phase::Pick => trd!("⏎ starten · ↑↓ oder tippen wählt ein anderes Projekt · Esc zurück"),
    }
}

/// Hinweis unter dem Ideenfeld, solange getippt wird.
pub fn live_text(flow: &QuickStartFlow, home: &str) -> String {
    if let Some(top) = flow.live.top().filter(|_| flow.live.top_strength > 0) {
        return trd!("erkannt: %@ · %@", top.name.clone(), short_path(&top.path, home));
    }
    match &flow.model_name {
        Some(name) => trd!("kein Name im Text · %@ hilft beim Zuordnen", name.clone()),
        None => trd!("kein Name im Text · ohne lokales Modell zuletzt benutzte zuerst"),
    }
}

pub fn thinking_text(model: &str) -> String {
    trd!("frage %@ …", model.to_string())
}

/// Warum das Modell nicht half: die zwei festen Gründe des Kerns übersetzt, Fehlertexte so, wie sie kommen.
fn failure_text(why: &str) -> String {
    match why {
        NOT_READABLE => trd!("Antwort nicht lesbar"),
        SKIPPED => trd!("übersprungen"),
        other => other.to_string(),
    }
}

/// Zeile über der Liste: woher die Vorauswahl kommt.
pub fn origin_text(origin: &Origin) -> String {
    let m = origin.model.clone();
    let base = match origin.kind {
        OriginKind::NameConfident => trd!("erkannt am Namen im Text"),
        OriginKind::NameAmbiguous => trd!("mehrere Projekte passen zum Namen"),
        OriginKind::NameWeak => trd!("ähnlicher Name im Text, bitte prüfen"),
        OriginKind::NoName => trd!("kein Projekt im Text erkannt · zuletzt benutzte zuerst"),
        OriginKind::ModelSure => trd!("erkannt von %@", m),
        OriginKind::ModelUnsure => trd!("%@ vermutet, bitte prüfen", m),
        OriginKind::ModelNoProject => trd!("%@ findet kein passendes Projekt · zuletzt benutzte zuerst", m),
    };
    match &origin.failure {
        Some(why) => format!("{base} · {}", trd!("Modell: %@", failure_text(why))),
        None => base,
    }
}

/// Beschriftung über dem ersten Prompt, wenn das Modell einen bereinigten angeboten hat.
pub fn cleaned_label(use_cleaned: bool) -> String {
    if use_cleaned { trd!("Projektverweis entfernt · Tab: Originaltext") } else { trd!("Originaltext · Tab: ohne Projektverweis") }
}

/// Rechts in der Zeile: woher der Kandidat kommt.
pub fn source_text(c: &QuickStartCandidate) -> String {
    if c.sessions > 0 {
        return trd!("%lld Sessions", i64::from(c.sessions));
    }
    match c.source {
        Source::Group => trd!("Gruppe"),
        Source::Session | Source::Folder => trd!("benutzter Ordner"),
        Source::Repo => "Git".to_string(),
    }
}

/// Fehlermeldung, wenn der gewählte Ordner nicht mehr da ist.
pub fn missing_text(path: &str, home: &str) -> String {
    trd!("%@ existiert nicht mehr", short_path(path, home))
}

// --- Wirkung der Tasten ---

/// Was ⏎ (oder ein Klick auf die Haupttaste) bewirkt hat.
#[derive(Clone, Debug, PartialEq)]
pub enum Confirmed {
    /// Nichts zu tun (Klick neben die Liste): kein Ton, es bleibt wie es ist.
    Nothing,
    /// Die Eingabe bewirkt nichts (leere Idee, kein Kandidat zum Starten): die Oberfläche piept (`NSSound.beep()`).
    Beep,
    /// Die Projektwahl steht: Fokus auf das Suchfeld.
    ShowPick,
    /// Die Phase „Denken“ beginnt: die Oberfläche stellt die Frage ans Modell und gibt die Antwort an `model_reply`.
    Ask(ModelRequest),
    /// Hier startet die Session.
    Start { path: String, group_id: Option<String>, prompt: String },
    /// Der Ordner ist weg (die Oberfläche zeigt den Text und piept).
    Missing(String),
}

fn started(s: Started) -> Confirmed {
    match s {
        Started::Go(c, prompt) => Confirmed::Start { path: c.path, group_id: c.group_id, prompt },
        Started::Beep => Confirmed::Beep,
        Started::Missing => Confirmed::Nothing,
    }
}

/// ⏎: Idee analysieren, Modellwarten überspringen oder starten. Fragt das Modell (`Ask`), läuft die Antwort über
/// [`model_reply`].
pub fn confirm(flow: &mut QuickStartFlow, home: &str) -> Confirmed {
    match flow.submit() {
        Submitted::Analysis(Analysis::Beep) => Confirmed::Beep,
        Submitted::Analysis(Analysis::Done) => Confirmed::ShowPick,
        Submitted::Analysis(Analysis::AskModel(request)) => Confirmed::Ask(request),
        Submitted::Start(Started::Missing) => Confirmed::Missing(pick_path(flow).map(|p| missing_text(&p, home)).unwrap_or_default()),
        Submitted::Start(s) => started(s),
    }
}

/// Antwort des Modells (oder Fehler) in den Ablauf geben. Fehler kommen in der eingestellten Sprache an (`describe`,
/// Swift: `LocalModelError.describe`), der Kern zeigt sie dann unverändert hinter „Modell: “.
pub fn model_reply(flow: &mut QuickStartFlow, reply: Result<String, ModelError>) {
    flow.model_reply(reply.map_err(|e| ModelError::Invalid(describe(&e))));
}

/// Klick auf eine Zeile der Liste.
pub fn pick(flow: &mut QuickStartFlow, i: usize, home: &str) -> Confirmed {
    match flow.pick(i) {
        Some(Started::Missing) => Confirmed::Missing(pick_path(flow).map(|p| missing_text(&p, home)).unwrap_or_default()),
        Some(s) => started(s),
        None => Confirmed::Nothing,
    }
}

fn pick_path(flow: &QuickStartFlow) -> Option<String> {
    flow.list.get(flow.selected).map(|c| c.path.clone())
}

/// Esc.
pub fn cancel(flow: &mut QuickStartFlow) -> Cancelled {
    flow.cancel()
}

#[cfg(test)]
mod tests;
