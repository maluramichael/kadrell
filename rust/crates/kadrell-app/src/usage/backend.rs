//! Konten im Einstellungen-Dialog: die echte Liste aus dem [`Hub`], Wechseln, Entfernen und Hinzufügen als Aufträge an
//! den Hintergrund-Schritt. Die lokalen Modelle bleiben beim Mock (eigenes Paket).

use std::collections::HashSet;
use std::sync::Arc;

use gpui::{App, Global};

use super::hub::{Cmd, Hub};
use crate::ui::settings::backend::{AccountInfo, Backend, MockBackend, ModelInfo, ProviderStatus, PullInfo, SearchInfo};
use crate::ui::settings::model::Provider;

/// Der Hub der Shell, für Dialoge, die ihn nicht über die Shell erreichen.
pub struct HubGlobal(pub Arc<Hub>);

impl Global for HubGlobal {}

pub fn create(cx: &App) -> Option<Box<dyn Backend>> {
    let hub = cx.try_global::<HubGlobal>()?.0.clone();
    let models = crate::models::backend::create(cx).unwrap_or_else(|| Box::new(MockBackend::new()));
    Some(Box::new(LiveBackend::with_models(hub, models)))
}

/// Die Aufträge laufen erst im nächsten Schritt (bis zu zwei Sekunden): der Dialog zeigt Wechsel und Entfernen sofort,
/// bis der Schnappschuss nachzieht.
pub struct LiveBackend {
    hub: Arc<Hub>,
    /// Lokale Modelle: echt aus `kadrell-models` (`models::backend`), in Tests der Mock.
    models: Box<dyn Backend>,
    active: Option<String>,
    removed: HashSet<String>,
}

impl LiveBackend {
    pub fn new(hub: Arc<Hub>) -> LiveBackend {
        LiveBackend::with_models(hub, Box::new(MockBackend::new()))
    }

    pub fn with_models(hub: Arc<Hub>, models: Box<dyn Backend>) -> LiveBackend {
        hub.request(Cmd::RefreshEmail);
        LiveBackend { hub, models, active: None, removed: HashSet::new() }
    }
}

impl Backend for LiveBackend {
    fn accounts(&self) -> Vec<AccountInfo> {
        let view = self.hub.view();
        view.accounts
            .into_iter()
            .filter(|a| !self.removed.contains(&a.id))
            .map(|a| AccountInfo { email: a.email.unwrap_or(a.title), id: a.id, plan: a.plan })
            .collect()
    }

    fn active_account(&self) -> Option<String> {
        let view = self.hub.view();
        self.active.clone().or(view.active_id).filter(|id| !self.removed.contains(id))
    }

    fn current_email(&self) -> Option<String> {
        self.hub.view().current_email
    }

    fn switch_account(&mut self, id: &str) {
        self.active = Some(id.to_string());
        self.hub.request(Cmd::Switch(id.to_string()));
    }

    fn remove_account(&mut self, id: &str) {
        self.removed.insert(id.to_string());
        self.hub.request(Cmd::Remove(id.to_string()));
    }

    fn add_account(&mut self) {
        self.hub.request(Cmd::AddCurrent);
    }

    fn status(&self, provider: Provider) -> ProviderStatus {
        self.models.status(provider)
    }

    fn models(&self, provider: Provider) -> Vec<ModelInfo> {
        self.models.models(provider)
    }

    fn delete_model(&mut self, id: &str) {
        self.models.delete_model(id);
    }

    fn reload(&mut self, provider: Provider) {
        self.models.reload(provider);
    }

    fn available_models(&self) -> Vec<(String, String)> {
        self.models.available_models()
    }

    fn pulls(&self) -> Vec<PullInfo> {
        self.models.pulls()
    }

    fn pull(&mut self, name: &str) {
        self.models.pull(name);
    }

    fn cancel_pull(&mut self, id: &str) {
        self.models.cancel_pull(id);
    }

    fn search(&mut self, query: &str) {
        self.models.search(query);
    }

    fn search_state(&self) -> SearchInfo {
        self.models.search_state()
    }

    fn action_error(&self) -> Option<String> {
        self.models.action_error()
    }

    fn generation(&self) -> u64 {
        self.models.generation()
    }

    fn llama_detected(&self) -> Option<String> {
        self.models.llama_detected()
    }

    fn pi_default(&self) -> Option<String> {
        self.models.pi_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_the_fake_accounts_and_queues_commands() {
        let hub = Arc::new(Hub::fake(1_800_000_000.));
        let mut b = LiveBackend::new(hub.clone());
        let accounts = b.accounts();
        assert_eq!(accounts.len(), 2);
        assert_eq!(accounts[0].email, "anna@example.com");
        assert_eq!(accounts[0].plan.as_deref(), Some("Ultra"));
        assert_eq!(b.active_account().as_deref(), Some("a1"));
        b.switch_account("a2");
        assert_eq!(b.active_account().as_deref(), Some("a2"), "Wechsel zeigt der Dialog sofort");
        b.remove_account("a2");
        assert_eq!(b.accounts().len(), 1);
        assert_eq!(b.active_account(), None);
        // Der Fake-Hub arbeitet keine Aufträge ab: die echten Konten bleiben unberührt.
        hub.step(1_800_000_001., &crate::usage::hub::Config {
            account: kadrell_accounts::AccountSettings::default(),
            pace_warning: false,
            notice: false,
            lang: kadrell_accounts::Lang::De,
        });
        assert_eq!(hub.view().accounts.len(), 2);
    }

    #[test]
    fn models_come_from_the_models_backend() {
        use kadrell_models::{LocalModels, LocalProviderKind, LocalSettings};
        let fake = kadrell_models::http::fake::Fake::ok("{\"version\":\"1\",\"models\":[{\"name\":\"echt:1\"}]}");
        let models = LocalModels::new(Arc::new(fake), LocalSettings::default(), Default::default(), "/h", None);
        models.refresh_blocking(LocalProviderKind::Ollama);
        let backend: Box<dyn Backend> = Box::new(crate::models::backend::ModelsBackend::new(models));
        let b = LiveBackend::with_models(Arc::new(Hub::fake(1_800_000_000.)), backend);
        assert_eq!(b.models(Provider::Ollama)[0].name, "echt:1");
        assert_eq!(b.available_models()[0].0, "ollama|echt:1");
        assert!(!LiveBackend::new(Arc::new(Hub::fake(1_800_000_000.))).models(Provider::Ollama).is_empty(), "ohne Modelle gilt der Mock");
    }
}
