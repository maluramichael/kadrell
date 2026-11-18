//! Tests der Leiste am Zustand: claude-Prüfung, Fehler im Register, Klänge, Tipp.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use kadrell_claude::attention::TipText;
use kadrell_claude::cli::{ClaudeCli, StripMode};
use kadrell_core::{Paths, Profile};
use kadrell_tray::Sound;

use super::cli_check::{Health, missing, of_version_check};
use super::launch::LaunchEnv;
use super::{AppState, Parts};

struct Dir(PathBuf);

impl Dir {
    fn new() -> Dir {
        static N: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!("kadrell-bar-test-{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
        std::fs::create_dir_all(dir.join("work")).unwrap();
        Dir(dir)
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn state_in(dir: &Dir) -> AppState {
    let profile_dir = dir.0.join("profile");
    std::fs::create_dir_all(&profile_dir).unwrap();
    let paths = Paths::new(dir.0.join("home"), dir.0.join("support"), dir.0.join("tmp"));
    let profile = Profile { name: Some("test".into()), dir: profile_dir.clone(), temporary: false, pid: std::process::id() };
    let base = ClaudeCli::new(String::new(), BTreeMap::from([("PATH".to_string(), "x".to_string())]), StripMode::Broad);
    let claude = Some(ClaudeCli::new("claude", base.environment.clone(), StripMode::Broad));
    let env = LaunchEnv { claude, base, exe: "kadrell".into(), socket: "sock".into(), home: dir.0.join("home").to_string_lossy().into_owned() };
    AppState::new(Parts { paths, profile, lock: None, env, endpoint: profile_dir.join("kadrell.endpoint") })
}

fn work(dir: &Dir, name: &str) -> String {
    let p = dir.0.join("work").join(name);
    std::fs::create_dir_all(&p).unwrap();
    p.to_string_lossy().replace('\\', "/")
}

#[test]
fn a_missing_binary_is_a_blocking_error_until_it_is_rechecked() {
    let dir = Dir::new();
    let mut state = state_in(&dir);
    assert_eq!(state.shown_error(), None);
    assert!(state.apply_cli_health(missing("claude")));
    assert_eq!(state.shown_error().as_deref(), Some("claude: claude nicht gefunden"));
    assert!(state.registry_error_is_missing_binary());
    assert!(!state.apply_cli_health(missing("claude")), "unverändert: nichts neu zeichnen");
    assert!(state.apply_cli_health(Health::default()));
    assert_eq!(state.shown_error(), None);
    assert!(!state.registry_error_is_missing_binary());
}

#[test]
fn an_old_version_only_warns_and_never_blocks() {
    let dir = Dir::new();
    let mut state = state_in(&dir);
    state.apply_cli_health(of_version_check(&kadrell_claude::cli::VersionCheck::TooOld { found: (2, 1, 1), tested: (2, 1, 273) }));
    assert_eq!(state.shown_error(), None);
    assert!(state.version_warning.as_deref().is_some_and(|w| w.starts_with("claude 2.1.1: älter")));
    state.apply_cli_health(Health::default());
    assert_eq!(state.version_warning, None);
}

#[test]
fn a_failed_check_clears_the_warning_and_a_tile_error_still_shows() {
    let dir = Dir::new();
    let mut state = state_in(&dir);
    state.version_warning = Some("alt".into());
    state.apply_cli_health(of_version_check(&kadrell_claude::cli::VersionCheck::RunFailed { binary: "c".into(), first_line: "x".into() }));
    assert_eq!(state.version_warning, None);
    state.fail_registry("c: boom");
    assert_eq!(state.shown_error().as_deref(), Some("c: boom"));
    state.apply_cli_health(Health::default());
    state.error = Some("Kachel".into());
    assert_eq!(state.shown_error().as_deref(), Some("Kachel"));
}

#[test]
fn opening_and_removing_a_session_queue_their_sounds() {
    let dir = Dir::new();
    let mut state = state_in(&dir);
    let key = state.new_claude_session(&work(&dir, "alpha"), None, true);
    assert_eq!(state.take_sounds(), [Sound::Open]);
    state.remove_sessions(&[key]);
    assert_eq!(state.take_sounds(), [Sound::Close]);
    assert!(state.take_sounds().is_empty());
}

#[test]
fn the_first_waiting_tip_shows_once_and_a_click_dismisses_it() {
    let dir = Dir::new();
    let mut state = state_in(&dir);
    let effects = state.attention.show_tip_once_now("tip.waiting.shown", TipText::WaitingMeansYellow);
    assert!(!effects.is_empty());
    assert!(state.tip_text().is_some_and(|t| t.starts_with("Gelb heißt")));
    state.dismiss_tip();
    assert_eq!(state.tip_text(), None);
    assert!(state.attention.show_tip_once_now("tip.waiting.shown", TipText::WaitingMeansYellow).is_empty(), "nur beim ersten Mal");
    assert_eq!(state.tip_text(), None);
}
