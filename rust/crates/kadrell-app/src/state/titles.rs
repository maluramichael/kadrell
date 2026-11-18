//! Lokaler Sessiontitel auf dem Zustand der App (Swift: `AppDelegate+SessionTitle.swift`, `AppDelegate+Control.swift`
//! `controlStatus`): jede Statusmeldung einer Session stößt `SessionTitler` an. Vor dem ersten Prompt wird das Modell
//! bereitgemacht (Ollama starten, aufwärmen), danach fragt ein Hintergrund-Thread einmal nach dem Titel (höchstens 8 s,
//! sonst bleibt es beim Titel von Claude Code bzw. beim ersten Prompt). Der Titel kommt über eine Warteschlange zurück
//! und wird beim nächsten Abgleich mit dem Register gesetzt.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use kadrell_claude::local_model::LocalModel;
use kadrell_claude::session_title::{self, README_LIMIT, SessionTitler, TitleAction, TitleCandidate};
use kadrell_core::Session;
use kadrell_models::{Feature, LocalModels};

use super::AppState;

/// Titler und fertige Titel, von den Hintergrund-Threads und dem Zustand geteilt.
#[derive(Clone, Default)]
pub struct Titles {
    titler: Arc<Mutex<SessionTitler>>,
    done: Arc<Mutex<Vec<(String, String)>>>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Was `SessionTitler` von einer Session braucht. Nur Claude-Sessions haben ein Transcript (Swift: `hasTranscript`).
pub fn candidate_of(s: &Session) -> TitleCandidate {
    TitleCandidate {
        id: s.id.clone(),
        cwd: s.cwd.clone(),
        first_prompt: s.first_prompt.clone(),
        has_transcript: !s.is_shell() && !s.is_remote() && !s.is_pi(),
        custom_name: s.custom_name.clone(),
        local_title: s.local_title.clone(),
    }
}

impl Titles {
    /// Aus jeder Statusmeldung einer Session (Swift: `sessionTitler.note`). Blockiert nie: Ollama starten, aufwärmen und
    /// fragen laufen auf eigenen Threads.
    pub fn note(&self, models: &LocalModels, s: &TitleCandidate, home: &str) {
        let active = models.active_model(Feature::SessionTitle).is_some();
        let mut titler = lock(&self.titler);
        match titler.note(s, active) {
            TitleAction::Nothing => {}
            TitleAction::WarmUp => warm_up(models.clone()),
            TitleAction::Ask => {
                let Some(prompt) = titler.begin(s, active) else { return };
                drop(titler);
                self.ask_later(models.for_feature(Feature::SessionTitle, Some(session_title::schema())), s, prompt, home);
            }
        }
    }

    fn ask_later(&self, model: impl LocalModel + Send + 'static, s: &TitleCandidate, prompt: String, home: &str) {
        let (done, id, cwd, home) = (self.done.clone(), s.id.clone(), s.cwd.clone(), home.to_string());
        std::thread::spawn(move || {
            let readme = |dir: &str| session_title::readme_head(dir, README_LIMIT);
            if let Some(title) = session_title::request(&cwd, &prompt, &model, &readme, &home) {
                lock(&done).push((id, title));
            }
        });
    }

    /// Fertige Titel seit dem letzten Abruf: Schlüssel der Session und Titel.
    pub fn take_done(&self) -> Vec<(String, String)> {
        std::mem::take(&mut *lock(&self.done))
    }
}

/// Vor dem ersten Prompt: Ollama bei Bedarf starten, dann das Modell einmal laden (Swift: `ensureRunning`, `warmUp`).
fn warm_up(models: LocalModels) {
    std::thread::spawn(move || {
        models.ensure_ollama_running(Feature::SessionTitle);
        models.warm_up(Feature::SessionTitle);
    });
}

impl AppState {
    /// Nach jeder Statusmeldung (`kadrell status`, Swift: `registry.sessions.first { $0.id == id }.map(sessionTitler.note)`).
    pub(super) fn note_title(&self, key: &str) {
        let Some(session) = self.store.sessions.get(key) else { return };
        self.titles.note(&self.models, &candidate_of(session), &self.home());
    }

    /// Fertige Titel in Sessions und Register eintragen (`registry.setLocalTitle`); ein vorhandener bleibt.
    pub(super) fn apply_titles(&mut self) {
        for (id, title) in self.titles.take_done() {
            self.store.sessions.set_local_title(&id, &title);
            lock(&self.registry).set_local_title(&id, &title);
        }
    }
}

#[cfg(test)]
mod tests;
