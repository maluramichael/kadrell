//! `claude`-Prüfung an der Shell (Swift: `AppDelegate.recheckCLI`, `recheckIfBroken`, `reloadViews`): beim Start,
//! über „Erneut prüfen“ im Leerzustand und beim Aktivieren des Fensters, solange noch ein Fehler steht. Die Prüfung
//! selbst steht in `state::cli_check`, hier nur Aufruf auf dem Hintergrund-Executor und die Tasten des Leerzustands.

use gpui::{Context, Window};
use kadrell_claude::cli::{INSTALL_DOCS_URL, VersionCheck};

use crate::Shell;
use crate::misc::empty::{Facts, Press};
use crate::state::cli_check::{self, Health};
use crate::workspace::auto::Filter;

/// `KADRELL_DEV_CLI=missing|failed|old|loading` zeigt die Zustände ohne echte Störung (für Screenshots).
fn dev_health() -> Option<Health> {
    let check = match std::env::var("KADRELL_DEV_CLI").ok()?.as_str() {
        "missing" => return Some(cli_check::missing("claude")),
        "failed" => VersionCheck::RunFailed { binary: "claude".into(), first_line: "Zugriff verweigert".into() },
        "old" => VersionCheck::TooOld { found: (2, 1, 1), tested: kadrell_claude::cli::MIN_VERSION },
        _ => return None,
    };
    Some(cli_check::of_version_check(&check))
}

/// `KADRELL_DEV_CLI=loading`: der erste Poll kommt erst nach einer Minute, der Leerzustand zeigt „Lade Sessions …“.
pub(crate) fn dev_loading() -> bool {
    std::env::var("KADRELL_DEV_CLI").is_ok_and(|v| v == "loading")
}

impl Shell {
    /// Beim Start: erste Prüfung und die Neuprüfung beim Aktivieren des Fensters (installiert man `claude` nach dem
    /// Start, verschwindet der Fehler ohne Neustart).
    pub(crate) fn start_cli_check(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        cx.observe_window_activation(window, |shell, window, cx| {
            if window.is_window_active() && shell.state.registry_error().is_some() {
                shell.recheck_cli(cx);
            }
        })
        .detach();
        self.recheck_cli(cx);
    }

    /// `claude` neu suchen und prüfen (Hintergrund), dann Fehler und Warnung eintragen.
    pub(crate) fn recheck_cli(&mut self, cx: &mut Context<Self>) {
        if let Some(health) = dev_health() {
            self.apply_cli_health(health, None, cx);
            return;
        }
        let previous = self.state.env.clone();
        let override_binary = self.state.settings.claude_binary();
        cx.spawn(async move |this, cx| {
            let done = cx.background_executor().spawn(async move { cli_check::recheck(&previous, &override_binary) }).await;
            let _ = this.update(cx, |shell, cx| shell.apply_cli_health(done.health, Some(done.env), cx));
        })
        .detach();
    }

    fn apply_cli_health(&mut self, health: Health, env: Option<crate::state::launch::LaunchEnv>, cx: &mut Context<Self>) {
        if let Some(env) = env {
            self.state.env.claude = env.claude;
        }
        if self.state.apply_cli_health(health) {
            self.rebuild();
        }
        cx.notify();
    }

    /// Was der Leerzustand wissen muss, um seinen Fall zu wählen.
    pub(crate) fn empty_facts(&self) -> Facts {
        let session_count = self.state.store.sessions.sessions.len();
        let filter = Filter::from_settings(&self.state.settings);
        let pool = if filter.all_sessions { session_count > 0 } else { !self.selection.items.is_empty() };
        Facts {
            error: self.state.registry_error(),
            missing_binary: self.state.registry_error_is_missing_binary(),
            polled: self.state.registry_polled(),
            session_count,
            auto_idle: self.ws.auto.on && pool,
        }
    }

    /// Erst nach einer Wahl (Taste im Leerzustand oder Einstellungen) erscheinen die Sprachtasten nicht mehr.
    /// `KADRELL_DEV_LANG_UNCHOSEN=1` zeigt sie für Screenshots trotzdem.
    pub(crate) fn language_chosen(&self) -> bool {
        self.state.settings.language_chosen() && std::env::var_os("KADRELL_DEV_LANG_UNCHOSEN").is_none()
    }

    /// Eine Taste des Leerzustands (Swift: `emptyHitRects`, Aktion beim Drücken).
    pub(crate) fn press_empty(&mut self, press: &Press, window: &mut Window, cx: &mut Context<Self>) {
        match press {
            Press::NewSession => self.new_session_action(window, cx),
            Press::Recheck => self.recheck_cli(cx),
            Press::CopyInstall => cx.write_to_clipboard(gpui::ClipboardItem::new_string(cli_check::install_command().to_string())),
            Press::OpenDocs => cx.open_url(INSTALL_DOCS_URL),
            Press::Language(language) => self.pick_language(*language, cx),
        }
    }

    fn pick_language(&mut self, language: kadrell_core::settings::Language, cx: &mut Context<Self>) {
        let mut settings = self.state.settings.clone();
        settings.set_language(language);
        self.state.apply_settings(settings);
        crate::lang::apply(&self.state.settings);
        cx.notify();
    }
}
