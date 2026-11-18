//! Was Tray-Icon, Tray-Menü und Badge zeigen, aus dem Zustand gebaut (Port von `StatusItemController.update`).
//! Reine Funktion: der Aufrufer vergleicht mit dem letzten Stand und schiebt nur Änderungen an das Icon.

use std::collections::BTreeSet;

use kadrell_core::SessionStatus;
use kadrell_tray::model::{self, Lang, MenuSession, StatusSummary, TrayItem};

use super::rules;
use crate::state::AppState;

#[derive(Clone, Debug, PartialEq)]
pub struct View {
    pub summary: StatusSummary,
    pub menu: Vec<TrayItem>,
    /// Wartende und ungesehen fertige Sessions (Taskleisten- bzw. Dock-Badge).
    pub badge: usize,
    /// Die wartenden Sessions in Baumreihenfolge (für „Nächste wartende Session“).
    pub waiting: Vec<String>,
}

/// Der Zustand für das Tray. Sessions ohne laufenden Prozess gelten nicht als wartend (`display_status`).
pub fn build(state: &AppState, lang: Lang, now: f64) -> View {
    let order = state.ordered_keys();
    let status_of = |key: &str| state.display_status(key);
    let waiting: BTreeSet<String> = order.iter().filter(|k| status_of(k) == Some(SessionStatus::Waiting)).cloned().collect();
    let working = order.iter().filter(|k| status_of(k) == Some(SessionStatus::Running)).count();
    let unseen: BTreeSet<String> = state.attention.unseen().iter().cloned().collect();
    let (waiting_rows, done_rows) = model::rows(&order, &waiting, &unseen);
    let summary = StatusSummary {
        waiting: waiting_rows.len(),
        working,
        done: done_rows.len(),
        display: rules::display(state.settings.status_bar()),
        lang,
    };
    let row = |key: &String| menu_session(state, key, now);
    let (w, d): (Vec<MenuSession>, Vec<MenuSession>) = (waiting_rows.iter().filter_map(row).collect(), done_rows.iter().filter_map(row).collect());
    View { summary, menu: model::build_menu(&w, &d, lang), badge: model::badge_count(&waiting, &unseen), waiting: waiting_rows }
}

fn menu_session(state: &AppState, key: &str, now: f64) -> Option<MenuSession> {
    let session = state.session(key)?;
    let group = state.store.groups.group_for_session(key).map(|g| g.name.clone()).unwrap_or_default();
    Some(MenuSession { key: key.to_string(), group, title: session.title(), elapsed: session.elapsed(now) })
}

/// Titel und Text einer Benachrichtigung: `<Gruppe> › <Titel>`, dazu die letzte Meldung der Session
/// (`AppDelegate.attention.notify`).
pub fn notification(state: &AppState, key: &str, waiting: bool, lang: Lang) -> Option<model::Notification> {
    let session = state.session(key)?;
    let group = state.store.groups.group_for_session(key).map(|g| g.name.clone()).unwrap_or_default();
    let message = state.registry.lock().unwrap_or_else(std::sync::PoisonError::into_inner).last_messages.get(key).cloned();
    Some(model::notification(key, &group, &session.title(), message.as_deref(), waiting, lang))
}
