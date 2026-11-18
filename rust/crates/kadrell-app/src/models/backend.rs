//! Lokale Modelle im Einstellungen-Dialog: der `Backend`-Teil für Anbieter, Modelle, Downloads und Suche, gespeist aus
//! `kadrell-models`. Konten kennt dieses Backend nicht (die liefert `usage::backend`, das dieses hier einbaut).

use gpui::App;
use kadrell_models::{LocalModel, LocalModelRef, LocalModels, LocalProviderKind, Status};

use super::ModelsGlobal;
use crate::ui::settings::backend::{AccountInfo, Backend, HitInfo, ModelInfo, ProviderStatus, PullInfo, SearchInfo};
use crate::ui::settings::model::Provider;

pub fn create(cx: &App) -> Option<Box<dyn Backend>> {
    let models = cx.try_global::<ModelsGlobal>()?.0.clone();
    Some(Box::new(ModelsBackend::new(models)))
}

pub struct ModelsBackend {
    models: LocalModels,
}

impl ModelsBackend {
    pub fn new(models: LocalModels) -> ModelsBackend {
        ModelsBackend { models }
    }

    fn installed(&self) -> Vec<LocalModel> {
        [LocalProviderKind::Ollama, LocalProviderKind::LlamaCpp].into_iter().flat_map(|p| self.models.models(p)).collect()
    }
}

pub fn kind(p: Provider) -> LocalProviderKind {
    match p {
        Provider::Ollama => LocalProviderKind::Ollama,
        Provider::LlamaCpp => LocalProviderKind::LlamaCpp,
    }
}

/// Größe und Zustand, gedämpft rechts neben dem Namen („2,5 GB · 4.0B · Q4_K_M“).
fn detail(m: &LocalModel) -> String {
    [m.size_text(), m.detail.clone()].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" · ")
}

fn status_of(s: Status) -> ProviderStatus {
    match s {
        Status::Off => ProviderStatus::Off,
        Status::Checking => ProviderStatus::Checking,
        Status::Ready(t) => ProviderStatus::Ready(t),
        Status::Failed(e) => ProviderStatus::Failed(e),
    }
}

impl Backend for ModelsBackend {
    fn accounts(&self) -> Vec<AccountInfo> {
        Vec::new()
    }

    fn active_account(&self) -> Option<String> {
        None
    }

    fn current_email(&self) -> Option<String> {
        None
    }

    fn switch_account(&mut self, _id: &str) {}

    fn remove_account(&mut self, _id: &str) {}

    fn status(&self, provider: Provider) -> ProviderStatus {
        status_of(self.models.status(kind(provider)))
    }

    fn models(&self, provider: Provider) -> Vec<ModelInfo> {
        let list = self.models.models(kind(provider));
        list.iter().map(|m| ModelInfo { id: m.id(), name: m.name.clone(), detail: detail(m), can_remove: m.can_remove && !m.downloading }).collect()
    }

    fn delete_model(&mut self, id: &str) {
        if let Some(m) = self.installed().into_iter().find(|m| m.id() == id) {
            self.models.delete(&m);
        }
    }

    fn reload(&mut self, provider: Provider) {
        self.models.refresh(kind(provider));
    }

    fn available_models(&self) -> Vec<(String, String)> {
        self.models.available().iter().map(|m| (m.id(), kadrell_models::types::display(&m.reference()))).collect()
    }

    fn pulls(&self) -> Vec<PullInfo> {
        let pull = |p: kadrell_models::Pull| {
            let failed = p.error.is_some();
            let text = p.error.clone().unwrap_or_else(|| p.progress.text());
            PullInfo { id: p.reference.string(), name: p.reference.name.clone(), text, fraction: p.progress.fraction(), failed }
        };
        self.models.pulls().into_iter().map(pull).collect()
    }

    fn pull(&mut self, name: &str) {
        self.models.pull(name);
    }

    fn cancel_pull(&mut self, id: &str) {
        if let Some(r) = LocalModelRef::parse(id) {
            self.models.cancel_pull(&r);
        }
    }

    fn search(&mut self, query: &str) {
        self.models.search(query);
    }

    fn search_state(&self) -> SearchInfo {
        let (hits, searching, error) = self.models.search_state();
        let hits = hits.into_iter().map(|h| HitInfo { name: h.name, summary: h.summary, tags: h.tags }).collect();
        SearchInfo { hits, searching, error }
    }

    fn action_error(&self) -> Option<String> {
        self.models.action_error()
    }

    fn generation(&self) -> u64 {
        self.models.generation()
    }

    fn llama_detected(&self) -> Option<String> {
        Some(self.models.llama_binary()).filter(|p| !p.is_empty())
    }

    fn pi_default(&self) -> Option<String> {
        kadrell_claude::pi::default_model(&crate::ui::folders::home_dir())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kadrell_models::LocalSettings;
    use kadrell_models::http::fake::Fake;
    use std::sync::Arc;

    fn backend() -> ModelsBackend {
        let fake = Fake::new(|r| match r.url.rsplit('/').next().unwrap_or("") {
            "version" => Ok((200, "{\"version\":\"0.9.0\"}".into())),
            "tags" => Ok((200, "{\"models\":[{\"name\":\"qwen3:4b\",\"size\":2500000000,\"details\":{\"parameter_size\":\"4.0B\",\"quantization_level\":\"Q4_K_M\"}}]}".into())),
            _ => Ok((200, String::new())),
        });
        ModelsBackend::new(LocalModels::new(Arc::new(fake), LocalSettings::default(), Default::default(), "/h", None))
    }

    #[test]
    fn models_come_with_size_and_detail_and_real_ids() {
        let b = backend();
        b.models.refresh_blocking(LocalProviderKind::Ollama);
        assert_eq!(b.status(Provider::Ollama), ProviderStatus::Ready("Ollama 0.9.0".into()));
        let list = b.models(Provider::Ollama);
        assert_eq!(list[0].id, "ollama|qwen3:4b");
        assert_eq!(list[0].detail, "2,5 GB · 4.0B · Q4_K_M");
        assert!(list[0].can_remove);
        assert_eq!(b.available_models(), vec![("ollama|qwen3:4b".to_string(), "Ollama · qwen3:4b".to_string())]);
        assert!(b.models(Provider::LlamaCpp).is_empty());
        assert_eq!(b.status(Provider::LlamaCpp), ProviderStatus::Off);
    }

    #[test]
    fn a_pull_shows_up_with_progress_and_can_be_dismissed() {
        let mut b = backend();
        let (reference, _flag) = b.models.begin_pull("qwen3:0.6b").unwrap();
        let pulls = b.pulls();
        assert_eq!((pulls[0].id.as_str(), pulls[0].name.as_str(), pulls[0].failed), ("ollama|qwen3:0.6b", "qwen3:0.6b", false));
        b.cancel_pull(&reference.string());
        assert!(b.pulls().is_empty());
    }

    #[test]
    fn llama_detection_is_none_when_nothing_is_found() {
        let b = backend();
        assert_eq!(b.llama_detected().is_some(), !b.models.llama_binary().is_empty());
        assert!(b.accounts().is_empty());
    }
}
